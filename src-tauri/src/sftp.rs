//! Dateien über SFTP: Ordner auflisten (lokal und entfernt) und übertragen mit Fortschritt.
//! Übertragungen überschreiben nie: belegte Namen werden zu «Name (2).ext».

use crate::ssh::{self, Client, Target};
use russh::client::Handle;
use russh_sftp::client::SftpSession;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Papierkorb von Sesam im Home des Benutzers auf dem Server (versteckt).
pub const TRASH: &str = ".sesam-papierkorb";

pub struct Sftp {
    pub s: SftpSession,
    /// Hält die SSH-Verbindung offen und dient für Befehle wie `sha256sum` (in Tests ohne SSH).
    ssh: Option<Handle<Client>>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub async fn open(t: Target) -> Result<Sftp, String> {
    start(ssh::connect(&t).await?).await.map_err(|(_, e)| e)
}

/// SFTP auf einer bestehenden Verbindung starten. Scheitert es, kommt die Verbindung zurück (für SCP).
/// SFTP über eine bestehende SSH-Verbindung (z. B. die einer Shell), ohne sie zu übernehmen.
pub async fn over(h: &Handle<Client>) -> Result<Sftp, String> {
    let mut ch = h.channel_open_session().await.map_err(err)?;
    ch.request_subsystem(true, "sftp").await.map_err(err)?;
    loop {
        match ch.wait().await {
            Some(russh::ChannelMsg::Success) => break,
            Some(russh::ChannelMsg::Failure | russh::ChannelMsg::Close | russh::ChannelMsg::Eof) | None => return Err("Der Server bietet kein SFTP an".into()),
            _ => {}
        }
    }
    let s = SftpSession::new(ch.into_stream()).await.map_err(|e| format!("SFTP nicht verfügbar: {e}"))?;
    Ok(Sftp { s, ssh: None })
}

pub async fn start(h: Handle<Client>) -> Result<Sftp, (Handle<Client>, String)> {
    let stream = async {
        let mut ch = h.channel_open_session().await.map_err(err)?;
        ch.request_subsystem(true, "sftp").await.map_err(err)?;
        // Auf die Antwort warten: ohne SFTP-Subsystem lehnt der Server ab oder schliesst den Kanal.
        loop {
            match ch.wait().await {
                Some(russh::ChannelMsg::Success) => return Ok(ch.into_stream()),
                Some(russh::ChannelMsg::Failure | russh::ChannelMsg::Close | russh::ChannelMsg::Eof) | None => {
                    return Err("Der Server bietet kein SFTP an".to_string());
                }
                _ => {}
            }
        }
    }
    .await;
    match stream {
        Ok(st) => match SftpSession::new(st).await {
            Ok(s) => Ok(Sftp { s, ssh: Some(h) }),
            Err(e) => Err((h, format!("SFTP nicht verfügbar: {e}"))),
        },
        Err(e) => Err((h, e)),
    }
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    /// Sekunden seit 1970.
    pub modified: Option<u64>,
    /// Unix-Rechte (z. B. 0o755), nur bei SFTP.
    pub perm: Option<u32>,
}

#[derive(Serialize, Debug)]
pub struct Listing {
    pub path: String,
    /// `None` = oberste Ebene. Unter Windows führt "" zur Laufwerksliste.
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
}

fn sort(entries: &mut [Entry]) {
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
}

// ---------- Pfade ----------

pub fn rjoin(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

pub fn rparent(path: &str) -> Option<String> {
    let p = path.trim_end_matches('/');
    if p.is_empty() {
        return None;
    }
    let (head, _) = p.rsplit_once('/')?;
    Some(if head.is_empty() { "/".into() } else { head.into() })
}

/// «bericht.pdf» → «bericht (2).pdf»; Ordner und Dateien ohne Endung bekommen die Zahl hinten.
pub(crate) fn numbered(name: &str, n: u32, dir: bool) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !dir && !stem.is_empty() => format!("{stem} ({n}).{ext}"),
        _ => format!("{name} ({n})"),
    }
}

pub(crate) fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return Err("Ungültiger Name".into());
    }
    Ok(())
}

// ---------- Auflisten ----------

impl Sftp {
    pub async fn list(&self, path: &str) -> Result<Listing, String> {
        let path = self.s.canonicalize(if path.is_empty() { "." } else { path }).await.map_err(err)?;
        let mut entries = Vec::new();
        for e in self.s.read_dir(&path).await.map_err(err)? {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let mut m = e.metadata();
            if e.file_type().is_symlink() {
                if let Ok(target) = self.s.metadata(rjoin(&path, &name)).await {
                    m = target;
                }
            }
            let modified = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
            let dir = m.file_type().is_dir();
            entries.push(Entry { name, dir, size: if dir { 0 } else { m.len() }, modified, perm: m.permissions.map(|p| p & 0o7777) });
        }
        sort(&mut entries);
        Ok(Listing { parent: rparent(&path), path, entries })
    }

    pub async fn mkdir(&self, dir: &str, name: &str) -> Result<String, String> {
        valid_name(name)?;
        let p = rjoin(dir, name);
        self.s.create_dir(&p).await.map_err(err)?;
        Ok(p)
    }

    /// Im selben Ordner umbenennen; ein belegter Name wird nicht überschrieben.
    pub async fn rename(&self, path: &str, name: &str) -> Result<String, String> {
        valid_name(name)?;
        let dir = rparent(path).ok_or("Das Wurzelverzeichnis lässt sich nicht umbenennen")?;
        let to = rjoin(&dir, name);
        if self.s.try_exists(&to).await.map_err(err)? {
            return Err(format!("«{name}» gibt es dort schon"));
        }
        self.s.rename(path, &to).await.map_err(|e| format!("{path}: {e}"))?;
        Ok(to)
    }

    /// In den Papierkorb auf dem Server verschieben (`~/.sesam-papierkorb/<Zeitstempel>/<Name>`), statt zu löschen.
    /// Gibt den Pfad im Papierkorb zurück; scheitert z. B. bei einem anderen Dateisystem (dann fragt die Oberfläche).
    pub async fn to_trash(&self, path: &str) -> Result<String, String> {
        if rparent(path).is_none() {
            return Err("Das Wurzelverzeichnis lässt sich nicht löschen".into());
        }
        let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("Element").to_string();
        let home = self.s.canonicalize(".").await.map_err(err)?;
        let root = rjoin(&home, TRASH);
        if path == root || path.starts_with(&format!("{root}/")) {
            return Err("Liegt schon im Papierkorb".into());
        }
        let slot = self.trash_slot().await?;
        let dest = rjoin(&slot, &name);
        self.s.rename(path, &dest).await.map_err(|e| format!("Papierkorb: {e}"))?;
        Ok(dest)
    }

    /// Grösse in Bytes (Ordner samt Inhalt, über `du`; ohne SSH nur Dateien).
    pub async fn size(&self, path: &str) -> Result<u64, String> {
        let m = self.s.symlink_metadata(path).await.map_err(|e| format!("{path}: {e}"))?;
        if !m.file_type().is_dir() {
            return Ok(m.size.unwrap_or(0));
        }
        let out = self.exec(&format!("du -sk -- {} 2>/dev/null | cut -f1", crate::scp::quote(path)?), false, None).await?;
        Ok(out.trim().parse::<u64>().unwrap_or(0) * 1024)
    }

