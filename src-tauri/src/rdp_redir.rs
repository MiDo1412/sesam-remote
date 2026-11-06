//! Server-Redirection ([MS-RDPBCGR] 2.2.13.1), nachsichtig dekodiert.
//! IronRDP prüft das Paket streng; gnome-remote-desktop (FreeRDP) schickt es bei der Übergabe nach dem
//! Login in einer Form, an der dieser Decoder scheitert. Dieser hier greift als Rückfallebene.

use base64::Engine;

/// Was Sesam aus einer Umleitung braucht.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Redir {
    pub session_id: u32,
    pub target_net_address: Option<String>,
    pub load_balance_info: Option<Vec<u8>>,
    pub username: Option<String>,
    pub domain: Option<String>,
    pub password: Option<Vec<u8>>,
    pub target_fqdn: Option<String>,
    pub target_netbios_name: Option<String>,
    pub target_net_addresses: Vec<String>,
    /// DER des Zertifikats, das das Ziel zeigen wird (ohne erneute Rückfrage annehmen).
    pub target_cert: Option<Vec<u8>>,
    pub password_pk_encrypted: bool,
}

const SEC_REDIRECTION_PKT: u16 = 0x0400;
const F_ADDR: u32 = 0x1;
const F_LB: u32 = 0x2;
const F_USER: u32 = 0x4;
const F_DOMAIN: u32 = 0x8;
const F_PASSWORD: u32 = 0x10;
const F_FQDN: u32 = 0x100;
const F_NETBIOS: u32 = 0x200;
const F_ADDRS: u32 = 0x800;
const F_TSV: u32 = 0x1000;
const F_PK: u32 = 0x4000;
const F_GUID: u32 = 0x8000;
const F_CERT: u32 = 0x1_0000;

impl From<&ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu> for Redir {
    fn from(p: &ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu) -> Self {
        use ironrdp_pdu::rdp::server_redirection::ServerRedirectionFlags as F;
        Redir {
            session_id: p.session_id,
            target_net_address: p.target_net_address.clone(),
            load_balance_info: p.load_balance_info.clone(),
            username: p.username.clone(),
            domain: p.domain.clone(),
            password: p.password.clone(),
            target_fqdn: p.target_fqdn.clone(),
            target_netbios_name: p.target_netbios_name.clone(),
            target_net_addresses: p.target_net_addresses.clone().unwrap_or_default(),
            target_cert: p.target_certificate.as_ref().and_then(|c| c.certificate_der()).map(<[u8]>::to_vec),
            password_pk_encrypted: p.flags.contains(F::PASSWORD_IS_PK_ENCRYPTED),
        }
    }
}

struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn u16(&mut self) -> Option<u16> {
        let v = self.b.get(self.i..self.i + 2)?;
        self.i += 2;
        Some(u16::from_le_bytes([v[0], v[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        let v = self.b.get(self.i..self.i + 4)?;
        self.i += 4;
        Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }
    fn field(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        let v = self.b.get(self.i..self.i + n)?;
        self.i += n;
        Some(v)
    }
}

/// UTF-16LE ohne abschliessende Nullen.
fn utf16(b: &[u8]) -> Option<String> {
    let units: Vec<u16> = b.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
    let end = units.iter().rposition(|u| *u != 0).map_or(0, |i| i + 1);
    String::from_utf16(&units[..end]).ok()
}

/// TargetCertificate: Base64 in UTF-16 (laut Spezifikation), Container aus Elementen; Typ 32 + DER = Zertifikat.
/// Nimmt auch Base64 mit Nullterminator, ASCII-Base64 oder den Container roh.
fn target_cert(b: &[u8]) -> Option<Vec<u8>> {
    let text = utf16(b).filter(|t| !t.is_empty() && t.is_ascii()).or_else(|| std::str::from_utf8(b).ok().map(|s| s.trim_end_matches('\0').to_string()));
    let container = text
        .and_then(|t| base64::engine::general_purpose::STANDARD.decode(t.trim()).ok())
        .unwrap_or_else(|| b.to_vec());
    let mut c = Cur { b: &container, i: 0 };
    while c.i + 12 <= container.len() {
        let (kind, enc, size) = (c.u32()?, c.u32()?, c.u32()? as usize);
        let data = container.get(c.i..c.i + size)?;
        c.i += size;
        if kind == 32 && enc == 1 {
            return Some(data.to_vec());
        }
    }
    // Zur Not: direkt ein DER-Zertifikat (SEQUENCE).
    (container.first() == Some(&0x30)).then_some(container)
}

/// Dekodiert ein Redirection-Paket ab `SEC_REDIRECTION_PKT`.
pub fn decode(b: &[u8]) -> Option<Redir> {
    let mut c = Cur { b, i: 0 };
    if c.u16()? != SEC_REDIRECTION_PKT {
        return None;
    }
    let length = c.u16()? as usize;
    if length < 12 {
        return None;
    }
    let session_id = c.u32()?;
    let flags = c.u32()?;
    let mut r = Redir { session_id, password_pk_encrypted: flags & F_PK != 0, ..Default::default() };
    if flags & F_ADDR != 0 {
        r.target_net_address = utf16(c.field()?);
    }
    if flags & F_LB != 0 {
        r.load_balance_info = Some(c.field()?.to_vec());
    }
    if flags & F_USER != 0 {
        r.username = utf16(c.field()?);
    }
    if flags & F_DOMAIN != 0 {
        r.domain = utf16(c.field()?);
    }
    if flags & F_PASSWORD != 0 {
        r.password = Some(c.field()?.to_vec());
    }
    if flags & F_FQDN != 0 {
        r.target_fqdn = utf16(c.field()?);
    }
    if flags & F_NETBIOS != 0 {
        r.target_netbios_name = utf16(c.field()?);
    }
    if flags & F_TSV != 0 {
        c.field()?;
    }
    if flags & F_GUID != 0 {
        c.field()?;
    }
    if flags & F_CERT != 0 {
        r.target_cert = target_cert(c.field()?);
    }
    if flags & F_ADDRS != 0 {
        let f = c.field()?;
        let mut a = Cur { b: f, i: 0 };
        let n = a.u32()?;
        for _ in 0..n.min(64) {
            if let Some(s) = a.field().and_then(utf16) {
                r.target_net_addresses.push(s);
            }
        }
    }
    Some(r)
}

/// Sucht das Redirection-Paket in einem Rahmen (TPKT/X.224/MCS/Share-Control-Kopf davor) und dekodiert es.
/// Nimmt den frühesten Treffer, der sich vollständig lesen lässt.
pub fn find(frame: &[u8]) -> Option<Redir> {
    (0..frame.len().saturating_sub(12))
        .filter(|&i| frame[i] == 0x00 && frame[i + 1] == 0x04)
        .find_map(|i| decode(&frame[i..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16z(s: &str) -> Vec<u8> {
        s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
    }

    fn field(out: &mut Vec<u8>, data: &[u8]) {
        out.extend((data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    }

    /// Paket wie es gnome-remote-desktop schicken könnte: Zertifikat als UTF-16-Base64 mit Null, kein Padding.
    fn packet(cert_der: &[u8]) -> Vec<u8> {
        let mut container = Vec::new();
        container.extend(32u32.to_le_bytes());
        container.extend(1u32.to_le_bytes());
        container.extend((cert_der.len() as u32).to_le_bytes());
        container.extend_from_slice(cert_der);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&container);

        let mut body = Vec::new();
        field(&mut body, b"Cookie: msts=3640205228.15629.0000\r\n");
        field(&mut body, &utf16z("mike"));
        field(&mut body, &utf16z("einmalig"));
        field(&mut body, &utf16z(&b64));

        let flags = F_LB | F_USER | F_PASSWORD | F_CERT;
        let mut p = Vec::new();
        p.extend(SEC_REDIRECTION_PKT.to_le_bytes());
        // Länge mit 8 Byte Padding angekündigt, das dann fehlt (kommt bei manchen Servern vor).
        p.extend(((12 + body.len() + 8) as u16).to_le_bytes());
        p.extend(42u32.to_le_bytes());
        p.extend(flags.to_le_bytes());
        p.extend(body);
        p
    }

    #[test]
    fn decodes_gnome_style_packet() {
        let der = [0x30, 0x82, 0x01, 0x0A, 1, 2, 3];
        let r = decode(&packet(&der)).unwrap();
        assert_eq!(r.session_id, 42);
        assert_eq!(r.username.as_deref(), Some("mike"));
        assert_eq!(r.password, Some(utf16z("einmalig")));
        assert_eq!(r.load_balance_info.as_deref(), Some(&b"Cookie: msts=3640205228.15629.0000\r\n"[..]));
        assert_eq!(r.target_cert.as_deref(), Some(&der[..]));
    }

    #[test]
    fn finds_packet_behind_headers() {
        let mut frame = vec![0x03, 0x00, 0x00, 0x99, 0x02, 0xF0, 0x80, 0x68, 0x00, 0x01, 0x03, 0xEB, 0x70, 0x81];
        frame.extend([0x00, 0x00, 0x0A, 0x00, 0x00, 0x00]); // Share-Control-Kopf, Pad
        frame.extend(packet(&[0x30, 0x03, 1, 2, 3]));
        let r = find(&frame).unwrap();
        assert_eq!(r.session_id, 42);
        assert!(find(&[0u8; 40]).is_none());
    }
}
