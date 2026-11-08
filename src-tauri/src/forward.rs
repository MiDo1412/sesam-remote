//! Portweiterleitungen einer Shell-Sitzung, Schreibweise wie bei PuTTY/OpenSSH:
//! `L8080 db:5432` (lokal → über den Server), `R9000 localhost:3000` (Server → hier), `D1080` (SOCKS5).
//! Lokale Ports lauschen nur auf 127.0.0.1, damit niemand im Netz den Tunnel mitbenutzt.

use crate::ssh::Client;
use russh::client::Handle;
use std::fmt;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    Local { listen: u16, host: String, port: u16 },
    Remote { listen: u16, host: String, port: u16 },
    Dynamic { listen: u16 },
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Rule::Local { listen, host, port } => write!(f, "localhost:{listen} → {host}:{port} (über den Server)"),
            Rule::Remote { listen, host, port } => write!(f, "Server-Port {listen} → {host}:{port} (hier)"),
            Rule::Dynamic { listen } => write!(f, "SOCKS5-Proxy localhost:{listen}"),
        }
    }
}

fn port(s: &str) -> Result<u16, String> {
    s.parse::<u16>().ok().filter(|p| *p != 0).ok_or_else(|| format!("Ungültiger Port «{s}»"))
}

/// Eine Zeile wie `L8080 localhost:80`, `R9000 host:3000` oder `D1080`. IPv6-Ziele in Klammern: `[::1]:80`.
pub fn parse(line: &str) -> Result<Rule, String> {
    let line = line.trim();
    let bad = || format!("Weiterleitung «{line}» nicht verstanden (Beispiele: L8080 localhost:80, R9000 localhost:3000, D1080)");
    let kind = line.chars().next().ok_or_else(bad)?.to_ascii_uppercase();
    let rest = line[1..].trim_start();
    let (listen, dest) = match rest.split_once(char::is_whitespace) {
        Some((l, t)) => (l, Some(t.trim())),
        None => (rest, None),
    };
    let listen = port(listen)?;
    let target = || -> Result<(String, u16), String> {
        let (h, p) = dest.and_then(|t| t.rsplit_once(':')).ok_or_else(bad)?;
        let h = h.trim_start_matches('[').trim_end_matches(']');
        if h.is_empty() || h.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(bad());
        }
        Ok((h.to_string(), port(p)?))
    };
    Ok(match kind {
        'L' => {
            let (host, port) = target()?;
            Rule::Local { listen, host, port }
        }
        'R' => {
            let (host, port) = target()?;
            Rule::Remote { listen, host, port }
        }
        'D' if dest.is_none() => Rule::Dynamic { listen },
        _ => return Err(bad()),
    })
}

/// Laufende Weiterleitungen; enden mit dem Drop (Shell geschlossen).
#[derive(Default)]
pub struct Forwards(Vec<tokio::task::JoinHandle<()>>);

impl Drop for Forwards {
    fn drop(&mut self) {
        self.0.iter().for_each(|t| t.abort());
    }
}