    /// Neuer Zeitstempel-Ordner im Papierkorb (für die Erhöhung mit sudo: dorthin verschiebt root).
    pub async fn trash_slot(&self) -> Result<String, String> {
        let root = rjoin(&self.s.canonicalize(".").await.map_err(err)?, TRASH);
        let slot = rjoin(&root, &chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string());
        // Mehrere Ersetzungen gleichzeitig können denselben Zeitstempel bekommen: «gibt es schon» ist dann in Ordnung.
        for d in [&root, &slot] {
            if !self.s.try_exists(d).await.map_err(err)? {
                if let Err(e) = self.s.create_dir(d).await {
                    if !self.s.try_exists(d).await.unwrap_or(false) {
                        return Err(format!("Papierkorb {d}: {e}"));
                    }
                }
            }
        }
        Ok(slot)
    }

    /// Aus dem Papierkorb an den alten Ort zurück (nur wenn dort nichts Neues liegt).
    pub async fn restore(&self, trashed: &str, original: &str) -> Result<(), String> {
        let home = self.s.canonicalize(".").await.map_err(err)?;
        if !trashed.starts_with(&format!("{}/", rjoin(&home, TRASH))) {
            return Err("Nur aus dem Papierkorb von Sesam".into());
        }
        if self.s.try_exists(original).await.map_err(err)? {
            self.remove(original).await?; // an seiner Stelle liegt die ersetzende Fassung
        }
        self.s.rename(trashed, original).await.map_err(|e| format!("Zurückholen: {e}"))?;
        if let Some(slot) = rparent(trashed) {
            let _ = self.s.remove_dir(&slot).await; // leerer Zeitstempel-Ordner
        }
        Ok(())
    }

    /// Papierkorb aufräumen: Zeitstempel-Ordner älter als `days` Tage löschen. Gibt die Zahl der übrigen zurück.
    pub async fn trash_cleanup(&self, days: u32) -> Result<usize, String> {
        let home = self.s.canonicalize(".").await.map_err(err)?;
        let root = rjoin(&home, TRASH);
        if !self.s.try_exists(&root).await.map_err(err)? {
            return Ok(0);
        }
        let cut = (chrono::Local::now() - chrono::Duration::days(i64::from(days))).format("%Y%m%d-%H%M%S").to_string();
        let mut left = 0;
        for e in self.s.read_dir(&root).await.map_err(err)? {
            let n = e.file_name();
            if n == "." || n == ".." {
                continue;
            }
            if n.len() >= 15 && n[..15] < *cut {
                let _ = self.remove(&rjoin(&root, &n)).await;
            } else {
                left += 1;
            }
        }
        Ok(left)
    }

    /// Datei oder Ordner samt Inhalt löschen. Verknüpfungen werden gelöscht, nicht verfolgt.
    pub async fn remove(&self, path: &str) -> Result<(), String> {
        if rparent(path).is_none() {
            return Err("Das Wurzelverzeichnis lässt sich nicht löschen".into());
        }
        let m = self.s.symlink_metadata(path).await.map_err(|e| format!("{path}: {e}"))?;
        if !m.file_type().is_dir() {
            return self.s.remove_file(path).await.map_err(|e| format!("{path}: {e}"));
        }
        // Erst alle Dateien, dann die Ordner von innen nach aussen.
        let (mut dirs, mut stack) = (Vec::new(), vec![path.to_string()]);
        while let Some(d) = stack.pop() {
            for e in self.s.read_dir(&d).await.map_err(|e| format!("{d}: {e}"))? {
                let n = e.file_name();
                if n == "." || n == ".." {
                    continue;
                }
                let p = rjoin(&d, &n);
                if e.file_type().is_dir() {
                    stack.push(p);
                } else {
                    self.s.remove_file(&p).await.map_err(|e| format!("{p}: {e}"))?;
                }
            }
            dirs.push(d);
        }
        for d in dirs.iter().rev() {
            self.s.remove_dir(d).await.map_err(|e| format!("{d}: {e}"))?;
        }
        Ok(())
    }

    /// Rechte setzen (nur die Bits 0o7777).
    pub async fn chmod(&self, path: &str, mode: u32) -> Result<(), String> {
        if mode > 0o7777 {
            return Err("Ungültige Rechte".into());
        }
        let mut a = russh_sftp::protocol::FileAttributes::empty();
        a.permissions = Some(mode);
        self.s.set_metadata(path, a).await.map_err(|e| format!("{path}: {e}"))
    }

    /// Leere Datei anlegen; ein belegter Name wird nicht überschrieben.
    pub async fn touch(&self, dir: &str, name: &str) -> Result<String, String> {
        use russh_sftp::protocol::OpenFlags;
        valid_name(name)?;
        let p = rjoin(dir, name);
        if self.s.try_exists(&p).await.map_err(err)? {
            return Err(format!("«{name}» gibt es dort schon"));
        }
        self.s.open_with_flags(&p, OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE).await.map_err(|e| format!("{p}: {e}"))?;
        Ok(p)
    }

    /// Shell-Befehl auf dem Server, auf Wunsch mit `sudo`. `password`: nur über stdin an sudo, nie als Argument;
    /// ohne Passwort `sudo -n` (klappt, wo sudo ohne Passwort erlaubt ist), sonst Fehler `SUDO_PASSWORD`.
    pub async fn exec(&self, script: &str, sudo: bool, password: Option<&str>) -> Result<String, String> {
        use russh::ChannelMsg;
        let h = self.ssh.as_ref().ok_or("Nur über SSH möglich")?;
        let q = crate::scp::quote(script)?;
        let cmd = match (sudo, password) {
            (false, _) => format!("sh -c {q}"),
            (true, None) => format!("sudo -n -- sh -c {q}"),
            (true, Some(_)) => format!("sudo -S -p '' -- sh -c {q}"),
        };
        let mut ch = h.channel_open_session().await.map_err(err)?;
        ch.exec(true, cmd).await.map_err(err)?;
        if let (true, Some(pw)) = (sudo, password) {
            crate::diag::add_secret(pw);
            ch.data(format!("{pw}\n").as_bytes()).await.map_err(err)?;
        }
        ch.eof().await.map_err(err)?;
        let (mut code, mut out, mut e) = (255u32, Vec::new(), Vec::new());
        while let Some(msg) = ch.wait().await {
            match msg {
                ChannelMsg::Data { data } => out.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => e.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => code = exit_status,
                ChannelMsg::Failure => return Err("Der Server lehnt Befehle ab (exec)".into()),
                _ => {}
            }
        }
        let e = String::from_utf8_lossy(&e).trim().to_string();
        match code {
            0 => Ok(String::from_utf8_lossy(&out).trim().to_string()),
            _ => Err(sudo_error(&e, code)),
        }
    }

    /// SHA-256 einer Datei auf dem Server, über einen exec-Kanal derselben Verbindung.
    pub async fn sha256(&self, path: &str) -> Result<String, String> {
        match &self.ssh {
            Some(h) => crate::scp::sha256(h, path).await,
            None => Err("Inhaltsprüfung nicht möglich: keine SSH-Verbindung".into()),
        }
    }

    async fn unique(&self, dir: &str, name: &str, is_dir: bool) -> Result<String, String> {
        let mut p = rjoin(dir, name);
        let mut n = 2;
        while self.s.try_exists(&p).await.map_err(err)? {
            p = rjoin(dir, &numbered(name, n, is_dir));
            n += 1;
        }
        Ok(p)
    }
}

