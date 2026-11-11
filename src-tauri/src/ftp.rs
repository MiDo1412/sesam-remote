//! Dateien über FTP und FTPS (suppaftp mit tokio und rustls/ring), für Webserver ohne SSH.
//! Zertifikate wie bei RDP/VNC per Pinning (`vnc::tls::Pinned`). Gleiche Zusagen wie SFTP:
//! nie überschreiben («Name (2).ext»), Teildateien `*.sesam-teil` verschwinden bei Fehler oder Abbruch.
//! FTP kann pro Verbindung nur eine Übertragung: jede Übertragung öffnet darum eine eigene Verbindung,
//! die Ansicht bleibt über die Steuerverbindung bedienbar.

use crate::sftp::{Cleanup, Entry, Listing, Meter, PARTIAL, Partial, Progress, local_unique, numbered, rjoin, rparent, valid_name};
use crate::vnc::tls::Pinned;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};
use suppaftp::FtpError;
use suppaftp::list::{File, ListParser, PosixPexQuery};
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsStream, ImplAsyncFtpStream};
use suppaftp::types::FileType;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Auch unverschlüsseltes FTP nutzt diesen Typ; TLS kommt dann einfach nie zum Zug.
type Stream = ImplAsyncFtpStream<AsyncRustlsStream>;

const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Security {
    /// AUTH TLS auf Port 21 (Standard).
    Explicit,
    /// TLS ab dem ersten Byte, meist Port 990.
    Implicit,
    /// Passwort und Daten im Klartext.
    Plain,
}

impl Security {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "explicit" => Ok(Self::Explicit),
            "implicit" => Ok(Self::Implicit),
            "none" => Ok(Self::Plain),
            _ => Err("Unbekannte FTP-Verschlüsselung".into()),
        }
    }
}

#[derive(Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub security: Security,
    pub active: bool,
    /// Erwarteter Fingerabdruck (gemerkt oder gerade bestätigt).
    pub cert: Option<String>,
    pub skip_cert_check: bool,
}

fn err(e: FtpError) -> String {
    match e {
        FtpError::UnexpectedResponse(r) => format!("Server meldet: {}", r.as_string().unwrap_or_default().trim()),
        FtpError::ConnectionError(e) => format!("Verbindung unterbrochen: {e}"),
        e => e.to_string(),
    }
}

/// Pfade gehen als Befehlszeile an den Server: ein Zeilenumbruch würde weitere Befehle einschleusen.
fn safe(p: &str) -> Result<&str, String> {
    if p.contains(['\r', '\n', '\0']) { Err("Ungültiger Pfad".into()) } else { Ok(p) }
}

/// Gescheiterter TLS-Handshake: unbekanntes oder geändertes Zertifikat als `CERT_UNKNOWN:`/`CERT_CHANGED:` fürs Frontend.
fn tls_err(pin: &Pinned, skip: bool, e: FtpError) -> String {
    match e {
        FtpError::SecureError(m) if !skip => pin.explain(std::io::Error::other(m)),
        e => err(e),
    }
}

/// Verbinden, TLS nach Wahl, anmelden, Binärmodus. Gibt auch den Fingerabdruck zurück, den TLS gesehen hat.
async fn connect(t: &Target) -> Result<(Stream, Option<String>), String> {
    let pin = if t.skip_cert_check { Pinned::accept_any() } else { Pinned::new(t.cert.clone()) };
    let tls = || -> Result<AsyncRustlsConnector, String> { Ok(tokio_rustls::TlsConnector::from(Arc::new(pin.config()?)).into()) };
    let addr = (t.host.as_str(), t.port);
    let open = async {
        let mut s = match t.security {
            Security::Implicit => Stream::connect_secure_implicit(addr, tls()?, &t.host).await.map_err(|e| tls_err(&pin, t.skip_cert_check, e))?,
            sec => {
                let s = Stream::connect(addr).await.map_err(|e| format!("Verbindung fehlgeschlagen: {}", err(e)))?;
                if sec == Security::Plain {
                    s
                } else {
                    s.into_secure(tls()?, &t.host).await.map_err(|e| match e {
                        FtpError::UnexpectedResponse(_) => format!("Der Server bietet kein FTPS an ({}). Unter «Verschlüsselung» prüfen.", err(e)),
                        e => tls_err(&pin, t.skip_cert_check, e),
                    })?
                }
            }
        };
        let user = if t.user.is_empty() { "anonymous" } else { t.user.as_str() };
        s.login(user, t.password.as_str()).await.map_err(|e| match e {
            FtpError::UnexpectedResponse(_) => format!("Anmeldung abgelehnt ({})", err(e)),
            e => err(e),
        })?;
        s.transfer_type(FileType::Binary).await.map_err(err)?;
        if t.active {
            s = s.active_mode(Duration::from_secs(30));
        } else {
            // Datenverbindung zur Adresse der Steuerverbindung, nicht zu der aus PASV (NAT, fremde Ziele).
            s.set_passive_nat_workaround(true);
        }
        Ok::<_, String>(s)
    };
    let s = tokio::time::timeout(TIMEOUT, open).await.map_err(|_| "Zeitüberschreitung beim Verbinden".to_string())??;
    let seen = pin.seen.lock().unwrap().clone();
    Ok((s, seen))
}

