//! Kerberos für NLA: Transport der KDC-Anfragen, die sspi während CredSSP stellt.
//! Den KDC sucht sspi selbst (Registry, DNS-SRV `_kerberos._tcp.<realm>` über das Betriebssystem, krb5.conf),
//! ausser er ist pro Verbindung eingetragen: `host[:port]`, `tcp://…`, `udp://…` oder ein KDC-Proxy `https://…/KdcProxy`.
//! Die KKDCP-Hülle (KDC-PROXY-MESSAGE, MS-KKDCP) baut sspi; hier gehen nur Bytes über TCP, UDP oder HTTP(S).

use ironrdp_connector::sspi::generator::NetworkRequest;
use ironrdp_connector::sspi::network_client::NetworkProtocol;
use ironrdp_connector::{ConnectorResult, custom_err};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

const TIMEOUT: Duration = Duration::from_secs(10);
/// Obergrenze für eine KDC-Antwort (Tickets mit grosser PAC bleiben weit darunter).
const MAX_REPLY: usize = 1 << 20;

/// Pro Verbindung eingetragener KDC als URL für sspi. Leer = automatisch.
pub fn kdc_url(input: &str) -> Result<Option<String>, String> {
    let s = input.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let url = if s.contains("://") { s.to_string() } else { format!("tcp://{s}") };
    let scheme = url.split("://").next().unwrap_or_default().to_ascii_lowercase();
    if !["tcp", "udp", "http", "https"].contains(&scheme.as_str()) {
        return Err("KDC: nur host[:port], tcp://, udp:// oder https:// (KDC-Proxy)".into());
    }
    match url::Url::parse(&url) {
        Ok(u) if u.host_str().is_some_and(|h| !h.is_empty()) => Ok(Some(url)),
        _ => Err("KDC: Adresse ist ungültig".into()),
    }
}

/// Kerberos versuchen? Nur mit Realm (UPN `benutzer@domain.tld` oder DNS-Domäne), eingetragenem KDC
/// oder wenn NTLM verboten ist. Sonst bleibt es bei NTLM wie bisher (lokale Konten, GNOME, xrdp).
pub fn wants_kerberos(user: &str, domain: Option<&str>, kdc: bool, no_ntlm: bool) -> bool {
    let realm = user.rsplit_once('@').map(|(_, r)| r).or(domain).unwrap_or_default();
    kdc || no_ntlm || realm.contains('.')
}

/// Name dieses Rechners für Kerberos (Adressen im AS-REQ).
pub fn computer_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "sesam".into())
}

/// Sendet KDC-Anfragen von sspi. Antworten für TCP/UDP mit 4-Byte-Längenpräfix, wie sspi sie erwartet.
pub struct Kdc;

impl ironrdp_tokio::NetworkClient for Kdc {
    async fn send(&mut self, req: &NetworkRequest) -> ConnectorResult<Vec<u8>> {
        send(req).await.map_err(|e| custom_err!("KDC", std::io::Error::other(e)))
    }
}

pub async fn send(req: &NetworkRequest) -> Result<Vec<u8>, String> {
    let host = req.url.host_str().unwrap_or_default().trim_matches(['[', ']']).to_string();
    tracing::info!(protocol = ?req.protocol, %host, "Kerberos: Anfrage an KDC");
    let work = async {
        match req.protocol {
            NetworkProtocol::Tcp => tcp(&host, req.url.port().unwrap_or(88), &req.data).await,
            NetworkProtocol::Udp => udp(&host, req.url.port().unwrap_or(88), &req.data).await,
            NetworkProtocol::Http | NetworkProtocol::Https => http(&req.url, &host, &req.data).await,
        }
    };
    let r = tokio::time::timeout(TIMEOUT, work).await.unwrap_or_else(|_| Err("Zeitüberschreitung".into()));
    if let Err(e) = &r {
        tracing::warn!(%host, error = %e, "Kerberos: KDC nicht erreichbar");
    }
    r
}

async fn tcp(host: &str, port: u16, data: &[u8]) -> Result<Vec<u8>, String> {
    let mut s = TcpStream::connect((host, port)).await.map_err(|e| e.to_string())?;
    s.write_all(data).await.map_err(|e| e.to_string())?;
    let len = s.read_u32().await.map_err(|e| e.to_string())? as usize;
    if len > MAX_REPLY {
        return Err("Antwort zu gross".into());
    }
    let mut buf = vec![0; 4 + len];
    buf[..4].copy_from_slice(&(len as u32).to_be_bytes());
    s.read_exact(&mut buf[4..]).await.map_err(|e| e.to_string())?;
    Ok(buf)
}