/// Lokale (L) und dynamische (D) Weiterleitungen öffnen und Fernweiterleitungen (R) beim Server anmelden.
/// Die Verbindungen von R nimmt `ssh::Client` entgegen. `note` meldet Erfolg und Fehler je Regel.
pub async fn start(h: &Arc<Handle<Client>>, rules: &[Rule], note: impl Fn(String)) -> Forwards {
    let mut out = Forwards::default();
    for rule in rules {
        let listen = match rule {
            Rule::Remote { listen, .. } => {
                match h.tcpip_forward("127.0.0.1", u32::from(*listen)).await {
                    Ok(_) => note(format!("Weiterleitung aktiv: {rule}")),
                    Err(e) => note(format!("Weiterleitung {rule} abgelehnt: {e}")),
                }
                continue;
            }
            Rule::Local { listen, .. } | Rule::Dynamic { listen } => *listen,
        };
        let l = match TcpListener::bind(("127.0.0.1", listen)).await {
            Ok(l) => l,
            Err(e) => {
                note(format!("Weiterleitung {rule} nicht möglich: {e}"));
                continue;
            }
        };
        note(format!("Weiterleitung aktiv: {rule}"));
        let (h, rule) = (h.clone(), rule.clone());
        out.0.push(tokio::spawn(async move {
            while let Ok((mut tcp, peer)) = l.accept().await {
                let (h, rule) = (h.clone(), rule.clone());
                tokio::spawn(async move {
                    let socks = matches!(rule, Rule::Dynamic { .. });
                    let target = match rule {
                        Rule::Local { host, port, .. } => Ok((host, port)),
                        _ => socks5_request(&mut tcp).await,
                    };
                    let Ok((host, port)) = target else { return };
                    let ch = h.channel_open_direct_tcpip(host.as_str(), port.into(), peer.ip().to_string(), peer.port().into()).await;
                    if socks && socks5_reply(&mut tcp, ch.is_ok()).await.is_err() {
                        return;
                    }
                    match ch {
                        Ok(ch) => {
                            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut ch.into_stream()).await;
                        }
                        Err(e) => tracing::info!("Weiterleitung nach {host}:{port} abgelehnt: {e}"),
                    }
                });
            }
        }));
    }
    out
}

/// Verbindung, die der Server für eine R-Weiterleitung öffnet, ans lokale Ziel durchreichen.
pub fn serve_remote(channel: russh::Channel<russh::client::Msg>, host: String, port: u16) {
    tokio::spawn(async move {
        match tokio::net::TcpStream::connect((host.as_str(), port)).await {
            Ok(mut tcp) => {
                let _ = tokio::io::copy_bidirectional(&mut tcp, &mut channel.into_stream()).await;
            }
            Err(e) => tracing::info!("Fernweiterleitung nach {host}:{port}: {e}"),
        }
    });
}

/// SOCKS5 (RFC 1928) ohne Anmeldung, nur CONNECT. Gibt das Ziel zurück; die Antwort folgt mit `socks5_reply`.
pub async fn socks5_request<S: AsyncRead + AsyncWrite + Unpin>(s: &mut S) -> Result<(String, u16), String> {
    let e = |e: std::io::Error| e.to_string();
    if s.read_u8().await.map_err(e)? != 5 {
        return Err("Kein SOCKS5".into());
    }
    let mut methods = vec![0; s.read_u8().await.map_err(e)?.into()];
    s.read_exact(&mut methods).await.map_err(e)?;
    if !methods.contains(&0) {
        let _ = s.write_all(&[5, 0xFF]).await;
        return Err("SOCKS5: nur ohne Anmeldung".into());
    }
    s.write_all(&[5, 0]).await.map_err(e)?;
    let mut head = [0; 4];
    s.read_exact(&mut head).await.map_err(e)?;
    let fail = |code: u8| [5, code, 0, 1, 0, 0, 0, 0, 0, 0];
    if head[0] != 5 || head[1] != 1 {
        let _ = s.write_all(&fail(7)).await; // Befehl nicht unterstützt
        return Err("SOCKS5: nur CONNECT".into());
    }
    let host = match head[3] {
        1 => {
            let mut a = [0; 4];
            s.read_exact(&mut a).await.map_err(e)?;
            std::net::Ipv4Addr::from(a).to_string()
        }
        3 => {
            let mut name = vec![0; s.read_u8().await.map_err(e)?.into()];
            s.read_exact(&mut name).await.map_err(e)?;
            String::from_utf8(name).map_err(|_| "SOCKS5: ungültiger Name".to_string())?
        }
        4 => {
            let mut a = [0; 16];
            s.read_exact(&mut a).await.map_err(e)?;
            std::net::Ipv6Addr::from(a).to_string()
        }
        _ => {
            let _ = s.write_all(&fail(8)).await;
            return Err("SOCKS5: unbekannter Adresstyp".into());
        }
    };
    Ok((host, s.read_u16().await.map_err(e)?))
}

