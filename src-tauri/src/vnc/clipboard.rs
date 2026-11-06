//! Extended Clipboard (Pseudo-Encoding 0xC0A1E5CE): UTF-8-Text statt Latin-1.
//! Nachrichten sind Client/ServerCutText mit negativer Länge; Nutzlast = Flags (u32) + Daten.

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::io::{Read, Write};

pub const TEXT: u32 = 1;
pub const CAPS: u32 = 1 << 24;
pub const REQUEST: u32 = 1 << 25;
pub const PEEK: u32 = 1 << 26;
pub const NOTIFY: u32 = 1 << 27;
pub const PROVIDE: u32 = 1 << 28;

pub const MAX_TEXT: u32 = 16 << 20;

fn client_cut_text(payload: &[u8]) -> Vec<u8> {
    let mut m = vec![6, 0, 0, 0];
    m.extend((-(payload.len() as i32)).to_be_bytes());
    m.extend_from_slice(payload);
    m
}

fn with_flags(flags: u32, rest: &[u8]) -> Vec<u8> {
    let mut p = flags.to_be_bytes().to_vec();
    p.extend_from_slice(rest);
    client_cut_text(&p)
}

/// Unsere Fähigkeiten: nur Text, alle Aktionen.
pub fn caps() -> Vec<u8> {
    with_flags(CAPS | REQUEST | PEEK | NOTIFY | PROVIDE | TEXT, &MAX_TEXT.to_be_bytes())
}

pub fn notify(has_text: bool) -> Vec<u8> {
    with_flags(NOTIFY | if has_text { TEXT } else { 0 }, &[])
}

pub fn request() -> Vec<u8> {
    with_flags(REQUEST | TEXT, &[])
}

/// Text mit CRLF und Null-Terminator, zlib-komprimiert (pro Nachricht ein eigener Strom).
pub fn provide(text: &str) -> Vec<u8> {
    let mut t = text.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes();
    t.push(0);
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(&(t.len() as u32).to_be_bytes()).and_then(|_| z.write_all(&t)).expect("Schreiben in Vec");
    with_flags(PROVIDE | TEXT, &z.finish().expect("Schreiben in Vec"))
}

/// Liest den Text aus einer Provide-Nutzlast (nach den Flags).
pub fn parse_provide(flags: u32, data: &[u8]) -> Result<Option<String>, String> {
    if flags & TEXT == 0 {
        return Ok(None);
    }
    let mut z = ZlibDecoder::new(data).take(MAX_TEXT as u64 + 4);
    let mut len = [0u8; 4];
    z.read_exact(&mut len).map_err(|e| format!("Zwischenablage: {e}"))?;
    let len = u32::from_be_bytes(len);
    if len > MAX_TEXT {
        return Err("Zwischenablage: Text zu gross".into());
    }
    let mut t = vec![0u8; len as usize];
    z.read_exact(&mut t).map_err(|e| format!("Zwischenablage: {e}"))?;
    if let Some(end) = t.iter().position(|&b| b == 0) {
        t.truncate(end);
    }
    Ok(Some(String::from_utf8_lossy(&t).replace("\r\n", "\n")))
}

/// Alte ClientCutText-Nachricht: nur Latin-1, andere Zeichen werden zu «?».
pub fn legacy(text: &str) -> Vec<u8> {
    let bytes: Vec<u8> = text.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect();
    let mut m = vec![6, 0, 0, 0];
    m.extend((bytes.len() as u32).to_be_bytes());
    m.extend(bytes);
    m
}

pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect::<String>().replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provide_roundtrip_utf8() {
        let msg = provide("Grüezi 👋\nZeile 2");
        let len = -i32::from_be_bytes(msg[4..8].try_into().unwrap()) as usize;
        assert_eq!(msg.len(), 8 + len);
        let flags = u32::from_be_bytes(msg[8..12].try_into().unwrap());
        assert_eq!(flags, PROVIDE | TEXT);
        assert_eq!(parse_provide(flags, &msg[12..]).unwrap().as_deref(), Some("Grüezi 👋\nZeile 2"));
    }

    #[test]
    fn legacy_is_latin1() {
        assert_eq!(&legacy("ä€")[8..], &[0xE4, b'?']);
        assert_eq!(latin1(&[0xE4, b'\r', b'\n']), "ä\n");
    }
}
