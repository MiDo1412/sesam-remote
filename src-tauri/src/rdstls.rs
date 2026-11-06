//! RDSTLS ([MS-RDPBCGR] 2.2.17, 5.4.5.4): Anmeldung nach einer Umleitung mit verschlüsseltem Passwort.
//! gnome-remote-desktop nutzt das für die Übergabe vom Anmeldebildschirm an die Benutzersitzung.
//! Der Client entschlüsselt nichts: Redirection-GUID und Passwort-Blob gehen unverändert zurück.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const VERSION_1: u16 = 0x0001;
const TYPE_CAPABILITIES: u16 = 0x0001;
const TYPE_AUTH_REQUEST: u16 = 0x0002;
const TYPE_AUTH_RESPONSE: u16 = 0x0004;
const DATA_CAPABILITIES: u16 = 0x0001;
const DATA_PASSWORD_CREDS: u16 = 0x0001;
const DATA_RESULT_CODE: u16 = 0x0001;

#[derive(Clone, Debug, PartialEq)]
pub struct Creds {
    pub redirection_guid: Vec<u8>,
    pub user: String,
    pub domain: String,
    /// Verschlüsseltes Passwort aus der Umleitung, unverändert.
    pub password_blob: Vec<u8>,
}

/// UTF-16LE mit Nullterminator, Länge in Byte davor (wie FreeRDP).
fn put_string(out: &mut Vec<u8>, s: &str) -> Result<(), String> {
    let b: Vec<u8> = s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    put_data(out, &b)
}

fn put_data(out: &mut Vec<u8>, d: &[u8]) -> Result<(), String> {
    let n = u16::try_from(d.len()).map_err(|_| "RDSTLS: Feld zu lang".to_string())?;
    out.extend(n.to_le_bytes());
    out.extend_from_slice(d);
    Ok(())
}

pub fn auth_request(c: &Creds) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for v in [VERSION_1, TYPE_AUTH_REQUEST, DATA_PASSWORD_CREDS] {
        out.extend(v.to_le_bytes());
    }
    put_data(&mut out, &c.redirection_guid)?;
    put_string(&mut out, &c.user)?;
    put_string(&mut out, &c.domain)?;
    put_data(&mut out, &c.password_blob)?;
    Ok(out)
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

/// Server-Fähigkeiten prüfen (8 Byte): Version 1 muss dabei sein.
pub fn check_capabilities(b: &[u8; 8]) -> Result<(), String> {
    if u16_at(b, 2) != TYPE_CAPABILITIES || u16_at(b, 4) != DATA_CAPABILITIES {
        return Err(format!("RDSTLS: unerwartete Nachricht statt Fähigkeiten ({:02x?})", &b[..6]));
    }
    if u16_at(b, 6) & VERSION_1 == 0 {
        return Err("RDSTLS: Server unterstützt Version 1 nicht".into());
    }
    Ok(())
}

/// Antwort auf die Anmeldung (10 Byte): Ergebniscode 0 = angenommen.
pub fn check_response(b: &[u8; 10]) -> Result<(), String> {
    if u16_at(b, 2) != TYPE_AUTH_RESPONSE || u16_at(b, 4) != DATA_RESULT_CODE {
        return Err(format!("RDSTLS: unerwartete Antwort ({:02x?})", &b[..6]));
    }
    match u32::from_le_bytes([b[6], b[7], b[8], b[9]]) {
        0 => Ok(()),
        code => Err(format!("RDSTLS: Übergabe vom Server abgelehnt (Code 0x{code:08x})")),
    }
}

/// Austausch auf der bereits aufgebauten TLS-Verbindung.
pub async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(s: &mut S, c: &Creds) -> Result<(), String> {
    let io = |e: std::io::Error| format!("RDSTLS: {e}");
    let mut caps = [0u8; 8];
    s.read_exact(&mut caps).await.map_err(io)?;
    check_capabilities(&caps)?;
    s.write_all(&auth_request(c)?).await.map_err(io)?;
    s.flush().await.map_err(io)?;
    let mut resp = [0u8; 10];
    s.read_exact(&mut resp).await.map_err(io)?;
    check_response(&resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> Creds {
        Creds { redirection_guid: vec![0xAA, 0xBB], user: "mike".into(), domain: String::new(), password_blob: vec![1, 2, 3] }
    }

    #[test]
    fn encodes_auth_request_like_freerdp() {
        let b = auth_request(&creds()).unwrap();
        let mut want = vec![1, 0, 2, 0, 1, 0];
        want.extend([2, 0, 0xAA, 0xBB]);
        want.extend([10, 0, b'm', 0, b'i', 0, b'k', 0, b'e', 0, 0, 0]);
        want.extend([2, 0, 0, 0]); // leere Domäne: nur Nullterminator
        want.extend([3, 0, 1, 2, 3]);
        assert_eq!(b, want);
    }

    #[test]
    fn checks_server_messages() {
        assert!(check_capabilities(&[1, 0, 1, 0, 1, 0, 1, 0]).is_ok());
        assert!(check_capabilities(&[1, 0, 4, 0, 1, 0, 1, 0]).is_err());
        assert!(check_response(&[1, 0, 4, 0, 1, 0, 0, 0, 0, 0]).is_ok());
        assert!(check_response(&[1, 0, 4, 0, 1, 0, 0x6d, 0, 0, 0xc0]).unwrap_err().contains("c000006d"));
    }

    #[tokio::test]
    async fn handshake_against_fake_server() {
        let (mut c, mut s) = tokio::io::duplex(1024);
        let server = tokio::spawn(async move {
            s.write_all(&[1, 0, 1, 0, 1, 0, 1, 0]).await.unwrap();
            let want = auth_request(&creds()).unwrap();
            let mut got = vec![0u8; want.len()];
            s.read_exact(&mut got).await.unwrap();
            assert_eq!(got, want);
            s.write_all(&[1, 0, 4, 0, 1, 0, 0, 0, 0, 0]).await.unwrap();
        });
        handshake(&mut c, &creds()).await.unwrap();
        server.await.unwrap();
    }
}