async fn udp(host: &str, port: u16, data: &[u8]) -> Result<Vec<u8>, String> {
    let addr = tokio::net::lookup_host((host, port)).await.map_err(|e| e.to_string())?.next().ok_or("KDC-Adresse nicht gefunden")?;
    let local: std::net::SocketAddr = if addr.is_ipv4() { ([0, 0, 0, 0], 0).into() } else { (std::net::Ipv6Addr::UNSPECIFIED, 0).into() };
    let sock = UdpSocket::bind(local).await.map_err(|e| e.to_string())?;
    sock.connect(addr).await.map_err(|e| e.to_string())?;
    sock.send(data).await.map_err(|e| e.to_string())?;
    let mut buf = vec![0; 0xFFFF];
    let n = sock.recv(&mut buf).await.map_err(|e| e.to_string())?;
    let mut out = (n as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&buf[..n]);
    Ok(out)
}

/// KDC-Proxy (MS-KKDCP): POST mit der KDC-PROXY-MESSAGE, Antwort ist wieder eine.
async fn http(url: &url::Url, host: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let port = url.port_or_known_default().unwrap_or(443);
    let tcp = TcpStream::connect((host, port)).await.map_err(|e| e.to_string())?;
    let req = http_request(url, data);
    let raw = if url.scheme() == "https" {
        // Strikte Prüfung gegen die Stammzertifikate des Systems, kein Pinning-Dialog. Ausdrücklich `Strict`: die
        // Voreinstellung von ironrdp nimmt jedes Zertifikat (security-review-2026-11-11, S-02).
        let (tls, _) = ironrdp_tls::upgrade_with_certificate_validation(tcp, host, ironrdp_tls::CertificateValidation::Strict).await.map_err(|e| format!("TLS zum KDC-Proxy: {e}"))?;
        exchange(tls, &req).await?
    } else {
        exchange(tcp, &req).await?
    };
    parse_http_response(&raw)
}

fn http_request(url: &url::Url, body: &[u8]) -> Vec<u8> {
    let path = &url[url::Position::BeforePath..url::Position::AfterQuery];
    let host = &url[url::Position::BeforeHost..url::Position::AfterPort];
    let mut r = format!(
        "POST {} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Sesam Remote\r\nContent-Type: application/kerberos\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        if path.is_empty() { "/" } else { path },
        body.len()
    )
    .into_bytes();
    r.extend_from_slice(body);
    r
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(mut s: S, req: &[u8]) -> Result<Vec<u8>, String> {
    s.write_all(req).await.map_err(|e| e.to_string())?;
    s.flush().await.map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    // Server schliessen nach «Connection: close»; manche TLS-Gegenstellen ohne close_notify, das ist hier unkritisch.
    let r = (&mut s).take(MAX_REPLY as u64 + 1).read_to_end(&mut raw).await;
    if let Err(e) = r
        && raw.is_empty() {
            return Err(e.to_string());
        }
    if raw.len() > MAX_REPLY {
        return Err("Antwort zu gross".into());
    }
    Ok(raw)
}

/// HTTP/1.1-Antwort: Status 200, Körper per Content-Length, chunked oder bis Verbindungsende.
fn parse_http_response(raw: &[u8]) -> Result<Vec<u8>, String> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("KDC-Proxy: unvollständige Antwort")?;
    let head = std::str::from_utf8(&raw[..end]).map_err(|_| "KDC-Proxy: ungültige Antwort")?;
    let body = &raw[end + 4..];
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or_default();
    let code = status.split_whitespace().nth(1).unwrap_or_default();
    if !status.starts_with("HTTP/1.") || code != "200" {
        return Err(format!("KDC-Proxy antwortet mit «{}»", status.chars().take(80).collect::<String>()));
    }
    let header = |name: &str| {
        head.split("\r\n").skip(1).find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
    };
    if header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
        return dechunk(body);
    }
    match header("content-length") {
        Some(n) => {
            let n: usize = n.parse().map_err(|_| "KDC-Proxy: ungültige Content-Length")?;
            body.get(..n).map(<[u8]>::to_vec).ok_or_else(|| "KDC-Proxy: Antwort abgeschnitten".into())
        }
        None => Ok(body.to_vec()),
    }
}