struct Ctl {
    s: Stream,
    used: Instant,
}

pub struct Ftp {
    t: Target,
    ctl: tokio::sync::Mutex<Ctl>,
    /// Was der Server zum Auflisten versteht: 0 = MLSD, 1 = `LIST -a`, 2 = `LIST`.
    list_mode: AtomicU8,
}

/// Eine Zeile von MLSD oder LIST als Eintrag; `true` = Verknüpfung. `None` für «total 12», «.», «..» und Unlesbares.
fn parse_line(line: &str, mlsd: bool) -> Option<(Entry, bool)> {
    let facts = if mlsd { line.split_once("; ").map_or("", |(f, _)| f).to_ascii_lowercase() } else { String::new() };
    if facts.split(';').any(|x| x == "type=cdir" || x == "type=pdir") {
        return None;
    }
    let (f, has_perm) = if mlsd {
        (ListParser::parse_mlsd(line).ok()?, facts.contains("unix.mode="))
    } else if let Ok(f) = ListParser::parse_posix(line) {
        (f, true)
    } else {
        (ListParser::parse_dos(line).ok()?, false)
    };
    let name = f.name().to_string();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\r', '\n', '\0']) {
        return None;
    }
    let dir = f.is_directory();
    let modified = f.modified().duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()).filter(|&s| s > 0);
    let perm = has_perm.then(|| perm(&f));
    Some((Entry { name, dir, size: if dir { 0 } else { f.size() as u64 }, modified, perm }, f.is_symlink()))
}

fn perm(f: &File) -> u32 {
    [(6, PosixPexQuery::Owner), (3, PosixPexQuery::Group), (0, PosixPexQuery::Others)]
        .into_iter()
        .map(|(s, q)| (((f.can_read(q) as u32) << 2) | ((f.can_write(q) as u32) << 1) | f.can_execute(q) as u32) << s)
        .sum()
}

/// Erster freier Name: «bericht.pdf», sonst «bericht (2).pdf» usw.
fn free_name(taken: &HashSet<String>, name: &str, dir: bool) -> String {
    let (mut n, mut i) = (name.to_string(), 2);
    while taken.contains(&n) {
        n = numbered(name, i, dir);
        i += 1;
    }
    n
}

impl Ftp {
    /// Verbinden. Zurück kommt auch der Fingerabdruck, den TLS gesehen hat (zum Merken).
    pub async fn open(t: Target) -> Result<(Self, Option<String>), String> {
        let (s, seen) = connect(&t).await?;
        Ok((Self { t, ctl: tokio::sync::Mutex::new(Ctl { s, used: Instant::now() }), list_mode: AtomicU8::new(0) }, seen))
    }