/// Fehler von `sudo` in verständlichen Text; `SUDO_PASSWORD` = Passwort nötig (Oberfläche fragt einmal nach).
pub const SUDO_PASSWORD: &str = "SUDO_PASSWORD";
fn sudo_error(e: &str, code: u32) -> String {
    let l = e.to_lowercase();
    if l.contains("a password is required") || l.contains("no tty present") || l.contains("terminal is required") {
        SUDO_PASSWORD.into()
    } else if l.contains("incorrect password") || l.contains("sorry, try again") {
        "Das Passwort für sudo stimmt nicht.".into()
    } else if l.contains("not in the sudoers") || l.contains("not allowed to run sudo") {
        "Dieser Benutzer darf auf dem Server kein sudo verwenden.".into()
    } else if l.contains("sudo: command not found") || l.contains("sudo: not found") {
        "Auf dem Server gibt es kein sudo.".into()
    } else if e.is_empty() {
        format!("Befehl fehlgeschlagen (Code {code})")
    } else {
        e.to_string()
    }
}

/// Rechte und Besitzer als ein Skript: `chmod`/`chown`, auf Wunsch rekursiv. Besitzer als «benutzer», «benutzer:gruppe» oder «:gruppe».
pub fn perms_script(path: &str, mode: Option<u32>, owner: Option<&str>, recursive: bool) -> Result<String, String> {
    let q = crate::scp::quote(path)?;
    let r = if recursive { "-R " } else { "" };
    let mut parts = Vec::new();
    if let Some(m) = mode {
        if m > 0o7777 {
            return Err("Ungültige Rechte".into());
        }
        parts.push(format!("chmod {r}{m:o} -- {q}"));
    }
    if let Some(o) = owner.map(str::trim).filter(|o| !o.is_empty()) {
        if !o.chars().all(|c| c.is_ascii_alphanumeric() || "._-:".contains(c)) || o.matches(':').count() > 1 {
            return Err(format!("Ungültiger Besitzer: {o}"));
        }
        parts.push(format!("chown {r}{o} -- {q}"));
    }
    if parts.is_empty() {
        return Err("Nichts zu ändern".into());
    }
    Ok(parts.join(" && "))
}

/// Hochgeladenes aus einem temporären Ordner (von `mktemp -d`) als root an den Zielort kopieren. Neu: gehört root
/// wie üblich unter /opt, /etc. `replace` und das Ziel ist eine Datei: Inhalt hineinschreiben (`cat >`), so bleiben
/// Besitzer und Rechte der bestehenden Datei. Sonst nie überschreiben.
pub fn move_script(from: &str, dest_dir: &str, replace: bool) -> Result<String, String> {
    let tmp = rparent(from).ok_or("Ungültige Quelle")?;
    let name = from.rsplit('/').next().filter(|n| !n.is_empty()).ok_or("Ungültige Quelle")?;
    // Nur aus einem Ordner von mktemp (tmp.XXXX) und nur eine Ebene darunter: rm -rf läuft hier als root.
    if !tmp.rsplit('/').next().is_some_and(|d| d.starts_with("tmp.")) || name.contains("..") {
        return Err("Ungültiger temporärer Ordner".into());
    }
    let q = crate::scp::quote;
    let (f, d, t, tq) = (q(from)?, q(dest_dir)?, q(&rjoin(dest_dir, name))?, q(&tmp)?);
    let exists = if replace {
        format!("if [ -f {t} ] && [ -f {f} ]; then cat -- {f} > {t} && rm -f -- {f} && rmdir -- {tq}; exit $?; fi; ")
    } else {
        String::new()
    };
    Ok(format!("{exists}if [ -e {t} ]; then echo \"{EXISTS}«{name}» gibt es dort schon\" >&2; exit 3; fi; cp -R -- {f} {d}/ && rm -rf -- {f} && rmdir -- {tq}"))
}

/// Fehlertext-Anfang: Ziel belegt, die Oberfläche fragt «Ersetzen?».
pub const EXISTS: &str = "EXISTS:";

/// Zeitstempel-Ordner im Papierkorb als Ziel prüfen und für die Shell setzen.
fn trash_arg(arg: &str) -> Result<String, String> {
    if !arg.starts_with('/') || !arg.contains(&format!("/{TRASH}/")) || arg.contains("..") {
        return Err("Ungültiger Papierkorb".into());
    }
    crate::scp::quote(arg)
}

/// Einzelne Dateiaktion mit sudo, wenn dem Benutzer die Rechte fehlen.
pub fn sudo_op_script(op: &str, path: &str, arg: &str) -> Result<String, String> {
    let q = crate::scp::quote;
    let p = q(path)?;
    match op {
        "remove" => {
            if path.trim_end_matches('/').is_empty() || !path.starts_with('/') {
                return Err("Diesen Pfad löscht Sesam nicht".into());
            }
            Ok(format!("rm -rf -- {p}"))
        }
        "rename" => {
            valid_name(arg)?;
            let t = q(&rjoin(&rparent(path).ok_or("Ungültiger Pfad")?, arg))?;
            Ok(format!("if [ -e {t} ]; then echo \"«{arg}» gibt es dort schon\" >&2; exit 3; fi; mv -- {p} {t}"))
        }
        "mkdir" => {
            valid_name(arg)?;
            Ok(format!("mkdir -- {}", q(&rjoin(path, arg))?))
        }
        "touch" => {
            valid_name(arg)?;
            let t = q(&rjoin(path, arg))?;
            Ok(format!("if [ -e {t} ]; then echo \"«{arg}» gibt es dort schon\" >&2; exit 3; fi; : > {t}"))
        }
        // In den Papierkorb (Zeitstempel-Ordner von `trash_slot`) verschieben, statt zu löschen.
        "trash" => {
            if path.trim_end_matches('/').is_empty() {
                return Err("Ungültiger Pfad".into());
            }
            Ok(format!("mv -- {p} {}/", trash_arg(arg)?))
        }
        // Vor dem Ersetzen mit sudo: alte Fassung samt Besitzer und Rechten in den Papierkorb kopieren,
        // Besitz und Rechte in `.sesam-meta` merken, dann dem Benutzer übergeben (damit er sie sehen und zurückholen kann).
        "backup" => {
            let slot = trash_arg(arg)?;
            Ok(format!("cp -a -- {p} {slot}/ && {{ stat -c '%u:%g %a' -- {p} 2>/dev/null || stat -f '%u:%g %Lp' -- {p}; }} > {slot}/.sesam-meta 2>/dev/null; chown -R \"$SUDO_USER\" {slot}"))
        }
        // Zurückholen mit sudo: Inhalt an den Ursprungsort, Besitz und Rechte wie gemerkt.
        "restore" => {
            if !path.contains(&format!("/{TRASH}/")) || path.contains("..") || !arg.starts_with('/') {
                return Err("Nur aus dem Papierkorb von Sesam".into());
            }
            let slot = rparent(path).ok_or("Ungültiger Pfad")?;
            let meta = q(&format!("{slot}/.sesam-meta"))?;
            let (o, slot) = (q(arg)?, trash_arg(&slot)?);
            Ok(format!("cp -a -- {p} {o} && if [ -s {meta} ]; then set -- $(cat {meta}); {{ chown \"$1\" {o} 2>/dev/null || true; }} && chmod \"$2\" {o}; fi && rm -rf -- {slot}"))
        }
        // Zum Herunterladen: Kopie in einen temporären Ordner, die dem angemeldeten Benutzer gehört.
        "fetch" => {
            if !arg.rsplit('/').next().is_some_and(|d| d.starts_with("tmp.")) {
                return Err("Ungültiger temporärer Ordner".into());
            }
            let t = q(arg)?;
            Ok(format!("cp -R -- {p} {t}/ && chown -R \"$SUDO_USER\" -- {t}"))
        }
        _ => Err("Unbekannte Aktion".into()),
    }
}

