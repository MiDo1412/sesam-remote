//! Diagnose-Log (`sesam.log` im App-Datenordner) für Fehlersuche bei RDP & Co.
//! Zugangsdaten gehören nie hinein: Verbindungsaufbau nur auf Info-Stufe, und jede Zeile wird
//! vor dem Schreiben um bekannte Geheimnisse bereinigt (`add_secret`).

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::Level;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;

static FILE: Mutex<Option<Log>> = Mutex::new(None);

/// Laufendes Log; ab `RUN_MAX` geschriebenen Bytes wechselt es im Betrieb auf eine neue Datei (Rotation wie beim Start).
struct Log {
    file: File,
    dir: PathBuf,
    written: u64,
}

impl Log {
    fn write(&mut self, b: &[u8], max: u64) {
        let _ = self.file.write_all(b);
        self.written += b.len() as u64;
        if self.written > max {
            rotate(&self.dir);
            if let Ok(f) = File::create(self.dir.join("sesam.log")) {
                (self.file, self.written) = (f, 0);
            }
        }
    }
}
static SECRETS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Geheimnis merken, damit es im Log als `***` erscheint (kurze Werte < 3 Zeichen nicht, sonst zu viele Treffer).
pub fn add_secret(s: &str) {
    if s.chars().count() >= 3 {
        let mut v = SECRETS.lock().unwrap();
        if !v.iter().any(|x| x == s) {
            v.push(s.to_string());
        }
    }
}

pub fn redact(line: &str) -> String {
    let mut out = line.to_string();
    for s in SECRETS.lock().unwrap().iter() {
        if out.contains(s.as_str()) {
            let clean = out.replace(s.as_str(), "***");
            zeroize::Zeroize::zeroize(&mut out);
            out = clean;
        }
    }
    out
}

/// Pro Ereignis gepuffert, beim Fallenlassen bereinigt in die Datei.
pub struct LineWriter(Vec<u8>);

impl Write for LineWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for LineWriter {
    fn drop(&mut self) {
        let line = redact(&String::from_utf8_lossy(&self.0));
        // Die ungefilterte Zeile kann ein Geheimnis enthalten.
        zeroize::Zeroize::zeroize(&mut self.0);
        if let Some(f) = FILE.lock().unwrap().as_mut() {
            f.write(line.as_bytes(), RUN_MAX);
        }
    }
}

struct Make;
impl<'a> MakeWriter<'a> for Make {
    type Writer = LineWriter;
    fn make_writer(&'a self) -> LineWriter {
        LineWriter(Vec::new())
    }
}

/// Höchstgrösse eines aufbewahrten Logs; von einem grösseren bleibt das Ende (dort steht, was zuletzt geschah).
const KEEP_MAX: u64 = 5 * 1024 * 1024;
/// So viel schreibt ein Lauf höchstens in eine Datei, bevor sie rotiert (lange laufende App, gesprächige Sitzungen).
const RUN_MAX: u64 = 10 * 1024 * 1024;

/// Logs der letzten Läufe aufbewahren: sesam.log → sesam.1.log → sesam.2.log (das älteste fällt weg).
/// So bleibt z. B. nachlesbar, was ein Update vor dem Neustart heruntergeladen und installiert hat.
fn rotate(dir: &Path) {
    let (cur, one, two) = (dir.join("sesam.log"), dir.join("sesam.1.log"), dir.join("sesam.2.log"));
    let _ = std::fs::rename(&one, &two);
    let Ok(len) = std::fs::metadata(&cur).map(|m| m.len()) else { return };
    if len <= KEEP_MAX {
        let _ = std::fs::rename(&cur, &one);
    } else if let Ok(mut f) = File::open(&cur) {
        use std::io::{Read, Seek, SeekFrom};
        let mut tail = Vec::new();
        if f.seek(SeekFrom::Start(len - KEEP_MAX)).is_ok() && f.read_to_end(&mut tail).is_ok() {
            let _ = std::fs::write(&one, tail);
        }
    }
}