    /// Steuerverbindung; nach einer Pause erst prüfen, denn Server trennen untätige Sitzungen.
    async fn ctl(&self) -> Result<tokio::sync::MutexGuard<'_, Ctl>, String> {
        let mut g = self.ctl.lock().await;
        if g.used.elapsed() > Duration::from_secs(20) && !matches!(tokio::time::timeout(Duration::from_secs(5), g.s.noop()).await, Ok(Ok(()))) {
            g.s = connect(&self.t).await?.0;
        }
        g.used = Instant::now();
        Ok(g)
    }

    /// Einträge eines Ordners ("" = Startordner) und sein absoluter Pfad; `true` = Verknüpfung.
    async fn entries(&self, s: &mut Stream, path: &str) -> Result<(String, Vec<(Entry, bool)>), String> {
        if !path.is_empty() {
            s.cwd(safe(path)?).await.map_err(|e| format!("{path}: {}", err(e)))?;
        }
        let path = s.pwd().await.map_err(err)?;
        let lines = loop {
            let mode = self.list_mode.load(Ordering::Relaxed);
            let r = match mode {
                0 => s.mlsd(None).await,
                1 => s.list(Some("-a")).await,
                _ => s.list(None).await,
            };
            match r {
                Ok(l) => break l,
                // Nicht verstanden: die nächste Art probieren und sich das merken.
                Err(FtpError::UnexpectedResponse(_)) if mode < 2 => self.list_mode.store(mode + 1, Ordering::Relaxed),
                Err(e) => return Err(format!("{path}: {}", err(e))),
            }
        };
        let mlsd = self.list_mode.load(Ordering::Relaxed) == 0;
        Ok((path, lines.iter().filter_map(|l| parse_line(l, mlsd)).collect()))
    }

    async fn names(&self, s: &mut Stream, dir: &str) -> Result<HashSet<String>, String> {
        Ok(self.entries(s, dir).await?.1.into_iter().map(|(e, _)| e.name).collect())
    }

    async fn unique(&self, dir: &str, name: &str, is_dir: bool) -> Result<String, String> {
        let mut g = self.ctl().await?;
        let taken = self.names(&mut g.s, dir).await?;
        Ok(rjoin(dir, &free_name(&taken, name, is_dir)))
    }

    pub async fn list(&self, path: &str) -> Result<Listing, String> {
        let mut g = self.ctl().await?;
        let (path, raw) = self.entries(&mut g.s, path).await?;
        let mut entries = Vec::with_capacity(raw.len());
        for (mut e, link) in raw {
            // Verknüpfung: Ordner, wenn man hineinwechseln kann.
            if link && g.s.cwd(rjoin(&path, &e.name)).await.is_ok() {
                (e.dir, e.size) = (true, 0);
            }
            entries.push(e);
        }
        Ok(Listing { parent: rparent(&path), path, entries })
    }

    pub async fn mkdir(&self, dir: &str, name: &str) -> Result<String, String> {
        valid_name(name)?;
        let p = rjoin(safe(dir)?, safe(name)?);
        self.ctl().await?.s.mkdir(&p).await.map_err(|e| format!("{p}: {}", err(e)))?;
        Ok(p)
    }

    /// Im selben Ordner umbenennen; ein belegter Name wird nicht überschrieben.
    pub async fn rename(&self, path: &str, name: &str) -> Result<String, String> {
        valid_name(name)?;
        let dir = rparent(safe(path)?).ok_or("Das Wurzelverzeichnis lässt sich nicht umbenennen")?;
        let to = rjoin(&dir, safe(name)?);
        let mut g = self.ctl().await?;
        if self.names(&mut g.s, &dir).await?.contains(name) {
            return Err(format!("«{name}» gibt es dort schon"));
        }
        g.s.rename(path, to.as_str()).await.map_err(|e| format!("{path}: {}", err(e)))?;
        Ok(to)
    }

    /// Datei oder Ordner samt Inhalt löschen. Verknüpfungen werden gelöscht, nicht verfolgt.
    pub async fn remove(&self, path: &str) -> Result<(), String> {
        if rparent(safe(path)?).is_none() {
            return Err("Das Wurzelverzeichnis lässt sich nicht löschen".into());
        }
        let mut g = self.ctl().await?;
        let s = &mut g.s;
        if s.cwd(path).await.is_err() {
            return s.rm(path).await.map_err(|e| format!("{path}: {}", err(e)));
        }
        let (mut dirs, mut stack) = (Vec::new(), vec![path.to_string()]);
        while let Some(d) = stack.pop() {
            for (e, link) in self.entries(s, &d).await?.1 {
                let p = rjoin(&d, &e.name);
                if e.dir && !link {
                    stack.push(p);
                } else {
                    s.rm(&p).await.map_err(|e| format!("{p}: {}", err(e)))?;
                }
            }
            dirs.push(d);
        }
        // Erst hinaus, sonst lehnen manche Server RMD des aktuellen Ordners ab.
        let _ = s.cwd("/").await;
        for d in dirs.iter().rev() {
            s.rmdir(d).await.map_err(|e| format!("{d}: {}", err(e)))?;
        }
        Ok(())
    }

    /// Rechte über `SITE CHMOD` (können die meisten Unix-Server, nicht alle).
    pub async fn chmod(&self, path: &str, mode: u32) -> Result<(), String> {
        if mode > 0o7777 {
            return Err("Ungültige Rechte".into());
        }
        let cmd = format!("CHMOD {mode:o} {}", safe(path)?);
        self.ctl().await?.s.site(cmd).await.map(|_| ()).map_err(|e| format!("{path}: {}", err(e)))
    }

    /// Leere Datei anlegen; ein belegter Name wird nicht überschrieben.
    pub async fn touch(&self, dir: &str, name: &str) -> Result<String, String> {
        valid_name(name)?;
        let p = rjoin(safe(dir)?, safe(name)?);
        let mut g = self.ctl().await?;
        if self.names(&mut g.s, dir).await?.contains(name) {
            return Err(format!("«{name}» gibt es dort schon"));
        }
        let w = g.s.put_with_stream(&p).await.map_err(|e| format!("{p}: {}", err(e)))?;
        w.finish().await.map_err(|e| format!("{p}: {}", err(e)))?;
        Ok(p)
    }
}