/// Lokal: Ordner anlegen.
pub fn local_mkdir(dir: &str, name: &str) -> Result<String, String> {
    local_name(name)?;
    let p = Path::new(dir).join(name);
    std::fs::create_dir(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p.to_string_lossy().into_owned())
}

/// Lokal: im selben Ordner umbenennen, ohne zu überschreiben.
pub fn local_rename(path: &str, name: &str) -> Result<String, String> {
    local_name(name)?;
    let from = Path::new(path);
    let to = from.parent().ok_or("Laufwerke lassen sich nicht umbenennen")?.join(name);
    if to.exists() {
        return Err(format!("«{name}» gibt es dort schon"));
    }
    std::fs::rename(from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
    Ok(to.to_string_lossy().into_owned())
}

fn local_name(name: &str) -> Result<(), String> {
    valid_name(name)?;
    if name.contains('\\') || (cfg!(windows) && name.contains([':', '*', '?', '"', '<', '>', '|'])) {
        return Err("Ungültiger Name".into());
    }
    Ok(())
}

pub fn local_list(path: &str) -> Result<Listing, String> {
    #[cfg(windows)]
    if path.is_empty() {
        let entries = (b'A'..=b'Z')
            .map(|d| format!("{}:\\", d as char))
            .filter(|d| Path::new(d).exists())
            .map(|name| Entry { name, dir: true, size: 0, modified: None, perm: None })
            .collect();
        return Ok(Listing { path: String::new(), parent: None, entries });
    }
    let p = std::path::absolute(path).map_err(err)?;
    let mut entries = Vec::new();
    for e in std::fs::read_dir(&p).map_err(|e| format!("{}: {e}", p.display()))? {
        let Ok(e) = e else { continue };
        // metadata() folgt Verknüpfungen; defekte überspringen.
        let Ok(m) = std::fs::metadata(e.path()) else { continue };
        let modified = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
        let size = if m.is_dir() { 0 } else { m.len() };
        entries.push(Entry { name: e.file_name().to_string_lossy().into_owned(), dir: m.is_dir(), size, modified, perm: None });
    }
    sort(&mut entries);
    let parent = match p.parent() {
        Some(par) => Some(par.to_string_lossy().into_owned()),
        None if cfg!(windows) => Some(String::new()),
        None => None,
    };
    Ok(Listing { path: p.to_string_lossy().into_owned(), parent, entries })
}

/// SHA-256 einer lokalen Datei, hexadezimal.
pub fn local_sha256(path: &str) -> Result<String, String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut h = sha2::Sha256::new();
    std::io::copy(&mut f, &mut h).map_err(|e| format!("{path}: {e}"))?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn local_unique(dir: &Path, name: &str, is_dir: bool) -> PathBuf {
    let mut p = dir.join(name);
    let mut n = 2;
    while p.exists() {
        p = dir.join(numbered(name, n, is_dir));
        n += 1;
    }
    p
}

// ---------- Übertragen ----------

#[derive(Clone, Serialize, Debug)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Progress {
    Running { done: u64, total: u64, file: String },
    Done { target: String },
    Failed { error: String },
}

/// Endung, unter der eine Datei geschrieben wird, bis sie vollständig ist.
pub const PARTIAL: &str = ".sesam-teil";

/// Löscht eine halb geschriebene Datei, wenn die Übertragung scheitert oder abgebrochen wird
/// (auch beim Abbruch der Aufgabe, darum im Drop).
pub(crate) struct Partial(pub(crate) Option<Cleanup>);

pub(crate) enum Cleanup {
    Local(PathBuf),
    Remote(Arc<Sftp>, String),
}

impl Drop for Partial {
    fn drop(&mut self) {
        match self.0.take() {
            Some(Cleanup::Local(p)) => {
                let _ = std::fs::remove_file(p);
            }
            Some(Cleanup::Remote(s, p)) => {
                if let Ok(rt) = tokio::runtime::Handle::try_current() {
                    rt.spawn(async move {
                        let _ = s.s.remove_file(p).await;
                    });
                }
            }
            None => {}
        }
    }
}

/// Ein Schritt der Übertragung: Ordner anlegen oder Datei kopieren.
struct Item {
    src: String,
    dst: String,
    dir: bool,
    /// Änderungszeit der Quelle (Unix-Sekunden), wird am Ziel übernommen, damit ein Vergleich danach «gleich» zeigt.
    mtime: Option<u32>,
}

fn unix_secs(t: std::io::Result<std::time::SystemTime>) -> Option<u32> {
    t.ok()?.duration_since(std::time::UNIX_EPOCH).ok().and_then(|d| u32::try_from(d.as_secs()).ok())
}

/// Lokale Änderungszeit setzen (bestmöglich; ein Fehler bricht die Übertragung nicht ab).
fn set_local_mtime(path: &str, mtime: Option<u32>) {
    if let Some(t) = mtime {
        let when = std::time::UNIX_EPOCH + Duration::from_secs(u64::from(t));
        if let Ok(f) = std::fs::File::options().write(true).open(path) {
            let _ = f.set_modified(when);
        }
    }
}

pub(crate) struct Meter<F: Fn(Progress)> {
    pub(crate) done: u64,
    pub(crate) total: u64,
    pub(crate) last: Instant,
    pub(crate) report: F,
}

impl<F: Fn(Progress)> Meter<F> {
    pub(crate) fn add(&mut self, n: u64, file: &str) {
        self.done += n;
        if self.last.elapsed() >= Duration::from_millis(100) {
            self.last = Instant::now();
            (self.report)(Progress::Running { done: self.done, total: self.total, file: file.to_string() });
        }
    }
}

async fn pump<R, W, F>(mut r: R, mut w: W, m: &mut Meter<F>, file: &str) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: Fn(Progress),
{
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = r.read(&mut buf).await.map_err(err)?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n]).await.map_err(err)?;
        m.add(n as u64, file);
    }
    w.shutdown().await.map_err(err)
}

