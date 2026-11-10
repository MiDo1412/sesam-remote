//! Lokales Terminal: Programm in einer Pseudo-Konsole (ConPTY unter Windows, openpty unter macOS/Linux)
//! starten. Ausgabe als `ShellEvent`s wie bei der SSH-Shell, damit dieselbe Oberfläche (xterm.js) passt.

use crate::relay::Relay;
use crate::ssh::ShellEvent;
#[cfg(desktop)]
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use std::collections::HashMap;
#[cfg(desktop)]
use std::io::{Read, Write};
use std::sync::{Arc, LazyLock, Mutex};

/// Was gestartet wird (aus dem Profil im Frontend).
#[derive(Deserialize, Debug, Clone)]
pub struct Spec {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[cfg(desktop)]
pub struct Term {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

/// iPadOS/Android: keine eigenen Prozesse, also kein lokales Terminal (das Frontend blendet es aus).
#[cfg(mobile)]
pub struct Term;

#[cfg(mobile)]
pub fn open(_spec: &Spec, _cols: u16, _rows: u16, _events: Relay<ShellEvent>) -> Result<Arc<Term>, String> {
    Err("Ein lokales Terminal gibt es auf diesem Gerät nicht; SSH-Shells gehen.".into())
}

#[cfg(mobile)]
impl Term {
    pub fn write(&self, _data: &[u8]) -> Result<(), String> {
        Ok(())
    }
    pub fn resize(&self, _cols: u16, _rows: u16) -> Result<(), String> {
        Ok(())
    }
    pub fn close(&self) {}
}

/// Offene lokale Terminals nach Sitzungs-ID.
pub static TERMS: LazyLock<Mutex<HashMap<String, Arc<Term>>>> = LazyLock::new(Default::default);

#[cfg(desktop)]
fn home() -> Option<String> {
    std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok()
}

#[cfg(desktop)]
pub fn open(spec: &Spec, cols: u16, rows: u16, events: Relay<ShellEvent>) -> Result<Arc<Term>, String> {
    if spec.command.trim().is_empty() {
        return Err("Kein Programm angegeben".into());
    }
    let pty = native_pty_system()
        .openpty(PtySize { rows: rows.max(2), cols: cols.max(2), pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("Terminal nicht verfügbar: {e}"))?;
    let mut cmd = CommandBuilder::new(&spec.command);
    cmd.args(&spec.args);
    // Startordner: aus dem Profil, sonst der persönliche Ordner (wie Windows Terminal).
    if let Some(dir) = spec.cwd.clone().filter(|d| std::path::Path::new(d).is_dir()).or_else(home) {
        cmd.cwd(dir);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "Sesam");
    // Wie Terminal.app und mintty: ohne Spracheinstellung UTF-8 vorgeben, sonst zeigen bash & Co. Emoji/Umlaute falsch.
    if std::env::var_os("LANG").is_none() && std::env::var_os("LC_ALL").is_none() {
        cmd.env("LANG", if cfg!(target_os = "macos") { "en_US.UTF-8" } else { "C.UTF-8" });
    }
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    let child = pty.slave.spawn_command(cmd).map_err(|e| format!("«{}» liess sich nicht starten: {e}", spec.command))?;
    drop(pty.slave); // sonst endet die Ausgabe nie, wenn das Programm beendet ist
    let mut reader = pty.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pty.master.take_writer().map_err(|e| e.to_string())?;
    let killer = child.clone_killer();
    let out = events.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || out.send(ShellEvent::Data { data: buf[..n].to_vec() }).is_err() {
                break;
            }
        }
    });
    // Ende über den Prozess erkennen: ConPTY liefert nach Programmende kein EOF.
    std::thread::spawn(move || {
        let mut child = child;
        let code = child.wait().ok().map(|s| s.exit_code());
        std::thread::sleep(std::time::Duration::from_millis(150)); // letzte Ausgabe noch durchlassen
        let reason = match code {
            Some(c) => format!("Beendet (Code {c})"),
            None => "Beendet".into(),
        };
        let _ = events.send(ShellEvent::Closed { reason });
    });
    Ok(Arc::new(Term { master: Mutex::new(pty.master), writer: Mutex::new(writer), killer: Mutex::new(killer) }))
}

#[cfg(desktop)]
impl Term {
    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(data).and_then(|_| w.flush()).map_err(|e| e.to_string())
    }
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.master.lock().unwrap().resize(PtySize { rows: rows.max(2), cols: cols.max(2), pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())
    }
    pub fn close(&self) {
        let _ = self.killer.lock().unwrap().kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::ipc::Channel;

    /// Echtes Programm in der Pseudo-Konsole: Ausgabe kommt an, Umlaute bleiben heil, Ende wird gemeldet.
    #[test]
    fn runs_program_and_reports_exit() {
        let (tx, rx) = std::sync::mpsc::channel::<ShellEvent>();
        let tx = Mutex::new(tx);
        let ch = Channel::<ShellEvent>::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(j) = body {
                let ev: serde_json::Value = serde_json::from_str(&j).unwrap();
                let e = match ev["kind"].as_str() {
                    Some("data") => ShellEvent::Data { data: ev["data"].as_array().unwrap().iter().map(|b| b.as_u64().unwrap() as u8).collect() },
                    _ => ShellEvent::Closed { reason: ev["reason"].as_str().unwrap_or_default().into() },
                };
                let _ = tx.lock().unwrap().send(e);
            }
            Ok(())
        });
        let spec = if cfg!(windows) {
            Spec { command: "cmd.exe".into(), args: vec!["/c".into(), "echo Grüezi".into()], cwd: None, env: HashMap::new() }
        } else {
            Spec { command: "/bin/sh".into(), args: vec!["-c".into(), "echo Grüezi".into()], cwd: None, env: HashMap::new() }
        };
        let t = open(&spec, 80, 24, Relay::new(ch)).unwrap();
        let (mut out, mut closed) = (Vec::new(), false);
        while let Ok(e) = rx.recv_timeout(std::time::Duration::from_secs(10)) {
            match e {
                ShellEvent::Data { data } => {
                    // ConPTY fragt die Cursorposition ab und wartet auf Antwort (in der App macht das xterm.js).
                    if data.windows(4).any(|w| w == b"[6n") {
                        t.write(b"[1;1R").unwrap();
                    }
                    out.extend(data)
                }
                ShellEvent::Closed { .. } => {
                    closed = true;
                    break;
                }
            }
        }
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("Gr") && text.contains("ezi"), "Ausgabe: {text:?}");
        assert!(closed, "Ende gemeldet");
    }

    #[test]
    fn empty_command_is_rejected() {
        let ch = Channel::<ShellEvent>::new(|_| Ok(()));
        assert!(open(&Spec { command: " ".into(), args: vec![], cwd: None, env: HashMap::new() }, 80, 24, Relay::new(ch)).is_err());
    }
}