// ---------- Übertragen ----------

/// Halb hochgeladene Datei auf dem Server löschen, wenn die Übertragung scheitert oder abgebrochen wird.
struct RemotePartial(Option<(Arc<Ftp>, String)>);

impl Drop for RemotePartial {
    fn drop(&mut self) {
        if let Some((f, p)) = self.0.take()
            && let Ok(rt) = tokio::runtime::Handle::try_current()
        {
            rt.spawn(async move {
                // Der Server gibt die Datei erst frei, wenn er den Abbruch der Datenverbindung bemerkt.
                tokio::time::sleep(Duration::from_millis(300)).await;
                if let Ok(mut g) = f.ctl().await {
                    let _ = g.s.rm(&p).await;
                }
            });
        }
    }
}

async fn copy<R, W, F>(r: &mut R, w: &mut W, m: &mut Meter<F>, file: &str) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: Fn(Progress),
{
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        crate::sftp::wait_if_paused().await;
        let n = r.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return w.flush().await.map_err(|e| e.to_string());
        }
        w.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
        m.add(n as u64, file);
    }
}

/// Lokale Datei oder lokalen Ordner nach `remote_dir` hochladen. Gibt den Zielpfad zurück.
pub async fn upload(ftp: &Arc<Ftp>, local: &Path, remote_dir: &str, report: impl Fn(Progress)) -> Result<String, String> {
    let name = local.file_name().ok_or("Ungültige Quelle")?.to_string_lossy().into_owned();
    let root = ftp.unique(safe(remote_dir)?, safe(&name)?, local.is_dir()).await?;

    // Plan: Ordner vor ihrem Inhalt; lokal reicht ein synchroner Durchlauf.
    let (mut plan, mut total) = (Vec::new(), 0u64);
    let mut stack = vec![(local.to_path_buf(), root.clone())];
    while let Some((src, dst)) = stack.pop() {
        let m = std::fs::metadata(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        safe(&dst)?;
        if m.is_dir() {
            for e in std::fs::read_dir(&src).map_err(|e| format!("{}: {e}", src.display()))?.flatten() {
                stack.push((e.path(), rjoin(&dst, &e.file_name().to_string_lossy())));
            }
            plan.push((src, dst, true));
        } else {
            total += m.len();
            plan.push((src, dst, false));
        }
    }

    let mut s = connect(&ftp.t).await?.0;
    let mut m = Meter { done: 0, total, last: Instant::now(), report };
    for (src, dst, dir) in plan {
        if dir {
            s.mkdir(&dst).await.map_err(|e| format!("{dst}: {}", err(e)))?;
            continue;
        }
        let tmp = format!("{dst}{PARTIAL}");
        let mut r = tokio::fs::File::open(&src).await.map_err(|e| format!("{}: {e}", src.display()))?;
        let mut guard = RemotePartial(Some((ftp.clone(), tmp.clone())));
        let mut w = s.put_with_stream(&tmp).await.map_err(|e| format!("{dst}: {}", err(e)))?;
        copy(&mut r, &mut w, &mut m, &src.to_string_lossy()).await?;
        w.finish().await.map_err(|e| format!("{dst}: {}", err(e)))?;
        s.rename(tmp.as_str(), dst.as_str()).await.map_err(|e| format!("{dst}: {}", err(e)))?;
        guard.0 = None;
    }
    (m.report)(Progress::Done { target: root.clone(), trashed: None });
    Ok(root)
}

/// Entfernte Datei oder entfernten Ordner nach `local_dir` herunterladen. Gibt den Zielpfad zurück.
/// `replace` (einzelne Datei, nach Rückfrage): gleichnamige lokale Datei ersetzen; erst nach vollständigem Laden.
pub async fn download(ftp: &Arc<Ftp>, remote: &str, local_dir: &Path, replace: bool, report: impl Fn(Progress)) -> Result<String, String> {
    let name = safe(remote)?.trim_end_matches('/').rsplit('/').next().filter(|n| !n.is_empty()).ok_or("Ungültige Quelle")?;
    crate::scp::check_name(name)?;
    let (mut plan, mut total) = (Vec::new(), 0u64);
    let root = {
        let mut g = ftp.ctl().await?;
        let s = &mut g.s;
        let is_dir = s.cwd(remote).await.is_ok();
        let root = if replace && !is_dir { local_dir.join(name) } else { local_unique(local_dir, name, is_dir) };
        if is_dir {
            let mut stack = vec![(remote.to_string(), root.clone())];
            while let Some((src, dst)) = stack.pop() {
                for (e, link) in ftp.entries(s, &src).await?.1 {
                    // Verknüpfungen nicht verfolgen (keine Schleifen); Namen, die lokal aus dem Ziel führen würden, auslassen.
                    if link || crate::scp::check_name(&e.name).is_err() {
                        continue;
                    }
                    let (p, d) = (rjoin(&src, &e.name), dst.join(&e.name));
                    if e.dir {
                        stack.push((p, d));
                    } else {
                        total += e.size;
                        plan.push((p, d, false));
                    }
                }
                plan.push((src, dst, true));
            }
            // Ordner vor ihrem Inhalt anlegen.
            plan.sort_by_key(|(_, _, dir)| !dir);
        } else {
            total = s.size(remote).await.unwrap_or(0) as u64;
            plan.push((remote.to_string(), root.clone(), false));
        }
        root
    };

    let mut s = connect(&ftp.t).await?.0;
    let mut m = Meter { done: 0, total, last: Instant::now(), report };
    for (src, dst, dir) in plan {
        if dir {
            tokio::fs::create_dir_all(&dst).await.map_err(|e| format!("{}: {e}", dst.display()))?;
            continue;
        }
        let mut tmp = dst.clone().into_os_string();
        tmp.push(PARTIAL);
        let tmp = PathBuf::from(tmp);
        let mut r = s.retr_as_stream(&src).await.map_err(|e| format!("{src}: {}", err(e)))?;
        // Vor der Datei anlegen: beim Abbruch zuerst schliessen, dann löschen (Windows).
        let mut guard = Partial(Some(Cleanup::Local(tmp.clone())));
        let mut w = tokio::fs::File::create(&tmp).await.map_err(|e| format!("{}: {e}", dst.display()))?;
        copy(&mut r, &mut w, &mut m, &src).await?;
        r.finish().await.map_err(|e| format!("{src}: {}", err(e)))?;
        drop(w);
        tokio::fs::rename(&tmp, &dst).await.map_err(|e| format!("{}: {e}", dst.display()))?;
        guard.0 = None;
    }
    let root = root.to_string_lossy().into_owned();
    (m.report)(Progress::Done { target: root.clone(), trashed: None });
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vnc::tls::{CERT_CHANGED, CERT_UNKNOWN};

    #[test]
    fn parses_posix_dos_and_mlsd() {
        let (e, link) = parse_line("-rw-r--r--    1 web      web          1234 Nov  5  2023 index.html", false).unwrap();
        assert_eq!((e.name.as_str(), e.dir, e.size, e.perm, link), ("index.html", false, 1234, Some(0o644), false));
        assert!(e.modified.is_some());
        let (e, _) = parse_line("drwxr-xr-x    2 web      web          4096 Jan 10 09:15 bilder mit leer", false).unwrap();
        assert_eq!((e.name.as_str(), e.dir, e.size, e.perm), ("bilder mit leer", true, 0, Some(0o755)));
        let (e, link) = parse_line("lrwxrwxrwx    1 web      web            11 Jan 10 09:15 www -> public_html", false).unwrap();
        assert_eq!((e.name.as_str(), link), ("www", true));
        assert!(parse_line("total 12", false).is_none());
        assert!(parse_line("drwxr-xr-x    2 web      web          4096 Jan 10 09:15 ..", false).is_none());

        let (e, _) = parse_line("10-19-20  03:19PM       <DIR>          pub", false).unwrap();
        assert_eq!((e.name.as_str(), e.dir, e.perm), ("pub", true, None));
        let (e, _) = parse_line("04-08-14  03:09PM                  403 readme.txt", false).unwrap();
        assert_eq!((e.name.as_str(), e.size), ("readme.txt", 403));

        let (e, _) = parse_line("type=file;size=42;modify=20240102030405;UNIX.mode=0640; a; b.txt", true).unwrap();
        assert_eq!((e.name.as_str(), e.size, e.perm, e.modified), ("a; b.txt", 42, Some(0o640), Some(1704164645)));
        let (e, _) = parse_line("type=dir;modify=20240102030405; css", true).unwrap();
        assert_eq!((e.dir, e.perm), (true, None));
        assert!(parse_line("type=cdir;modify=20240102030405; /www", true).is_none());
        assert!(parse_line("type=pdir;modify=20240102030405; ..", true).is_none());
    }

    #[test]
    fn free_names_never_overwrite() {
        let taken: HashSet<String> = ["bericht.pdf", "bericht (2).pdf", "css"].map(String::from).into();
        assert_eq!(free_name(&taken, "bericht.pdf", false), "bericht (3).pdf");
        assert_eq!(free_name(&taken, "css", true), "css (2)");
        assert_eq!(free_name(&taken, "neu.txt", false), "neu.txt");
    }

    #[test]
    fn rejects_command_injection_and_unknown_security() {
        assert!(safe("/www/a.txt").is_ok());
        assert!(safe("/www/a\r\nDELE x").is_err() && safe("a\nb").is_err());
        assert_eq!(Security::parse("implicit").unwrap(), Security::Implicit);
        assert!(Security::parse("tls").is_err());
    }

    #[test]
    fn unknown_or_changed_certificate_goes_to_the_frontend() {
        let fp = "SHA256:AA:BB".to_string();
        let unknown = Pinned::new(None);
        *unknown.seen.lock().unwrap() = Some(fp.clone());
        assert_eq!(tls_err(&unknown, false, FtpError::SecureError("x".into())), format!("{CERT_UNKNOWN}{fp}"));
        let changed = Pinned::new(Some("SHA256:CC".into()));
        *changed.seen.lock().unwrap() = Some(fp.clone());
        assert!(tls_err(&changed, false, FtpError::SecureError("x".into())).starts_with(CERT_CHANGED));
        // «Zertifikat nicht prüfen»: kein Nachfragen
        assert!(!tls_err(&unknown, true, FtpError::SecureError("x".into())).starts_with(CERT_UNKNOWN));
    }
}

// ---------- Ersetzen ohne Risiko ----------
// Nie zuerst löschen: erst die neue Fassung unter Zwischennamen hochladen und die Grösse prüfen, dann die alte in den
// Papierkorb (~/.sesam-papierkorb/<Zeit>/ wie bei SFTP) und die neue an ihren Platz. Kann der Server nicht
// umbenennen oder keine Ordner anlegen, wird die alte direkt überschrieben (STOR), aber nie vorher entfernt.

/// Die Schritte auf dem Server, getrennt von FTP, damit die Reihenfolge testbar ist.
pub(crate) trait ReplaceOps {
    async fn size(&mut self, p: &str) -> Result<u64, String>;
    async fn mkdir(&mut self, p: &str) -> Result<(), String>;
    async fn rename(&mut self, from: &str, to: &str) -> Result<(), String>;
    async fn rm(&mut self, p: &str) -> Result<(), String>;
    /// Lokale Datei vollständig nach `p` schreiben (überschreibt).
    async fn stor(&mut self, p: &str) -> Result<(), String>;
}

/// `tmp` ist schon hochgeladen. Gibt den Pfad der alten Fassung im Papierkorb zurück (None = ohne Papierkorb ersetzt).
pub(crate) async fn finish_replace<O: ReplaceOps>(o: &mut O, tmp: &str, dst: &str, want: u64, slot: &str) -> Result<Option<String>, String> {
    let name = dst.rsplit('/').next().unwrap_or(dst);
    if o.size(tmp).await.ok() != Some(want) {
        let _ = o.rm(tmp).await;
        return Err(format!("{dst}: Die hochgeladene Fassung ist unvollständig; die bisherige Datei bleibt unverändert."));
    }
    if let Some(root) = rparent(slot) {
        let _ = o.mkdir(&root).await; // gibt es meist schon
    }
    let old = rjoin(slot, name);
    if o.mkdir(slot).await.is_ok() && o.rename(dst, &old).await.is_ok() {
        if let Err(e) = o.rename(tmp, dst).await {
            // Neue Fassung nicht an ihren Platz: alte zurück, Zwischendatei weg.
            let _ = o.rename(&old, dst).await;
            let _ = o.rm(tmp).await;
            return Err(format!("{dst}: {e}; die bisherige Datei ist wieder an ihrem Platz."));
        }
        return Ok(Some(old));
    }
    // Ohne Papierkorb: umbenennen über die alte (manche Server ersetzen dabei), sonst direkt überschreiben.
    if o.rename(tmp, dst).await.is_ok() {
        return Ok(None);
    }
    let r = o.stor(dst).await;
    let _ = o.rm(tmp).await;
    r.map(|_| None)
}

struct FtpOps<'a> {
    s: &'a mut Stream,
    local: &'a Path,
}