pub async fn socks5_reply<S: AsyncWrite + Unpin>(s: &mut S, ok: bool) -> std::io::Result<()> {
    // Gebundene Adresse ist für CONNECT bedeutungslos: 0.0.0.0:0.
    s.write_all(&[5, if ok { 0 } else { 5 }, 0, 1, 0, 0, 0, 0, 0, 0]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert_eq!(parse("L8080 localhost:80").unwrap(), Rule::Local { listen: 8080, host: "localhost".into(), port: 80 });
        assert_eq!(parse(" l 5433  db.intern:5432 ").unwrap(), Rule::Local { listen: 5433, host: "db.intern".into(), port: 5432 });
        assert_eq!(parse("R9000 127.0.0.1:3000").unwrap(), Rule::Remote { listen: 9000, host: "127.0.0.1".into(), port: 3000 });
        assert_eq!(parse("D1080").unwrap(), Rule::Dynamic { listen: 1080 });
        assert_eq!(parse("L8080 [::1]:80").unwrap(), Rule::Local { listen: 8080, host: "::1".into(), port: 80 });
        for bad in ["", "X1 a:1", "L0 a:1", "L8080", "L8080 host", "L8080 host:0", "L70000 a:1", "D1080 a:1", "Lx a:1", "L1 :80", "L1 a b:80"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn socks5_handshake() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let client = tokio::spawn(async move {
            b.write_all(&[5, 1, 0]).await.unwrap();
            let mut r = [0; 2];
            b.read_exact(&mut r).await.unwrap();
            assert_eq!(r, [5, 0]);
            b.write_all(&[5, 1, 0, 3, 7]).await.unwrap();
            b.write_all(b"example").await.unwrap();
            b.write_all(&443u16.to_be_bytes()).await.unwrap();
            let mut r = [0; 10];
            b.read_exact(&mut r).await.unwrap();
            assert_eq!(r[..2], [5, 0]);
        });
        assert_eq!(socks5_request(&mut a).await.unwrap(), ("example".to_string(), 443));
        socks5_reply(&mut a, true).await.unwrap();
        client.await.unwrap();

        // IPv4 und Ablehnung von Anmeldung/BIND.
        let (mut a, mut b) = tokio::io::duplex(1024);
        b.write_all(&[5, 1, 0, 5, 1, 0, 1, 10, 0, 0, 1, 0, 22]).await.unwrap();
        assert_eq!(socks5_request(&mut a).await.unwrap(), ("10.0.0.1".to_string(), 22));
        let (mut a, mut b) = tokio::io::duplex(1024);
        b.write_all(&[5, 1, 2]).await.unwrap();
        assert!(socks5_request(&mut a).await.is_err());
        let (mut a, mut b) = tokio::io::duplex(1024);
        b.write_all(&[5, 1, 0, 5, 2, 0, 1, 10, 0, 0, 1, 0, 22]).await.unwrap();
        assert!(socks5_request(&mut a).await.unwrap_err().contains("CONNECT"));
        let (mut a, mut b) = tokio::io::duplex(1024);
        b.write_all(&[4, 1]).await.unwrap();
        assert!(socks5_request(&mut a).await.is_err());
    }
}

/// Tunnel für einen Desktop-Dienst: lokaler Port auf 127.0.0.1 (vom System gewählt) → `port` auf dem Server selbst.
/// Der Tunnel lebt, bis der zurückgegebene Task abgebrochen wird.
pub async fn tunnel(h: Arc<Handle<Client>>, port: u16) -> Result<(u16, tokio::task::JoinHandle<()>), String> {
    let l = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| format!("SSH-Tunnel: {e}"))?;
    let local = l.local_addr().map_err(|e| e.to_string())?.port();
    let task = tokio::spawn(async move {
        while let Ok((mut tcp, peer)) = l.accept().await {
            let h = h.clone();
            tokio::spawn(async move {
                let _ = tcp.set_nodelay(true);
                match h.channel_open_direct_tcpip("127.0.0.1", port.into(), peer.ip().to_string(), peer.port().into()).await {
                    Ok(ch) => {
                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut ch.into_stream()).await;
                    }
                    Err(e) => tracing::info!("SSH-Tunnel zu Port {port} abgelehnt: {e}"),
                }
            });
        }
    });
    Ok((local, task))
}
