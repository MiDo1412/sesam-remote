//! Gemeinsame Zwischenablage für Dateien und Bilder: über SFTP, wenn die Verbindung SSH hat.
//! Lokal → Server: Dateien/Bilder nach `~/Sesam-Ablage`, Server-Zwischenablage bekommt die Pfade.
//! Server → lokal: besteht der Text nur aus `file://`-Adressen, werden die Dateien nach
//! `Downloads/Sesam-Ablage` geladen und als Dateien in die lokale Zwischenablage gelegt.

use crate::library::Connection;
use crate::sftp::{self, Sftp, rjoin};
use crate::ssh;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Was zuletzt abgeglichen wurde, damit nichts im Kreis läuft oder doppelt hochgeladen wird.
#[derive(PartialEq, Debug, Default)]
pub enum Last {
    #[default]
    Nothing,
    Text(String),
    Files(Vec<PathBuf>),
    Image(u64),
}

pub enum Local {
    Files(Vec<PathBuf>),
    Text(String),
    Image { width: usize, height: usize, rgba: Vec<u8> },
    Nothing,
}

/// Zwischenablage des Systems, immer nur von einem Thread zugleich: NSPasteboard ist auf dem Mac nicht
/// threadsicher (gleichzeitige Zugriffe aus VNC-Abgleich und RDP-Zwischenablage brachten Sesam zum Absturz).
pub fn clipboard<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>) -> Result<T, arboard::Error> {
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut arboard::Clipboard::new()?)
}

/// Reihenfolge: Dateien, Text, Bild (z. B. kopiert Word Text *und* ein Bild davon).
pub fn read_local() -> Local {
    clipboard(|cb| Ok(read_local_from(cb))).unwrap_or(Local::Nothing)
}

fn read_local_from(cb: &mut arboard::Clipboard) -> Local {
    if let Ok(files) = cb.get().file_list() {
        if !files.is_empty() {
            return Local::Files(files);
        }
    }
    if let Ok(t) = cb.get_text() {
        if !t.is_empty() {
            return Local::Text(t);
        }
    }
    if let Ok(img) = cb.get_image() {
        return Local::Image { width: img.width, height: img.height, rgba: img.bytes.into_owned() };
    }
    Local::Nothing
}

pub fn write_local_text(t: &str) {
    let _ = clipboard(|cb| cb.set_text(t));
}

fn write_local_files(files: &[PathBuf]) -> Result<(), String> {
    clipboard(|cb| cb.set().file_list(files)).map_err(|e| e.to_string())
}

pub fn hash(b: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

pub fn encode_png(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width as u32, height as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(rgba).map_err(|e| e.to_string())?;
    w.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// `file:///home/mike/a%20b.txt` → `/home/mike/a b.txt`. `None`, wenn nicht jede Zeile so eine Adresse ist.
pub fn file_uris(text: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() || lines.len() > 1000 {
        return None;
    }
    lines.into_iter().map(|l| l.strip_prefix("file://").and_then(|p| p.starts_with('/').then(|| percent_decode(p))?)).collect()
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub struct ClipShare {
    pub enabled: bool,
    conn: Connection,
    download_dir: PathBuf,
    sftp: tokio::sync::Mutex<Option<Arc<Sftp>>>,
    pub last: Mutex<Last>,
}

impl ClipShare {
    pub fn new(conn: Connection, download_dir: PathBuf) -> Self {
        Self { enabled: conn.shared_clipboard, conn, download_dir, sftp: Default::default(), last: Default::default() }
    }

    pub fn has_ssh(&self) -> bool {
        self.conn.ssh_port.is_some()
    }

    /// SFTP-Sitzung bei Bedarf öffnen. Der Server-Schlüssel muss schon bestätigt sein (Shell oder Dateien).
    async fn sftp(&self) -> Result<Arc<Sftp>, String> {
        let mut slot = self.sftp.lock().await;
        if let Some(s) = slot.as_ref() {
            return Ok(s.clone());
        }
        let target = crate::ssh_target(&self.conn, None)?;
        let s = Arc::new(sftp::open(target).await.map_err(|e| {
            if e.starts_with(ssh::HOSTKEY_UNKNOWN) {
                "Dateien über die Zwischenablage: den Server bitte zuerst einmal über «Shell» oder «Dateien» bestätigen.".to_string()
            } else {
                format!("Dateien über die Zwischenablage: {e}")
            }
        })?);
        *slot = Some(s.clone());
        Ok(s)
    }

    async fn ablage(s: &Sftp) -> Result<String, String> {
        let home = s.s.canonicalize(".").await.map_err(|e| e.to_string())?;
        let dir = rjoin(&home, "Sesam-Ablage");
        if !s.s.try_exists(&dir).await.map_err(|e| e.to_string())? {
            s.s.create_dir(&dir).await.map_err(|e| e.to_string())?;
        }
        Ok(dir)
    }

    /// Lokale Dateien hochladen; gibt die Pfade auf dem Server zurück.
    pub async fn upload_files(&self, files: &[PathBuf]) -> Result<Vec<String>, String> {
        let s = self.sftp().await?;
        let dir = Self::ablage(&s).await?;
        let mut out = Vec::new();
        for f in files {
            out.push(sftp::upload(&s, f, &dir, false, |_| {}).await?);
        }
        Ok(out)
    }

    /// Bild als PNG hochladen; gibt den Pfad auf dem Server zurück.
    pub async fn upload_png(&self, png: &[u8]) -> Result<String, String> {
        let s = self.sftp().await?;
        let dir = Self::ablage(&s).await?;
        let tmp = std::env::temp_dir().join(format!("Bild {}.png", chrono::Local::now().format("%Y-%m-%d %H.%M.%S")));
        std::fs::write(&tmp, png).map_err(|e| e.to_string())?;
        let r = sftp::upload(&s, &tmp, &dir, false, |_| {}).await;
        let _ = std::fs::remove_file(&tmp);
        r
    }

    /// Dateien vom Server laden und als Dateien in die lokale Zwischenablage legen.
    pub async fn download_to_clipboard(&self, remote: &[String]) -> Result<usize, String> {
        let s = self.sftp().await?;
        let dir = self.download_dir.join("Sesam-Ablage");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut local = Vec::new();
        for r in remote {
            local.push(PathBuf::from(sftp::download(&s, r, &dir, false, |_| {}).await?));
        }
        write_local_files(&local)?;
        *self.last.lock().unwrap() = Last::Files(local.clone());
        Ok(local.len())
    }

    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_pure_file_uri_lists() {
        assert_eq!(
            file_uris("file:///home/mike/a%20b.txt\r\nfile:///tmp/%C3%A4.png\n"),
            Some(vec!["/home/mike/a b.txt".into(), "/tmp/ä.png".into()])
        );
        assert_eq!(file_uris("/etc/hosts"), None, "reine Pfade sind Text, keine Dateien");
        assert_eq!(file_uris("file:///a\nHallo"), None);
        assert_eq!(file_uris("file://server/share"), None);
        assert_eq!(file_uris("file:///kaputt%zz"), None);
        assert_eq!(file_uris(""), None);
    }

    #[test]
    fn png_roundtrip() {
        let rgba = [255u8, 0, 0, 255, 0, 255, 0, 128];
        let png = encode_png(2, 1, &rgba).unwrap();
        let mut r = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let info = r.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(&buf[..8], &rgba);
        assert_ne!(hash(&rgba), hash(&[0u8; 8]));
    }
}