impl ReplaceOps for FtpOps<'_> {
    async fn size(&mut self, p: &str) -> Result<u64, String> {
        self.s.size(p).await.map(|n| n as u64).map_err(err)
    }
    async fn mkdir(&mut self, p: &str) -> Result<(), String> {
        self.s.mkdir(p).await.map_err(err)
    }
    async fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.s.rename(from, to).await.map_err(err)
    }
    async fn rm(&mut self, p: &str) -> Result<(), String> {
        self.s.rm(p).await.map_err(err)
    }
    async fn stor(&mut self, p: &str) -> Result<(), String> {
        let mut r = tokio::fs::File::open(self.local).await.map_err(|e| e.to_string())?;
        let mut w = self.s.put_with_stream(p).await.map_err(err)?;
        tokio::io::copy(&mut r, &mut w).await.map_err(|e| e.to_string())?;
        w.finish().await.map_err(err)
    }
}

/// Eine Datei ersetzen (nach Rückfrage): über Zwischennamen, alte Fassung in den Papierkorb.
pub async fn upload_replace(ftp: &Arc<Ftp>, local: &Path, remote_dir: &str, report: impl Fn(Progress)) -> Result<String, String> {
    if !local.is_file() {
        return Err("Ersetzen geht bei FTP nur für einzelne Dateien".into());
    }
    let name = local.file_name().ok_or("Ungültige Quelle")?.to_string_lossy().into_owned();
    let dst = rjoin(safe(remote_dir)?, safe(&name)?);
    let tmp = format!("{dst}{PARTIAL}");
    let want = std::fs::metadata(local).map_err(|e| format!("{}: {e}", local.display()))?.len();
    let mut s = connect(&ftp.t).await?.0;
    let home = s.pwd().await.map_err(err)?;
    let slot = rjoin(&rjoin(&home, crate::sftp::TRASH), &chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string());
    let mut m = Meter { done: 0, total: want, last: Instant::now(), report };
    {
        let mut r = tokio::fs::File::open(local).await.map_err(|e| format!("{}: {e}", local.display()))?;
        let mut guard = RemotePartial(Some((ftp.clone(), tmp.clone())));
        let mut w = s.put_with_stream(&tmp).await.map_err(|e| format!("{dst}: {}", err(e)))?;
        copy(&mut r, &mut w, &mut m, &local.to_string_lossy()).await?;
        w.finish().await.map_err(|e| format!("{dst}: {}", err(e)))?;
        guard.0 = None;
    }
    let trashed = finish_replace(&mut FtpOps { s: &mut s, local }, &tmp, &dst, want, &slot).await?;
    (m.report)(Progress::Done { target: dst.clone(), trashed });
    Ok(dst)
}

