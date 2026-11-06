//! Dateien über SFTP: Ordner auflisten (lokal und entfernt) und übertragen mit Fortschritt.
//! Übertragungen überschreiben nie: belegte Namen werden zu «Name (2).ext».

use crate::ssh::{self, Client, Target};
use russh::client::Handle;
use russh_sftp::client::SftpSession;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct Sftp {
    pub s: SftpSession,
    /// Hält die SSH-Verbindung offen.
    _ssh: Handle<Client>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub async fn open(t: Target<'_>) -> Result<Sftp, String> {
    let h = ssh::connect(&t).await?;
    let ch = h.channel_open_session().await.map_err(err)?;
    ch.request_subsystem(true, "sftp").await.map_err(err)?;
    let s = SftpSession::new(ch.into_stream()).await.map_err(|e| format!("SFTP nicht verfügbar: {e}"))?;
    Ok(Sftp { s, _ssh: h })
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    /// Sekunden seit 1970.
    pub modified: Option<u64>,
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
fn numbered(name: &str, n: u32, dir: bool) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !dir && !stem.is_empty() => format!("{stem} ({n}).{ext}"),
        _ => format!("{name} ({n})"),
    }
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
            entries.push(Entry { name, dir, size: if dir { 0 } else { m.len() }, modified });
        }
        sort(&mut entries);
        Ok(Listing { parent: rparent(&path), path, entries })
    }

    pub async fn mkdir(&self, dir: &str, name: &str) -> Result<String, String> {
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            return Err("Ungültiger Ordnername".into());
        }
        let p = rjoin(dir, name);
        self.s.create_dir(&p).await.map_err(err)?;
        Ok(p)
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

pub fn local_list(path: &str) -> Result<Listing, String> {
    #[cfg(windows)]
    if path.is_empty() {
        let entries = (b'A'..=b'Z')
            .map(|d| format!("{}:\\", d as char))
            .filter(|d| Path::new(d).exists())
            .map(|name| Entry { name, dir: true, size: 0, modified: None })
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
        entries.push(Entry { name: e.file_name().to_string_lossy().into_owned(), dir: m.is_dir(), size, modified });
    }
    sort(&mut entries);
    let parent = match p.parent() {
        Some(par) => Some(par.to_string_lossy().into_owned()),
        None if cfg!(windows) => Some(String::new()),
        None => None,
    };
    Ok(Listing { path: p.to_string_lossy().into_owned(), parent, entries })
}

fn local_unique(dir: &Path, name: &str, is_dir: bool) -> PathBuf {
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

/// Ein Schritt der Übertragung: Ordner anlegen oder Datei kopieren.
struct Item {
    src: String,
    dst: String,
    dir: bool,
}

struct Meter<F: Fn(Progress)> {
    done: u64,
    total: u64,
    last: Instant,
    report: F,
}

impl<F: Fn(Progress)> Meter<F> {
    fn add(&mut self, n: u64, file: &str) {
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
pub async fn upload(sftp: &Sftp, local: &Path, remote_dir: &str, report: impl Fn(Progress)) -> Result<String, String> {
    let name = local.file_name().ok_or("Ungültige Quelle")?.to_string_lossy().into_owned();
    let root = sftp.unique(remote_dir, &name, local.is_dir()).await?;

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
            let r = tokio::fs::File::open(&it.src).await.map_err(|e| format!("{}: {e}", it.src))?;
            let w = sftp.s.create(&it.dst).await.map_err(|e| format!("{}: {e}", it.dst))?;
            pump(r, w, &mut m, &it.src).await?;
        }
    }
    (m.report)(Progress::Done { target: root.clone() });
    Ok(root)
}

/// Entfernte Datei oder entfernten Ordner nach `local_dir` herunterladen. Gibt den Zielpfad zurück.
pub async fn download(sftp: &Sftp, remote: &str, local_dir: &Path, report: impl Fn(Progress)) -> Result<String, String> {
    let meta = sftp.s.metadata(remote).await.map_err(|e| format!("{remote}: {e}"))?;
    let name = remote.trim_end_matches('/').rsplit('/').next().filter(|n| !n.is_empty()).ok_or("Ungültige Quelle")?;
    let root = local_unique(local_dir, name, meta.file_type().is_dir());

    let (mut plan, mut total) = (Vec::new(), 0u64);
    let mut stack = vec![(remote.to_string(), root.clone(), meta)];
    while let Some((src, dst, m)) = stack.pop() {
        if m.file_type().is_dir() {
            plan.push(Item { src: src.clone(), dst: dst.to_string_lossy().into(), dir: true });
            for e in sftp.s.read_dir(&src).await.map_err(err)? {
                let n = e.file_name();
                // Nur echte Dateien und Ordner; Verknüpfungen nicht verfolgen (keine Schleifen).
                if n != "." && n != ".." && !e.file_type().is_symlink() {
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
            let r = sftp.s.open(&it.src).await.map_err(|e| format!("{}: {e}", it.src))?;
            let w = tokio::fs::File::create(&it.dst).await.map_err(|e| format!("{}: {e}", it.dst))?;
            pump(r, w, &mut m, &it.src).await?;
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
    fn remote_paths() {
        assert_eq!(rjoin("/", "etc"), "/etc");
        assert_eq!(rjoin("/home/mike", "a b"), "/home/mike/a b");
        assert_eq!(rparent("/home/mike"), Some("/home".into()));
        assert_eq!(rparent("/home/"), Some("/".into()));
        assert_eq!(rparent("/"), None);
    }

    #[test]
    fn numbered_names() {
        assert_eq!(numbered("bericht.pdf", 2, false), "bericht (2).pdf");
        assert_eq!(numbered("archiv.tar.gz", 3, false), "archiv.tar (3).gz");
        assert_eq!(numbered(".bashrc", 2, false), ".bashrc (2)");
        assert_eq!(numbered("v1.2", 2, true), "v1.2 (2)");
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
