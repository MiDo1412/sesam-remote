//! Erste Hilfe: eine Verbindung Schritt für Schritt prüfen (Name, Erreichbarkeit, Port, Begrüssung des Dienstes).
//! Anmeldung und Zertifikat prüft lib.rs mit den Zugangsdaten der Verbindung; hier nur, was ohne sie geht.

use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Ein Schritt der Prüfung. `ok`: Some(true) bestanden, Some(false) Fehler, None übersprungen/Hinweis.
/// `hint` sagt dem Frontend, welchen Knopf es anbieten soll (wake, localnet, tunnel, creds, trust).
#[derive(Serialize, Clone, Debug)]
pub struct Step {
    pub key: &'static str,
    pub title: String,
    pub ok: Option<bool>,
    pub detail: String,
    pub hint: &'static str,
}

pub fn step(key: &'static str, title: &str, ok: Option<bool>, detail: impl Into<String>, hint: &'static str) -> Step {
    Step { key, title: title.into(), ok, detail: detail.into(), hint }
}

/// Wie der Dienst grüsst: SSH und FTP und VNC melden sich selbst, RDP antwortet auf eine X.224-Anfrage.
pub fn greeting_ok(service: &str, data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    let first = text.lines().next().unwrap_or("").trim().to_string();
    match service {
        "ssh" if first.starts_with("SSH-") => Some(first),
        "vnc" if first.starts_with("RFB ") => Some(first),
        "ftp" if first.starts_with("220") => Some(first),
        "rdp" if data.len() >= 4 && data[0] == 3 && data[1] == 0 => Some("RDP (X.224) antwortet".into()),
        _ => None,
    }
}

/// X.224 Connection Request mit RDP-Aushandlung (TLS und CredSSP angeboten), 19 Bytes.
const RDP_CR: [u8; 19] = [3, 0, 0, 19, 14, 0xe0, 0, 0, 0, 0, 0, 1, 0, 8, 0, 3, 0, 0, 0];

fn private_address(host: &str) -> bool {
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(a)) => a.is_private() || a.is_link_local(),
        Ok(std::net::IpAddr::V6(a)) => (a.segments()[0] & 0xfe00) == 0xfc00 || (a.segments()[0] & 0xffc0) == 0xfe80,
        Err(_) => host.ends_with(".local") || host.ends_with(".fritz.box") || host.ends_with(".lan") || host.ends_with(".home") || !host.contains('.'),
    }
}

/// Name, Erreichbarkeit, Port und Begrüssung. Gibt die Schritte zurück und ob es danach weitergehen kann.
pub async fn network(host: &str, port: u16, service: &str, send: &impl Fn(Step)) -> bool {
    let name = "Namensauflösung";
    if host.parse::<std::net::IpAddr>().is_ok() {
        send(step("name", name, Some(true), format!("{host} ist eine IP-Adresse, kein Name nötig."), ""));
    } else {
        match tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host((host, port))).await {
            Ok(Ok(a)) => {
                let ips: Vec<String> = a.map(|a| a.ip().to_string()).collect::<std::collections::BTreeSet<_>>().into_iter().take(3).collect();
                send(step("name", name, Some(true), format!("{host} → {}", ips.join(", ")), ""));
            }
            _ => {
                send(step("name", name, Some(false), format!("Der Name «{host}» ist unbekannt. Tippfehler? Im Heimnetz hilft oft die IP-Adresse statt des Namens."), ""));
                return false;
            }
        }
    }
    let t = Instant::now();
    let conn = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect((host, port))).await;
    let reach = "Erreichbarkeit";
    let port_title = format!("Port {port}");
    let mut tcp = match conn {
        Ok(Ok(s)) => {
            send(step("reach", reach, Some(true), format!("Antwortet in {} ms.", t.elapsed().as_millis()), ""));
            send(step("port", &port_title, Some(true), "Offen.", ""));
            s
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            send(step("reach", reach, Some(true), "Das Gerät antwortet.", ""));
            send(step("port", &port_title, Some(false), format!("Geschlossen: auf Port {port} läuft kein Dienst, oder eine Firewall weist ab."), "tunnel"));
            return false;
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            let local = cfg!(target_os = "macos") && private_address(host) && (msg.contains("No route to host") || msg.contains("os error 65"));
            if local {
                send(step("reach", reach, Some(false), "macOS blockiert den Zugriff ins lokale Netz. Sesam braucht dafür die Freigabe «Lokales Netzwerk».", "localnet"));
            } else {
                send(step("reach", reach, Some(false), format!("Nicht erreichbar: {msg}"), "wake"));
            }
            return false;
        }
        Err(_) => {
            // Keine Antwort auf diesem Port: antwortet das Gerät auf einem anderen?
            let other = crate::wake::reach(host, &[22, 3389, 5900, 80, 443, 445].into_iter().filter(|p| *p != port).collect::<Vec<_>>()).await;
            if let Some(ms) = other {
                send(step("reach", reach, Some(true), format!("Das Gerät antwortet ({ms} ms)."), ""));
                send(step("port", &port_title, Some(false), format!("Keine Antwort auf Port {port}: vermutlich filtert eine Firewall."), "tunnel"));
            } else {
                send(step("reach", reach, Some(false), "Keine Antwort. Ausgeschaltet, im Ruhezustand oder in einem anderen Netz?", "wake"));
            }
            return false;
        }
    };
    let greet = "Begrüssung des Dienstes";
    let mut buf = vec![0u8; 256];
    if service == "rdp" {
        let _ = tcp.write_all(&RDP_CR).await;
    }
    let n = match tokio::time::timeout(Duration::from_secs(4), tcp.read(&mut buf)).await {
        Ok(Ok(n)) => n,
        _ => 0,
    };
    match greeting_ok(service, &buf[..n]) {
        Some(g) => {
            send(step("greet", greet, Some(true), g, ""));
            true
        }
        None => {
            let seen = if n == 0 { "keine Antwort".to_string() } else { format!("«{}»", String::from_utf8_lossy(&buf[..n.min(40)]).trim()) };
            send(step("greet", greet, Some(false), format!("Port {port} antwortet nicht wie {}: {seen}. Läuft dort ein anderer Dienst?", service.to_uppercase()), ""));
            false
        }
    }
}

