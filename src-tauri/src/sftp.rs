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
            plan.push(Item { src: src.to_string_lossy().into(), dst: dst.clone(), dir: true });
            for e in std::fs::read_dir(&src).map_err(err)?.flatten() {
                stack.push((e.path(), rjoin(&dst, &e.file_name().to_string_lossy())));
            }
        } else {
            total += m.len();
            plan.push(Item { src: src.to_string_lossy().into(), dst, dir: false });
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
            plan.push(Item { src: src.clone(), dst: dst.to_string_lossy().into(), dir: true });
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
            plan.push(Item { src, dst: dst.to_string_lossy().into(), dir: false });
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
        }
    }
    let root = root.to_string_lossy().into_owned();
    (m.report)(Progress::Done { target: root.clone() });
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