fn dechunk(mut b: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = b.windows(2).position(|w| w == b"\r\n").ok_or("KDC-Proxy: ungültiger Chunk")?;
        let size = std::str::from_utf8(&b[..eol]).ok().and_then(|s| usize::from_str_radix(s.split(';').next()?.trim(), 16).ok()).ok_or("KDC-Proxy: ungültiger Chunk")?;
        b = &b[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        out.extend_from_slice(b.get(..size).ok_or("KDC-Proxy: Antwort abgeschnitten")?);
        b = b.get(size + 2..).ok_or("KDC-Proxy: Antwort abgeschnitten")?;
    }
}

/// Enthält ein SPNEGO-Token eine NTLM-Nachricht? (Signatur «NTLMSSP\0», MS-NLMP 2.2.1)
pub fn carries_ntlm(token: &[u8]) -> bool {
    token.windows(8).any(|w| w == b"NTLMSSP\0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kdc_input_forms() {
        assert_eq!(kdc_url("").unwrap(), None);
        assert_eq!(kdc_url(" dc1.contoso.ch ").unwrap().as_deref(), Some("tcp://dc1.contoso.ch"));
        assert_eq!(kdc_url("dc1.contoso.ch:88").unwrap().as_deref(), Some("tcp://dc1.contoso.ch:88"));
        assert_eq!(kdc_url("https://gw.contoso.ch/KdcProxy").unwrap().as_deref(), Some("https://gw.contoso.ch/KdcProxy"));
        assert!(kdc_url("udp://10.0.0.1:88").unwrap().is_some());
        assert!(kdc_url("ftp://x").is_err());
        assert!(kdc_url("tcp://").is_err());
    }

    #[test]
    fn kerberos_only_with_realm() {
        assert!(wants_kerberos("anna@contoso.ch", None, false, false));
        assert!(wants_kerberos("anna", Some("contoso.local"), false, false));
        assert!(!wants_kerberos("anna", Some("CONTOSO"), false, false));
        assert!(!wants_kerberos("admin", None, false, false));
        assert!(wants_kerberos("admin", None, true, false));
        assert!(wants_kerberos("admin", None, false, true));
    }

    #[test]
    fn kkdcp_request_and_responses() {
        let url = url::Url::parse("https://gw.contoso.ch/KdcProxy").unwrap();
        let req = String::from_utf8(http_request(&url, b"\x30\x03abc")).unwrap();
        assert!(req.starts_with("POST /KdcProxy HTTP/1.1\r\nHost: gw.contoso.ch\r\n"));
        assert!(req.contains("Content-Type: application/kerberos\r\n") && req.contains("Content-Length: 5\r\n"));
        assert!(req.ends_with("\r\n\r\n\x30\x03abc"));

        let ok = b"HTTP/1.1 200 OK\r\nContent-Type: application/kerberos\r\nContent-Length: 3\r\n\r\nxyzJUNK";
        assert_eq!(parse_http_response(ok).unwrap(), b"xyz");
        let chunked = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n2\r\nab\r\n3;x=y\r\ncde\r\n0\r\n\r\n";
        assert_eq!(parse_http_response(chunked).unwrap(), b"abcde");
        let close = b"HTTP/1.0 200 OK\r\n\r\nall";
        assert_eq!(parse_http_response(close).unwrap(), b"all");
        assert!(parse_http_response(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap_err().contains("404"));
        assert!(parse_http_response(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nab").is_err());
        assert!(parse_http_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nff\r\nab").is_err());
    }

    #[test]
    fn ntlm_signature() {
        assert!(carries_ntlm(b"\xa0\x0e\x30\x0cNTLMSSP\0\x01\x00\x00\x00"));
        assert!(!carries_ntlm(b"\x60\x82\x01\x00\x06\x09\x2a\x86\x48\x86\xf7\x12\x01\x02\x02"));
    }

    /// KDC über TCP: Anfrage mit Längenpräfix hin, Antwort mit Längenpräfix zurück.
    #[tokio::test]
    async fn tcp_roundtrip() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let n = s.read_u32().await.unwrap() as usize;
            let mut b = vec![0; n];
            s.read_exact(&mut b).await.unwrap();
            assert_eq!(b, b"req");
            s.write_all(b"\0\0\0\x02ok").await.unwrap();
        });
        let req = NetworkRequest { protocol: NetworkProtocol::Tcp, url: url::Url::parse(&format!("tcp://127.0.0.1:{port}")).unwrap(), data: b"\0\0\0\x03req".to_vec() };
        assert_eq!(send(&req).await.unwrap(), b"\0\0\0\x02ok");
    }
}