/// Netzwerkfehler beim Verbinden als kurzer deutscher Satz; der Originaltext des Systems (oft englisch) nur ins Protokoll.
pub fn net_error(host: &str, port: u16, e: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    tracing::warn!(%host, port, error = %e, "Verbinden fehlgeschlagen");
    let msg = e.to_string();
    // getaddrinfo: Unix «failed to lookup address information», Windows WSAHOST_NOT_FOUND/WSANO_DATA
    if msg.contains("failed to lookup address") || matches!(e.raw_os_error(), Some(11001 | 11004)) {
        return format!("Der Name «{host}» ist unbekannt. Tippfehler? Im Heimnetz hilft oft die IP-Adresse statt des Namens.");
    }
    match e.kind() {
        K::ConnectionRefused => format!("Port {port} auf {host} ist geschlossen: dort läuft kein Dienst, oder eine Firewall weist ab."),
        K::TimedOut => format!("{host} antwortet nicht (Zeitüberschreitung)."),
        K::HostUnreachable | K::NetworkUnreachable | K::NetworkDown => {
            format!("{host} ist nicht erreichbar: ausgeschaltet, in einem anderen Netz oder ohne Freigabe «Lokales Netzwerk».")
        }
        K::ConnectionReset | K::ConnectionAborted => format!("{host} hat die Verbindung abgebrochen."),
        _ => format!("Verbindung zu {host} (Port {port}) fehlgeschlagen. Einzelheiten stehen im Protokoll."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erkennt_begruessungen() {
        assert_eq!(greeting_ok("ssh", b"SSH-2.0-OpenSSH_9.6\r\n").as_deref(), Some("SSH-2.0-OpenSSH_9.6"));
        assert!(greeting_ok("vnc", b"RFB 003.008\n").is_some() && greeting_ok("ftp", b"220 Willkommen\r\n").is_some());
        assert!(greeting_ok("rdp", &[3, 0, 0, 19, 14, 0xd0]).is_some());
        assert!(greeting_ok("ssh", b"HTTP/1.1 400").is_none() && greeting_ok("rdp", b"").is_none());
        assert!(private_address("192.168.1.5") && private_address("pi.local") && !private_address("8.8.8.8") && !private_address("example.com"));
    }

    #[tokio::test]
    async fn geschlossener_port_und_unbekannter_name() {
        let steps = std::sync::Mutex::new(Vec::new());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        assert!(!network("127.0.0.1", port, "ssh", &|s| steps.lock().unwrap().push(s)).await);
        let s = steps.lock().unwrap().clone();
        assert_eq!(s.iter().map(|s| (s.key, s.ok)).collect::<Vec<_>>(), [("name", Some(true)), ("reach", Some(true)), ("port", Some(false))]);
        steps.lock().unwrap().clear();
        assert!(!network("gibt-es-nicht.invalid", 22, "ssh", &|s| steps.lock().unwrap().push(s)).await);
        assert_eq!(steps.lock().unwrap()[0].ok, Some(false));
    }

    #[tokio::test]
    async fn netzfehler_auf_deutsch() {
        let e = tokio::net::TcpStream::connect(("gibt-es-nicht.invalid", 22)).await.unwrap_err();
        assert!(net_error("gibt-es-nicht.invalid", 22, &e).starts_with("Der Name «gibt-es-nicht.invalid» ist unbekannt"), "{e}");
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let e = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap_err();
        assert!(net_error("127.0.0.1", port, &e).starts_with(&format!("Port {port} auf 127.0.0.1 ist geschlossen")));
        let e = std::io::Error::other("irgendwas");
        assert!(!net_error("h", 1, &e).contains("irgendwas"));
    }
}