/// Lokale Datei oder lokalen Ordner nach `remote_dir` hochladen. Gibt den Zielpfad zurück.
/// `replace` (nur für eine einzelne Datei, nach ausdrücklicher Rückfrage): gleichnamige Datei ersetzen.
pub async fn upload(sftp: &Arc<Sftp>, local: &Path, remote_dir: &str, replace: bool, report: impl Fn(Progress)) -> Result<String, String> {
    let name = local.file_name().ok_or("Ungültige Quelle")?.to_string_lossy().into_owned();
    let replace = replace && local.is_file();
    let root = if replace { rjoin(remote_dir, &name) } else { sftp.unique(remote_dir, &name, local.is_dir()).await? };

    // Plan: Ordner zuerst, dann Dateien; lokal reicht ein synchroner Durchlauf.
    let (mut plan, mut total) = (Vec::new(), 0u64);
    let mut stack = vec![(local.to_path_buf(), root.clone())];
    while let Some((src, dst)) = stack.pop() {
        let m = std::fs::metadata(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        if m.is_dir() {
            plan.push(Item { src: src.to_string_lossy().into(), dst: dst.clone(), dir: true, mtime: None });
            for e in std::fs::read_dir(&src).map_err(err)?.flatten() {
                stack.push((e.path(), rjoin(&dst, &e.file_name().to_string_lossy())));
            }
        } else {
            total += m.len();
            plan.push(Item { src: src.to_string_lossy().into(), dst, dir: false, mtime: unix_secs(m.modified()) });
        }
    }

    let mut m = Meter { done: 0, total, last: Instant::now(), report };
    for it in plan {
        if it.dir {
            sftp.s.create_dir(&it.dst).await.map_err(|e| format!("{}: {e}", it.dst))?;
        } else {
            let tmp = format!("{}{PARTIAL}", it.dst);
            let r = tokio::fs::File::open(&it.src).await.map_err(|e| format!("{}: {e}", it.src))?;
            let w = sftp.s.create(&tmp).await.map_err(|e| format!("{}: {e}", it.dst))?;
            let mut guard = Partial(Some(Cleanup::Remote(sftp.clone(), tmp.clone())));
            pump(r, w, &mut m, &it.src).await?;
            if replace {
                // SFTP-Umbenennen überschreibt nicht; die alte Datei erst weg, wenn die neue vollständig da ist.
                let _ = sftp.s.remove_file(&it.dst).await;
            }
            sftp.s.rename(&tmp, &it.dst).await.map_err(|e| format!("{}: {e}", it.dst))?;
            guard.0 = None;
            if let Some(t) = it.mtime {
                // SFTP setzt Zugriffs- und Änderungszeit nur gemeinsam.
                let mut a = russh_sftp::protocol::FileAttributes::empty();
                a.atime = Some(t);
                a.mtime = Some(t);
                let _ = sftp.s.set_metadata(&it.dst, a).await;
            }
        }
    }
    (m.report)(Progress::Done { target: root.clone() });
    Ok(root)
}

/// Entfernte Datei oder entfernten Ordner nach `local_dir` herunterladen. Gibt den Zielpfad zurück.
/// `replace` wie bei `upload` (Umbenennen ersetzt lokal eine Datei, nie einen Ordner).
pub async fn download(sftp: &Arc<Sftp>, remote: &str, local_dir: &Path, replace: bool, report: impl Fn(Progress)) -> Result<String, String> {
    let meta = sftp.s.metadata(remote).await.map_err(|e| format!("{remote}: {e}"))?;
    let name = remote.trim_end_matches('/').rsplit('/').next().filter(|n| !n.is_empty()).ok_or("Ungültige Quelle")?;
    crate::scp::check_name(name)?;
    let is_dir = meta.file_type().is_dir();
    let root = if replace && !is_dir { local_dir.join(name) } else { local_unique(local_dir, name, is_dir) };

    let (mut plan, mut total) = (Vec::new(), 0u64);
    let mut stack = vec![(remote.to_string(), root.clone(), meta)];
    while let Some((src, dst, m)) = stack.pop() {
        if m.file_type().is_dir() {
            plan.push(Item { src: src.clone(), dst: dst.to_string_lossy().into(), dir: true, mtime: None });
            for e in sftp.s.read_dir(&src).await.map_err(err)? {
                let n = e.file_name();
                // Nur echte Dateien und Ordner; Verknüpfungen nicht verfolgen (keine Schleifen).
                // Namen vom Server, die lokal aus dem Zielordner führen würden (z. B. «..\x» unter Windows), auslassen.
                if crate::scp::check_name(&n).is_ok() && !e.file_type().is_symlink() {
                    stack.push((rjoin(&src, &n), dst.join(&n), e.metadata()));
                }
            }
        } else {
            total += m.len();
            plan.push(Item { src, dst: dst.to_string_lossy().into(), dir: false, mtime: m.mtime });
        }
    }

    let mut m = Meter { done: 0, total, last: Instant::now(), report };
    for it in plan {
        if it.dir {
            tokio::fs::create_dir_all(&it.dst).await.map_err(|e| format!("{}: {e}", it.dst))?;
        } else {
            let tmp = PathBuf::from(format!("{}{PARTIAL}", it.dst));
            let r = sftp.s.open(&it.src).await.map_err(|e| format!("{}: {e}", it.src))?;
            let w = tokio::fs::File::create(&tmp).await.map_err(|e| format!("{}: {e}", it.dst))?;
            let mut guard = Partial(Some(Cleanup::Local(tmp.clone())));
            pump(r, w, &mut m, &it.src).await?;
            tokio::fs::rename(&tmp, &it.dst).await.map_err(|e| format!("{}: {e}", it.dst))?;
            guard.0 = None;
            set_local_mtime(&it.dst, it.mtime);
        }
    }
    let root = root.to_string_lossy().into_owned();
    (m.report)(Progress::Done { target: root.clone() });
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtime_bleibt_erhalten() {
        let p = std::env::temp_dir().join(format!("sesam-mtime-{}", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        set_local_mtime(&p.to_string_lossy(), Some(1_600_000_000));
        assert_eq!(unix_secs(std::fs::metadata(&p).unwrap().modified()), Some(1_600_000_000));
        let _ = std::fs::remove_file(&p);
    }
    use russh_sftp::protocol::{Attrs, Data, File, FileAttributes, Handle as H, Name, OpenFlags, Status, StatusCode, Version};
    use std::collections::{BTreeMap, HashSet};
    use std::sync::Mutex;

    /// Kleiner SFTP-Server im Arbeitsspeicher als Gegenstelle für Hoch-/Herunterladen.
    #[derive(Default)]
    struct MemFs {
        /// Pfad → `None` = Ordner, `Some(daten)` = Datei.
        nodes: BTreeMap<String, Option<Vec<u8>>>,
        /// Schreiben scheitert, sobald eine Datei grösser würde (simuliert volle Platte / Abbruch).
        fail_above: Option<usize>,
    }

    struct Server {
        fs: Arc<Mutex<MemFs>>,
        listed: HashSet<String>,
    }

    fn ok(id: u32) -> Status {
        Status { id, status_code: StatusCode::Ok, error_message: "Ok".into(), language_tag: "en-US".into() }
    }

    fn attrs(node: &Option<Vec<u8>>) -> FileAttributes {
        let mut a = FileAttributes::empty();
        match node {
            None => a.set_dir(true),
            Some(d) => {
                a.set_regular(true);
                a.size = Some(d.len() as u64);
            }
        }
        a
    }

    impl russh_sftp::server::Handler for Server {
        type Error = StatusCode;
        fn unimplemented(&self) -> StatusCode {
            StatusCode::OpUnsupported
        }
        async fn init(&mut self, _: u32, _: std::collections::HashMap<String, String>) -> Result<Version, StatusCode> {
            Ok(Version::new())
        }
        async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
            let p = if path == "." { "/home".into() } else { path };
            Ok(Name { id, files: vec![File::dummy(p)] })
        }
        async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
            let fs = self.fs.lock().unwrap();
            fs.nodes.get(&path).map(|n| Attrs { id, attrs: attrs(n) }).ok_or(StatusCode::NoSuchFile)
        }
        async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
            self.stat(id, path).await
        }
        async fn mkdir(&mut self, id: u32, path: String, _: FileAttributes) -> Result<Status, StatusCode> {
            let mut fs = self.fs.lock().unwrap();
            if fs.nodes.contains_key(&path) {
                return Err(StatusCode::Failure);
            }
            fs.nodes.insert(path, None);
            Ok(ok(id))
        }
        async fn opendir(&mut self, id: u32, path: String) -> Result<H, StatusCode> {
            self.listed.remove(&path);
            Ok(H { id, handle: path })
        }
        async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, StatusCode> {
            if !self.listed.insert(handle.clone()) {
                return Err(StatusCode::Eof);
            }
            let fs = self.fs.lock().unwrap();
            let prefix = format!("{}/", handle.trim_end_matches('/'));
            let files = fs
                .nodes
                .iter()
                .filter_map(|(p, n)| p.strip_prefix(&prefix).filter(|rest| !rest.contains('/')).map(|name| File::new(name, attrs(n))))
                .collect();
            Ok(Name { id, files })
        }
        async fn open(&mut self, id: u32, path: String, flags: OpenFlags, _: FileAttributes) -> Result<H, StatusCode> {
            let mut fs = self.fs.lock().unwrap();
            if flags.contains(OpenFlags::CREATE) {
                fs.nodes.insert(path.clone(), Some(Vec::new()));
            } else if !matches!(fs.nodes.get(&path), Some(Some(_))) {
                return Err(StatusCode::NoSuchFile);
            }
            Ok(H { id, handle: path })
        }
        async fn write(&mut self, id: u32, handle: String, offset: u64, data: Vec<u8>) -> Result<Status, StatusCode> {
            let mut fs = self.fs.lock().unwrap();
            let fs_limit = fs.fail_above;
            let Some(Some(f)) = fs.nodes.get_mut(&handle) else { return Err(StatusCode::Failure) };
            let end = offset as usize + data.len();
            if fs_limit.is_some_and(|l| end > l) {
                return Err(StatusCode::Failure);
            }
            if f.len() < end {
                f.resize(end, 0);
            }
            f[offset as usize..end].copy_from_slice(&data);
            Ok(ok(id))
        }
        async fn read(&mut self, id: u32, handle: String, offset: u64, len: u32) -> Result<Data, StatusCode> {
            let fs = self.fs.lock().unwrap();
            let Some(Some(f)) = fs.nodes.get(&handle) else { return Err(StatusCode::Failure) };
            if offset as usize >= f.len() {
                return Err(StatusCode::Eof);
            }
            let end = f.len().min(offset as usize + len as usize);
            Ok(Data { id, data: f[offset as usize..end].to_vec() })
        }
        async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
            Ok(ok(id))
        }
        async fn rename(&mut self, id: u32, from: String, to: String) -> Result<Status, StatusCode> {
            let mut fs = self.fs.lock().unwrap();
            if fs.nodes.contains_key(&to) {
                return Err(StatusCode::Failure);
            }
            let moved: Vec<_> = fs.nodes.keys().filter(|k| *k == &from || k.starts_with(&format!("{from}/"))).cloned().collect();
            if moved.is_empty() {
                return Err(StatusCode::NoSuchFile);
            }
            for k in moved {
                let v = fs.nodes.remove(&k).unwrap();
                fs.nodes.insert(format!("{to}{}", &k[from.len()..]), v);
            }
            Ok(ok(id))
        }
        async fn remove(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
            let mut fs = self.fs.lock().unwrap();
            match fs.nodes.get(&path) {
                Some(Some(_)) => {
                    fs.nodes.remove(&path);
                    Ok(ok(id))
                }
                _ => Err(StatusCode::NoSuchFile),
            }
        }
        async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
            let mut fs = self.fs.lock().unwrap();
            let prefix = format!("{path}/");
            if fs.nodes.keys().any(|k| k.starts_with(&prefix)) {
                return Err(StatusCode::Failure);
            }
            match fs.nodes.remove(&path) {
                Some(None) => Ok(ok(id)),
                _ => Err(StatusCode::NoSuchFile),
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn papierkorb_und_zurueckholen() {
        let fs = Arc::new(Mutex::new(MemFs::default()));
        fs.lock().unwrap().nodes.insert("/home".into(), None);
        fs.lock().unwrap().nodes.insert("/home/a.txt".into(), Some(b"alt".to_vec()));
        let (client, server) = tokio::io::duplex(1 << 20);
        russh_sftp::server::run(server, Server { fs: fs.clone(), listed: HashSet::new() }).await;
        let sftp = Sftp { s: SftpSession::new(client).await.unwrap(), ssh: None };
        let t = sftp.to_trash("/home/a.txt").await.unwrap();
        assert!(t.starts_with("/home/.sesam-papierkorb/") && t.ends_with("/a.txt"), "{t}");
        assert!(!fs.lock().unwrap().nodes.contains_key("/home/a.txt"));
        // Inzwischen liegt eine neue Fassung dort: Zurückholen ersetzt sie durch die alte.
        fs.lock().unwrap().nodes.insert("/home/a.txt".into(), Some(b"neu".to_vec()));
        sftp.restore(&t, "/home/a.txt").await.unwrap();
        assert_eq!(fs.lock().unwrap().nodes["/home/a.txt"].as_deref(), Some(b"alt".as_slice()));
        assert!(sftp.restore("/etc/passwd", "/home/x").await.is_err(), "nur aus dem Papierkorb");
        assert!(sftp.to_trash("/home/.sesam-papierkorb").await.is_err());
        // Aufräumen: frische Einträge bleiben.
        let _ = sftp.to_trash("/home/a.txt").await.unwrap();
        assert_eq!(sftp.trash_cleanup(7).await.unwrap(), 1);
        assert_eq!(sftp.trash_cleanup(0).await.unwrap(), 1, "Zeitstempel dieser Sekunde zählt noch nicht als alt");
    }

    #[tokio::test]
    async fn upload_and_download_roundtrip() {
        let fs = Arc::new(Mutex::new(MemFs::default()));
        fs.lock().unwrap().nodes.insert("/home".into(), None);
        let (client, server) = tokio::io::duplex(1 << 20);
        russh_sftp::server::run(server, Server { fs: fs.clone(), listed: HashSet::new() }).await;
        let sftp = Arc::new(Sftp { s: SftpSession::new(client).await.unwrap(), ssh: None });

        // Lokaler Ordner mit Unterordner und einer Datei über mehrere Puffer (256 KB).
        let base = std::env::temp_dir().join(format!("sesam-xfer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("projekt");
        std::fs::create_dir_all(src.join("unter")).unwrap();
        std::fs::write(src.join("a.txt"), "Grüezi").unwrap();
        let big: Vec<u8> = (0..700_000u32).map(|i| (i * 7 % 251) as u8).collect();
        std::fs::write(src.join("unter").join("gross.bin"), &big).unwrap();

        let last = Arc::new(Mutex::new(None));
        let l2 = last.clone();
        let target = upload(&sftp, &src, "/home", false, move |p| *l2.lock().unwrap() = Some(p)).await.unwrap();
        assert_eq!(target, "/home/projekt");
        assert!(matches!(last.lock().unwrap().take(), Some(Progress::Done { .. })));
        {
            let fs = fs.lock().unwrap();
            assert_eq!(fs.nodes["/home/projekt/a.txt"].as_deref(), Some("Grüezi".as_bytes()));
            assert_eq!(fs.nodes["/home/projekt/unter/gross.bin"].as_deref(), Some(big.as_slice()));
        }

        // Zweites Hochladen überschreibt nicht.
        assert_eq!(upload(&sftp, &src, "/home", false, |_| {}).await.unwrap(), "/home/projekt (2)");
        let names: Vec<_> = sftp.list("/home").await.unwrap().entries.into_iter().map(|e| (e.name, e.dir)).collect();
        assert_eq!(names, [("projekt".to_string(), true), ("projekt (2)".to_string(), true)]);

        // Zurück auf diesen Computer, neben das Original: wird «projekt (2)».
        let back = download(&sftp, "/home/projekt", &base, false, |_| {}).await.unwrap();
        assert_eq!(PathBuf::from(&back), base.join("projekt (2)"));
        assert_eq!(std::fs::read(base.join("projekt (2)").join("unter").join("gross.bin")).unwrap(), big);
        assert_eq!(std::fs::read_to_string(base.join("projekt (2)").join("a.txt")).unwrap(), "Grüezi");

        assert!(download(&sftp, "/home/fehlt", &base, false, |_| {}).await.is_err());

        // Ersetzen nur auf ausdrücklichen Wunsch: gleicher Name, neuer Inhalt, keine «(2)».
        std::fs::write(src.join("a.txt"), "Neu").unwrap();
        assert_eq!(upload(&sftp, &src.join("a.txt"), "/home/projekt", true, |_| {}).await.unwrap(), "/home/projekt/a.txt");
        assert_eq!(fs.lock().unwrap().nodes["/home/projekt/a.txt"].as_deref(), Some("Neu".as_bytes()));
        fs.lock().unwrap().nodes.insert("/home/projekt/a.txt".into(), Some(b"Server".to_vec()));
        let a = download(&sftp, "/home/projekt/a.txt", &src, true, |_| {}).await.unwrap();
        assert_eq!(PathBuf::from(&a), src.join("a.txt"));
        assert_eq!(std::fs::read_to_string(src.join("a.txt")).unwrap(), "Server");
        // Ohne Ersetzen bleibt es beim Nummerieren.
        assert!(download(&sftp, "/home/projekt/a.txt", &src, false, |_| {}).await.unwrap().ends_with("a (2).txt"));
        std::fs::remove_file(src.join("a (2).txt")).unwrap();
        std::fs::write(src.join("a.txt"), "Grüezi").unwrap();

        // Umbenennen und rekursiv löschen.
        assert_eq!(sftp.rename("/home/projekt (2)", "kopie").await.unwrap(), "/home/kopie");
        assert!(sftp.rename("/home/kopie", "projekt").await.unwrap_err().contains("gibt es dort schon"));
        assert!(sftp.rename("/home/kopie", "a/b").await.is_err());
        sftp.remove("/home/kopie").await.unwrap();
        assert!(!fs.lock().unwrap().nodes.keys().any(|k| k.starts_with("/home/kopie")));
        assert!(sftp.remove("/").await.is_err());

        // Keine Teildateien übrig, weder hier noch dort.
        assert!(!fs.lock().unwrap().nodes.keys().any(|k| k.ends_with(PARTIAL)));
        let walk = |d: &Path| std::fs::read_dir(d).unwrap().flatten().any(|e| e.file_name().to_string_lossy().ends_with(PARTIAL));
        assert!(!walk(&base.join("projekt (2)")) && !walk(&base.join("projekt (2)").join("unter")));

        // Scheitert das Schreiben mittendrin, verschwindet die halbe Datei.
        fs.lock().unwrap().fail_above = Some(300_000);
        let e = upload(&sftp, &src.join("unter").join("gross.bin"), "/home", false, |_| {}).await;
        assert!(e.is_err());
        tokio::time::sleep(Duration::from_millis(50)).await; // Aufräumen läuft im Hintergrund
        let fs2 = fs.lock().unwrap();
        assert!(!fs2.nodes.keys().any(|k| k.ends_with(PARTIAL) || k == "/home/gross.bin"), "{:?}", fs2.nodes.keys().collect::<Vec<_>>());
        drop(fs2);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn remote_paths() {
        assert_eq!(rjoin("/", "etc"), "/etc");
        assert_eq!(rjoin("/home/mike", "a b"), "/home/mike/a b");
        assert_eq!(rparent("/home/mike"), Some("/home".into()));
        assert_eq!(rparent("/home/"), Some("/".into()));
        assert_eq!(rparent("/"), None);
    }

    #[test]
    fn local_hash() {
        let f = std::env::temp_dir().join(format!("sesam-hash-{}", std::process::id()));
        std::fs::write(&f, "abc").unwrap();
        assert_eq!(local_sha256(f.to_str().unwrap()).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        std::fs::remove_file(&f).unwrap();
        assert!(local_sha256(f.to_str().unwrap()).is_err());
    }

    #[test]
    fn numbered_names() {
        assert_eq!(numbered("bericht.pdf", 2, false), "bericht (2).pdf");
        assert_eq!(numbered("archiv.tar.gz", 3, false), "archiv.tar (3).gz");
        assert_eq!(numbered(".bashrc", 2, false), ".bashrc (2)");
        assert_eq!(numbered("v1.2", 2, true), "v1.2 (2)");
    }

    #[test]
    fn local_mkdir_and_rename() {
        let dir = std::env::temp_dir().join(format!("sesam-local-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();
        let a = local_mkdir(d, "a").unwrap();
        local_mkdir(d, "b").unwrap();
        assert!(local_rename(&a, "b").is_err(), "nie überschreiben");
        assert!(local_mkdir(d, "..").is_err() && local_mkdir(d, "x\\y").is_err());
        local_rename(&a, "c").unwrap();
        assert!(dir.join("c").is_dir() && !dir.join("a").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_listing_and_unique() {
        let dir = std::env::temp_dir().join(format!("sesam-sftp-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Ordner")).unwrap();
        std::fs::write(dir.join("b.txt"), "hallo").unwrap();
        std::fs::write(dir.join("A.txt"), "").unwrap();

        let l = local_list(dir.to_str().unwrap()).unwrap();
        let names: Vec<_> = l.entries.iter().map(|e| (e.name.as_str(), e.dir, e.size)).collect();
        assert_eq!(names, [("Ordner", true, 0), ("A.txt", false, 0), ("b.txt", false, 5)]);
        assert!(l.parent.is_some());

        assert_eq!(local_unique(&dir, "b.txt", false), dir.join("b (2).txt"));
        assert_eq!(local_unique(&dir, "neu.txt", false), dir.join("neu.txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Gegen einen echten SSH-Server (nur auf Wunsch): SESAM_TEST_SSH="host:port:user:schlüsseldatei:ordner".
    /// Hochladen wie die Durchreiche, Papierkorb samt Rückgängig, Sichern/Zurückholen-Skripte über exec.
    #[tokio::test]
    #[ignore]
    async fn echter_server_papierkorb_und_durchreiche() {
        let v = std::env::var("SESAM_TEST_SSH").expect("SESAM_TEST_SSH");
        let p: Vec<&str> = v.splitn(5, ':').collect();
        let mut t = crate::ssh::Target {
            host: p[0].into(),
            port: p[1].parse().unwrap(),
            user: p[2].into(),
            password: None,
            key: Some(std::fs::read_to_string(p[3]).unwrap()),
            trusted: None,
            opts: Default::default(),
            forwards: Vec::new(),
            jump: None,
        };
        let h = match crate::ssh::connect(&t).await {
            Ok(h) => h,
            Err(e) => {
                t.trusted = Some(e.rsplit('|').next().unwrap().to_string());
                crate::ssh::connect(&t).await.unwrap()
            }
        };
        let s = std::sync::Arc::new(super::start(h).await.map_err(|e| e.1).unwrap());
        let dir = p[4].to_string();
        // Durchreiche: Datei hochladen, Name eindeutig.
        let local = std::env::temp_dir().join("sesam-durchreiche.txt");
        std::fs::write(&local, "durchgereicht").unwrap();
        let up = super::upload(&s, &local, &dir, false, |_| {}).await.unwrap();
        assert!(up.ends_with("sesam-durchreiche.txt"), "{up}");
        // Papierkorb und Rückgängig.
        let trashed = s.to_trash(&up).await.unwrap();
        assert!(trashed.contains("/.sesam-papierkorb/") && !s.s.try_exists(&up).await.unwrap());
        s.restore(&trashed, &up).await.unwrap();
        assert!(s.s.try_exists(&up).await.unwrap());
        assert!(s.size(&up).await.unwrap() == 13 && s.size(&dir).await.unwrap() > 0);
        // Sichern vor dem Ersetzen und Zurückholen, hier ohne sudo (SUDO_USER von Hand).
        let slot = s.trash_slot().await.unwrap();
        let sh = |c: String| format!("SUDO_USER=$(id -un); {c}");
        s.exec(&sh(super::sudo_op_script("backup", &up, &slot).unwrap()), false, None).await.unwrap();
        s.exec(&format!("echo neu > {}", crate::scp::quote(&up).unwrap()), false, None).await.unwrap();
        s.exec(&super::sudo_op_script("restore", &format!("{slot}/sesam-durchreiche.txt"), &up).unwrap(), false, None).await.unwrap();
        let back = s.exec(&format!("cat {}", crate::scp::quote(&up).unwrap()), false, None).await.unwrap();
        assert_eq!(back.trim(), "durchgereicht");
        assert!(!s.s.try_exists(&slot).await.unwrap());
        s.remove(&up).await.unwrap();
        std::fs::remove_file(local).unwrap();
    }

    /// Sichern und Zurückholen laufen echt in der Shell (ohne sudo; chown auf den eigenen Benutzer).
    #[cfg(unix)]
    #[test]
    fn sichern_und_zurueckholen_in_der_shell() {
        let base = std::env::temp_dir().join(format!("sesam-bk-{}", std::process::id()));
        let slot = base.join(".sesam-papierkorb/20261110-1");
        std::fs::create_dir_all(&slot).unwrap();
        let f = base.join("conf");
        std::fs::write(&f, "alt").unwrap();
        std::fs::set_permissions(&f, std::os::unix::fs::PermissionsExt::from_mode(0o640)).unwrap();
        let (f, slot) = (f.to_str().unwrap().to_string(), slot.to_str().unwrap().to_string());
        let sh = |c: String| assert!(std::process::Command::new("sh").arg("-c").arg(&c).env("SUDO_USER", std::env::var("USER").unwrap_or("root".into())).status().unwrap().success(), "{c}");
        sh(super::sudo_op_script("backup", &f, &slot).unwrap());
        assert_eq!(std::fs::read_to_string(format!("{slot}/conf")).unwrap(), "alt");
        assert!(std::fs::read_to_string(format!("{slot}/.sesam-meta")).unwrap().trim().ends_with(" 640"));
        std::fs::write(&f, "neu").unwrap();
        std::fs::set_permissions(&f, std::os::unix::fs::PermissionsExt::from_mode(0o666)).unwrap();
        sh(super::sudo_op_script("restore", &format!("{slot}/conf"), &f).unwrap());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "alt");
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&f).unwrap().permissions()) & 0o777, 0o640);
        assert!(!std::path::Path::new(&slot).exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn skripte_fuer_rechte_und_verschieben() {
        assert_eq!(super::perms_script("/opt/a b", Some(0o755), Some("root:wheel"), true).unwrap(), "chmod -R 755 -- '/opt/a b' && chown -R root:wheel -- '/opt/a b'");
        assert_eq!(super::perms_script("/x", Some(0o644), None, false).unwrap(), "chmod 644 -- '/x'");
        assert!(super::perms_script("/x", None, Some("root; rm -rf /"), false).is_err());
        assert!(super::perms_script("/x", None, None, false).is_err());
        let m = super::move_script("/tmp/tmp.ab12/install.sh", "/opt", false).unwrap();
        assert!(m.contains("cp -R -- '/tmp/tmp.ab12/install.sh' '/opt'/") && m.contains("rmdir -- '/tmp/tmp.ab12'") && m.contains("[ -e '/opt/install.sh' ]") && !m.contains("cat"));
        let r = super::move_script("/tmp/tmp.ab12/sshd_config", "/etc/ssh", true).unwrap();
        assert!(r.starts_with("if [ -f '/etc/ssh/sshd_config' ] && [ -f '/tmp/tmp.ab12/sshd_config' ]; then cat -- '/tmp/tmp.ab12/sshd_config' > '/etc/ssh/sshd_config'"));
        assert!(super::move_script("/home/mike/install.sh", "/opt", false).is_err(), "nur aus mktemp-Ordnern");
        assert!(super::sudo_op_script("remove", "/", "").is_err() && super::sudo_op_script("remove", "rel", "").is_err());
        assert_eq!(super::sudo_op_script("mkdir", "/opt", "app").unwrap(), "mkdir -- '/opt/app'");
        assert!(super::sudo_op_script("rename", "/opt/a", "../b").is_err());
        assert!(super::sudo_op_script("fetch", "/etc/x", "/home/m").is_err());
        let b = super::sudo_op_script("backup", "/etc/ssh/sshd_config", "/home/m/.sesam-papierkorb/20261110-1").unwrap();
        assert!(b.starts_with("cp -a -- '/etc/ssh/sshd_config' '/home/m/.sesam-papierkorb/20261110-1'/") && b.contains(".sesam-meta") && b.contains("chown -R"));
        let r = super::sudo_op_script("restore", "/home/m/.sesam-papierkorb/20261110-1/sshd_config", "/etc/ssh/sshd_config").unwrap();
        assert!(r.contains("cp -a --") && r.contains("chown \"$1\"") && r.contains("chmod \"$2\"") && r.ends_with("rm -rf -- '/home/m/.sesam-papierkorb/20261110-1'"));
        assert!(super::sudo_op_script("restore", "/etc/passwd", "/tmp/x").is_err() && super::sudo_op_script("backup", "/etc/x", "/tmp").is_err());
        assert_eq!(super::sudo_error("sudo: a password is required", 1), super::SUDO_PASSWORD);
        assert!(super::sudo_error("Sorry, try again.\nsudo: 1 incorrect password attempt", 1).contains("stimmt nicht"));
    }
}