/// Log neu anlegen (die zwei vorigen bleiben als sesam.1.log/sesam.2.log) und tracing einschalten. Gibt den Pfad zurück.
pub fn init(dir: &Path) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    rotate(dir);
    let path = dir.join("sesam.log");
    *FILE.lock().unwrap() = Some(Log { file: File::create(&path).ok()?, dir: dir.to_path_buf(), written: 0 });
    let filter = Targets::new()
        .with_default(Level::WARN)
        .with_target("sesam_remote_lib", Level::DEBUG)
        // Bild, EGFX, Kanäle: ausführlich. Verbindungsaufbau/CredSSP nur Info (dort stehen Zugangsdaten in Debug-Ausgaben).
        .with_target("ironrdp_session", Level::DEBUG)
        .with_target("ironrdp_egfx", Level::DEBUG)
        .with_target("ironrdp_dvc", Level::DEBUG)
        .with_target("ironrdp_displaycontrol", Level::DEBUG)
        .with_target("ironrdp_cliprdr", Level::INFO)
        .with_target("ironrdp_connector", Level::INFO)
        .with_target("ironrdp_tokio", Level::INFO)
        .with_target("ironrdp_async", Level::INFO)
        .with_target("sspi", Level::WARN);
    let layer = tracing_subscriber::fmt::layer().with_ansi(false).with_writer(Make);
    tracing_subscriber::registry().with(layer).with(filter).try_init().ok()?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Sesam Remote gestartet");
    Some(path)
}

/// Fehler mit allen Ursachen («a: b: c»), damit die Statuszeile mehr als «decode error» zeigt.
pub fn chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut out = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        let t = s.to_string();
        if !out.contains(&t) {
            out.push_str(": ");
            out.push_str(&t);
        }
        cur = s.source();
    }
    redact(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_redacted() {
        add_secret("geheim123");
        add_secret("ab"); // zu kurz, bleibt
        assert_eq!(redact("pw=geheim123 x=ab"), "pw=*** x=ab");
    }

    #[test]
    fn keeps_previous_logs() {
        let dir = std::env::temp_dir().join(format!("sesam-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let read = |n: &str| std::fs::read_to_string(dir.join(n)).unwrap_or_default();
        for run in ["lauf1", "lauf2", "lauf3"] {
            rotate(&dir);
            std::fs::write(dir.join("sesam.log"), run).unwrap();
        }
        rotate(&dir);
        assert_eq!((read("sesam.log"), read("sesam.1.log"), read("sesam.2.log")), (String::new(), "lauf3".into(), "lauf2".into()));
        // Zu grosses Log: nur das Ende bleibt.
        std::fs::write(dir.join("sesam.log"), vec![b'a'; (KEEP_MAX + 10) as usize]).unwrap();
        rotate(&dir);
        assert_eq!(std::fs::metadata(dir.join("sesam.1.log")).unwrap().len(), KEEP_MAX);
        assert_eq!(read("sesam.2.log"), "lauf3");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rotiert_im_laufenden_betrieb() {
        let dir = std::env::temp_dir().join(format!("sesam-runlog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut log = Log { file: File::create(dir.join("sesam.log")).unwrap(), dir: dir.clone(), written: 0 };
        log.write(b"aaaaa\n", 10);
        log.write(b"bbbbb\n", 10);
        log.write(b"c\n", 10);
        let read = |n: &str| std::fs::read_to_string(dir.join(n)).unwrap_or_default();
        assert_eq!((read("sesam.log"), read("sesam.1.log")), ("c\n".into(), "aaaaa\nbbbbb\n".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn chain_lists_sources() {
        #[derive(Debug)]
        struct Outer(std::io::Error);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("decode error")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let e = Outer(std::io::Error::other("invalid field cbData in GfxCapsConfirm"));
        assert_eq!(chain(&e), "decode error: invalid field cbData in GfxCapsConfirm");
    }
}