impl Ftp {
    /// Aus dem Papierkorb zurück (wie SFTP): eine neuere Fassung am alten Ort weicht.
    pub async fn restore(&self, trashed: &str, original: &str) -> Result<(), String> {
        if !trashed.contains(&format!("/{}/", crate::sftp::TRASH)) {
            return Err("Nur aus dem Papierkorb von Sesam".into());
        }
        let mut g = self.ctl().await?;
        let _ = g.s.rm(original).await;
        g.s.rename(trashed, original).await.map_err(|e| format!("Zurückholen: {}", err(e)))?;
        if let Some(slot) = rparent(trashed) {
            let _ = g.s.rmdir(&slot).await;
        }
        Ok(())
    }

    /// Papierkorb aufräumen wie bei SFTP: Zeitstempel-Ordner älter als `days` Tage.
    pub async fn trash_cleanup(&self, days: u32) -> Result<usize, String> {
        let home = self.ctl().await?.s.pwd().await.map_err(err)?;
        let root = rjoin(&home, crate::sftp::TRASH);
        let Ok(l) = self.list(&root).await else { return Ok(0) };
        let cut = (chrono::Local::now() - chrono::Duration::days(i64::from(days))).format("%Y%m%d-%H%M%S").to_string();
        let mut left = 0;
        for e in l.entries {
            if e.name.len() >= 15 && e.name[..15] < *cut {
                let _ = self.remove(&rjoin(&root, &e.name)).await;
            } else {
                left += 1;
            }
        }
        Ok(left)
    }
}

#[cfg(test)]
mod replace_tests {
    use super::*;
    use std::collections::{BTreeMap, HashSet as Set};

    /// Server im Speicher; `deny` verbietet einzelne Befehle (z. B. "rename", "mkdir").
    #[derive(Default)]
    struct Fake {
        files: BTreeMap<String, Vec<u8>>,
        dirs: Set<String>,
        deny: Set<&'static str>,
        new: Vec<u8>,
    }
    impl ReplaceOps for Fake {
        async fn size(&mut self, p: &str) -> Result<u64, String> {
            self.files.get(p).map(|f| f.len() as u64).ok_or("550".into())
        }
        async fn mkdir(&mut self, p: &str) -> Result<(), String> {
            if self.deny.contains("mkdir") || !self.dirs.insert(p.into()) {
                return Err("550".into());
            }
            Ok(())
        }
        async fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
            if self.deny.contains("rename") {
                return Err("502".into());
            }
            let f = self.files.remove(from).ok_or("550")?;
            self.files.insert(to.into(), f);
            Ok(())
        }
        async fn rm(&mut self, p: &str) -> Result<(), String> {
            self.files.remove(p).map(|_| ()).ok_or("550".into())
        }
        async fn stor(&mut self, p: &str) -> Result<(), String> {
            self.files.insert(p.into(), self.new.clone());
            Ok(())
        }
    }

    fn fake(deny: &[&'static str]) -> Fake {
        let mut f = Fake { deny: deny.iter().copied().collect(), new: b"neu".to_vec(), ..Default::default() };
        f.dirs.insert("/home/.sesam-papierkorb".into());
        f.files.insert("/www/index.html".into(), b"alt".to_vec());
        f.files.insert(format!("/www/index.html{PARTIAL}"), b"neu".to_vec());
        f
    }
    const SLOT: &str = "/home/.sesam-papierkorb/20261111-1";

    #[tokio::test]
    async fn ersetzen_mit_papierkorb() {
        let mut f = fake(&[]);
        let t = finish_replace(&mut f, &format!("/www/index.html{PARTIAL}"), "/www/index.html", 3, SLOT).await.unwrap();
        assert_eq!(t.as_deref(), Some("/home/.sesam-papierkorb/20261111-1/index.html"));
        assert_eq!(f.files["/www/index.html"], b"neu");
        assert_eq!(f.files[t.as_deref().unwrap()], b"alt");
        assert!(!f.files.keys().any(|k| k.ends_with(PARTIAL)));
    }

    #[tokio::test]
    async fn unvollstaendig_laesst_alte_stehen() {
        let mut f = fake(&[]);
        assert!(finish_replace(&mut f, &format!("/www/index.html{PARTIAL}"), "/www/index.html", 99, SLOT).await.is_err());
        assert_eq!(f.files["/www/index.html"], b"alt");
        assert!(!f.files.keys().any(|k| k.ends_with(PARTIAL)));
    }

    #[tokio::test]
    async fn ohne_ordner_und_umbenennen_wird_ueberschrieben_nie_geloescht() {
        // Kein mkdir: ohne Papierkorb, Umbenennen über die alte.
        let mut f = fake(&["mkdir"]);
        assert_eq!(finish_replace(&mut f, &format!("/www/index.html{PARTIAL}"), "/www/index.html", 3, SLOT).await.unwrap(), None);
        assert_eq!(f.files["/www/index.html"], b"neu");
        // Weder mkdir noch rename: direkt überschreiben (STOR), Zwischendatei weg.
        let mut f = fake(&["mkdir", "rename"]);
        assert_eq!(finish_replace(&mut f, &format!("/www/index.html{PARTIAL}"), "/www/index.html", 3, SLOT).await.unwrap(), None);
        assert_eq!(f.files["/www/index.html"], b"neu");
        assert!(!f.files.keys().any(|k| k.ends_with(PARTIAL)));
    }
}
