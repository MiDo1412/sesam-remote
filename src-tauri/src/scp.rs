//! Dateien über SCP (wie PSCP), für Server ohne SFTP: `scp -t` zum Hochladen, `scp -f` zum Herunterladen,
//! jeweils rekursiv über einen exec-Kanal. Auflisten über `ls`, Ordner anlegen über `mkdir`.
//! Gleiche Zusagen wie bei SFTP: nie überschreiben («Name (2).ext»), Teildateien heissen `*.sesam-teil`
//! und verschwinden bei Fehler oder Abbruch. Umbenennen, Löschen und Rechte gibt es hier nicht.

use crate::sftp::{Cleanup, Entry, Listing, Meter, PARTIAL, Partial, Progress, local_unique, numbered, rjoin, rparent, valid_name};
use crate::ssh::Client;
use russh::ChannelMsg;
use russh::client::{Handle, Msg};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const BUF: usize = 256 * 1024;

/// Was die Dateiansicht mit SCP nicht kann.
pub const LIMITED: &str = "Mit SCP nicht möglich (Server ohne SFTP): kein Umbenennen, Löschen oder Ändern von Rechten.";

pub struct Output {
    pub code: u32,
    pub out: Vec<u8>,
    pub err: Vec<u8>,
}

/// Befehle auf dem Server ausführen. Eigene Schnittstelle, damit die Tests ohne SSH laufen.
pub trait Remote: Send + Sync + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send;
    fn run(&self, cmd: String) -> impl Future<Output = Result<Output, String>> + Send;
    fn open(&self, cmd: String) -> impl Future<Output = Result<Self::Stream, String>> + Send;
}

impl Remote for Handle<Client> {
    type Stream = russh::ChannelStream<Msg>;

    async fn run(&self, cmd: String) -> Result<Output, String> {
        let mut ch = self.channel_open_session().await.map_err(|e| e.to_string())?;
        ch.exec(true, cmd).await.map_err(|e| e.to_string())?;
        let mut o = Output { code: 255, out: Vec::new(), err: Vec::new() };
        while let Some(msg) = ch.wait().await {
            match msg {
                ChannelMsg::Data { data } => o.out.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => o.err.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => o.code = exit_status,
                ChannelMsg::Failure => return Err("Der Server lehnt Befehle ab (exec)".into()),
                _ => {}
            }
        }
        Ok(o)
    }

    async fn open(&self, cmd: String) -> Result<Self::Stream, String> {
        let ch = self.channel_open_session().await.map_err(|e| e.to_string())?;
        ch.exec(true, cmd).await.map_err(|e| e.to_string())?;
        Ok(ch.into_stream())
    }
}

pub struct Scp<R: Remote = Handle<Client>> {
    r: R,
}

/// Für die Shell in einfache Anführungszeichen setzen. Zeilenumbrüche lehnen wir ab (SCP-Protokoll ist zeilenbasiert).
pub fn quote(s: &str) -> Result<String, String> {
    if s.contains(['\n', '\r', '\0']) {
        return Err(format!("Name mit Zeilenumbruch wird nicht unterstützt: {:?}", s));
    }
    Ok(format!("'{}'", s.replace('\'', r"'\''")))
}

/// Ein einzelner Name aus dem SCP-Protokoll oder vom Server: darf nicht aus dem Zielordner führen.
pub fn check_name(name: &str) -> Result<(), String> {
    let bad_win = cfg!(windows) && name.contains(':');
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\n', '\r', '\0']) || bad_win {
        return Err(format!("Ungültiger Name vom Server: {name:?}"));
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum Record {
    File { mode: u32, size: u64, name: String },
    Dir { mode: u32, name: String },
    End,
    Time,
}

/// Eine Kopfzeile ohne `\n`: `C0644 123 name`, `D0755 0 name`, `E`, `T…`.
pub fn parse_record(line: &str) -> Result<Record, String> {
    let bad = || format!("Unerwartete SCP-Antwort: {line:?}");
    let kind = line.chars().next().ok_or_else(bad)?;
    match kind {
        'E' if line == "E" => Ok(Record::End),
        'T' => Ok(Record::Time),
        'C' | 'D' => {
            let mut it = line[1..].splitn(3, ' ');
            let (mode, size, name) = (it.next().ok_or_else(bad)?, it.next().ok_or_else(bad)?, it.next().ok_or_else(bad)?);
            let mode = u32::from_str_radix(mode, 8).map_err(|_| bad())?;
            let size: u64 = size.parse().map_err(|_| bad())?;
            check_name(name)?;
            let name = name.to_string();
            Ok(if kind == 'C' { Record::File { mode, size, name } } else { Record::Dir { mode, name } })
        }
        _ => Err(bad()),
    }
}

async fn read_line<S: AsyncRead + Unpin>(s: &mut S) -> Result<String, String> {
    let mut line = Vec::new();
    loop {
        let b = s.read_u8().await.map_err(|_| "SCP: Verbindung unerwartet beendet".to_string())?;
        if b == b'\n' {
            break;
        }
        if line.len() > 8192 {
            return Err("SCP: Zeile zu lang".into());
        }
        line.push(b);
    }
    String::from_utf8(line).map_err(|_| "SCP: Name ist kein UTF-8".into())
}

/// Quittung lesen: 0 = gut, 1/2 + Meldung = Fehler.
pub async fn ack<S: AsyncRead + Unpin>(s: &mut S) -> Result<(), String> {
    match s.read_u8().await.map_err(|_| "SCP: Verbindung unerwartet beendet (läuft «scp» auf dem Server?)".to_string())? {
        0 => Ok(()),
        1 | 2 => Err(format!("Server: {}", read_line(s).await?.trim_start_matches("scp: "))),
        b => Err(format!("SCP: unerwartete Quittung {b}")),
    }
}

/// Ausgabe von `LC_ALL=C ls -lAL` in Einträge. Gerätedateien und Unlesbares fallen weg; Datum fehlt (Format je nach System).
pub fn parse_ls(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let dir = match line.as_bytes().first() {
            Some(b'd') => true,
            Some(b'-') => false,
            _ => continue,
        };
        // Rechte, Links, Benutzer, Gruppe, Grösse, Monat, Tag, Zeit/Jahr, dann der Name (darf Leerzeichen enthalten).
        let mut rest = line;
        let mut fields = Vec::new();
        for _ in 0..8 {
            rest = rest.trim_start_matches(' ');
            let end = rest.find(' ').unwrap_or(rest.len());
            fields.push(&rest[..end]);
            rest = &rest[end..];
        }
        let name = rest.strip_prefix(' ').unwrap_or(rest);
        let Ok(size) = fields[4].parse::<u64>() else { continue };
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        out.push(Entry { name: name.to_string(), dir, size: if dir { 0 } else { size }, modified: None });
    }
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Entfernte Teildatei bei Fehler oder Abbruch löschen.
struct RemoteTmp<R: Remote>(Option<(Arc<Scp<R>>, String)>);

impl<R: Remote> Drop for RemoteTmp<R> {
    fn drop(&mut self) {
        if let Some((s, p)) = self.0.take()
            && let (Ok(rt), Ok(q)) = (tokio::runtime::Handle::try_current(), quote(&p))
        {
            rt.spawn(async move {
                let _ = s.r.run(format!("rm -f -- {q}")).await;
            });
        }
    }
}

enum Step {
    Enter(String),
    File(PathBuf, String, u64),
    Leave,
}

/// Lokalen Baum als SCP-Schritte (verschachtelt wie `scp -r`), oberstes Element unter `name`.
fn plan(src: &Path, name: String, steps: &mut Vec<Step>, total: &mut u64) -> Result<(), String> {
    check_name(&name).map_err(|_| format!("{}: Name lässt sich mit SCP nicht übertragen", src.display()))?;
    let m = std::fs::metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
    if m.is_dir() {
        steps.push(Step::Enter(name));
        for e in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))?.flatten() {
            plan(&e.path(), e.file_name().to_string_lossy().into_owned(), steps, total)?;
        }
        steps.push(Step::Leave);
    } else {
        *total += m.len();
        steps.push(Step::File(src.to_path_buf(), name, m.len()));
    }
    Ok(())
}

fn basename(p: &str) -> Option<&str> {
    p.trim_end_matches('/').rsplit('/').next().filter(|n| !n.is_empty())
}

impl<R: Remote> Scp<R> {
    pub fn new(r: R) -> Self {
        Self { r }
    }

    async fn ok(&self, cmd: String) -> Result<String, String> {
        let o = self.r.run(cmd).await?;
        if o.code != 0 {
            let e = String::from_utf8_lossy(&o.err).trim().to_string();
            return Err(if e.is_empty() { format!("Befehl auf dem Server fehlgeschlagen (Code {})", o.code) } else { e });
        }
        Ok(String::from_utf8_lossy(&o.out).into_owned())
    }

    async fn exists(&self, p: &str) -> Result<bool, String> {
        let q = quote(p)?;
        Ok(self.r.run(format!("test -e {q} || test -L {q}")).await?.code == 0)
    }

    async fn unique(&self, dir: &str, name: &str, is_dir: bool) -> Result<String, String> {
        let mut p = rjoin(dir, name);
        let mut n = 2;
        while self.exists(&p).await? {
            p = rjoin(dir, &numbered(name, n, is_dir));
            n += 1;
        }
        Ok(p)
    }

    pub async fn list(&self, path: &str) -> Result<Listing, String> {
        let cd = if path.is_empty() { "cd".to_string() } else { format!("cd -- {}", quote(path)?) };
        // ls-Fehler (z. B. defekte Verknüpfungen) nicht als Abbruch werten, nur ein fehlgeschlagenes cd.
        let out = self.ok(format!("{cd} && pwd && {{ LC_ALL=C ls -lAL; true; }}")).await?;
        let (path, rest) = out.split_once('\n').ok_or("Ordner nicht lesbar")?;
        Ok(Listing { parent: rparent(path), path: path.to_string(), entries: parse_ls(rest) })
    }

    pub async fn mkdir(&self, dir: &str, name: &str) -> Result<String, String> {
        valid_name(name)?;
        let p = rjoin(dir, name);
        self.ok(format!("mkdir -- {}", quote(&p)?)).await?;
        Ok(p)
    }
}

/// Wie `sftp::upload`, aber über `scp -t`.
pub async fn upload<R: Remote>(s: &Arc<Scp<R>>, local: &Path, remote_dir: &str, report: impl Fn(Progress)) -> Result<String, String> {
    let name = local.file_name().ok_or("Ungültige Quelle")?.to_string_lossy().into_owned();
    let root = s.unique(remote_dir, &name, local.is_dir()).await?;
    let (mut steps, mut total) = (Vec::new(), 0);
    plan(local, basename(&root).ok_or("Ungültiges Ziel")?.to_string(), &mut steps, &mut total)?;

    let mut st = s.r.open(format!("scp -r -t -- {}", quote(remote_dir)?)).await?;
    ack(&mut st).await?;
    let mut m = Meter { done: 0, total, last: Instant::now(), report };
    let mut cwd = vec![remote_dir.to_string()];
    let mut buf = vec![0u8; BUF];
    for step in steps {
        let w = |e: std::io::Error| format!("SCP: {e}");
        match step {
            Step::Enter(n) => {
                st.write_all(format!("D0755 0 {n}\n").as_bytes()).await.map_err(w)?;
                ack(&mut st).await?;
                cwd.push(rjoin(cwd.last().unwrap(), &n));
            }
            Step::Leave => {
                st.write_all(b"E\n").await.map_err(w)?;
                ack(&mut st).await?;
                cwd.pop();
            }
            Step::File(src, n, size) => {
                let dir = cwd.last().unwrap().clone();
                let tmp = rjoin(&dir, &format!("{n}{PARTIAL}"));
                let mut f = tokio::fs::File::open(&src).await.map_err(|e| format!("{}: {e}", src.display()))?;
                st.write_all(format!("C0644 {size} {n}{PARTIAL}\n").as_bytes()).await.map_err(w)?;
                ack(&mut st).await?;
                let mut guard = RemoteTmp(Some((s.clone(), tmp.clone())));
                let label = src.to_string_lossy();
                let mut left = size;
                while left > 0 {
                    let k = f.read(&mut buf[..left.min(BUF as u64) as usize]).await.map_err(|e| format!("{label}: {e}"))?;
                    if k == 0 {
                        return Err(format!("{label}: Datei wurde während der Übertragung kürzer"));
                    }
                    st.write_all(&buf[..k]).await.map_err(w)?;
                    left -= k as u64;
                    m.add(k as u64, &label);
                }
                st.write_all(&[0]).await.map_err(w)?;
                ack(&mut st).await?;
                s.ok(format!("mv -- {} {}", quote(&tmp)?, quote(&rjoin(&dir, &n))?)).await?;
                guard.0 = None;
            }
        }
    }
    let _ = st.shutdown().await;
    (m.report)(Progress::Done { target: root.clone() });
    Ok(root)
}

/// Wie `sftp::download`, aber über `scp -f`. Die Gesamtgrösse von Ordnern wächst mit, weil SCP sie vorher nicht nennt.
pub async fn download<R: Remote>(s: &Arc<Scp<R>>, remote: &str, local_dir: &Path, report: impl Fn(Progress)) -> Result<String, String> {
    let name = basename(remote).ok_or("Ungültige Quelle")?;
    let mut st = s.r.open(format!("scp -r -f -- {}", quote(remote)?)).await?;
    let w = |e: std::io::Error| format!("SCP: {e}");
    st.write_all(&[0]).await.map_err(w)?;

    let mut m = Meter { done: 0, total: 0, last: Instant::now(), report };
    let mut root: Option<PathBuf> = None;
    let mut cwd: Vec<PathBuf> = Vec::new();
    let mut buf = vec![0u8; BUF];
    loop {
        // Nach dem obersten Element ist Schluss.
        if root.is_some() && cwd.is_empty() {
            break;
        }
        let first = st.read_u8().await.map_err(|_| "SCP: Verbindung unerwartet beendet".to_string())?;
        if first == 1 || first == 2 {
            return Err(format!("Server: {}", read_line(&mut st).await?.trim_start_matches("scp: ")));
        }
        let line = format!("{}{}", first as char, read_line(&mut st).await?);
        let rec = parse_record(&line)?;
        // Das oberste Element bekommt einen freien lokalen Namen.
        let target = |n: &str, is_dir: bool, root: &mut Option<PathBuf>, cwd: &Vec<PathBuf>| match cwd.last() {
            Some(d) => d.join(n),
            None => {
                let p = local_unique(local_dir, name, is_dir);
                *root = Some(p.clone());
                p
            }
        };
        match rec {
            Record::Time => {}
            Record::End => {
                cwd.pop().ok_or_else(|| "SCP: unerwartetes Ende".to_string())?;
            }
            Record::Dir { name: n, .. } => {
                let p = target(&n, true, &mut root, &cwd);
                tokio::fs::create_dir_all(&p).await.map_err(|e| format!("{}: {e}", p.display()))?;
                cwd.push(p);
            }
            Record::File { name: n, size, .. } => {
                let p = target(&n, false, &mut root, &cwd);
                let tmp = PathBuf::from(format!("{}{PARTIAL}", p.display()));
                let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| format!("{}: {e}", tmp.display()))?;
                let mut guard = Partial(Some(Cleanup::Local(tmp.clone())));
                st.write_all(&[0]).await.map_err(w)?;
                m.total += size;
                let label = rjoin(remote, &n);
                let mut left = size;
                while left > 0 {
                    let k = st.read(&mut buf[..left.min(BUF as u64) as usize]).await.map_err(w)?;
                    if k == 0 {
                        return Err("SCP: Verbindung unerwartet beendet".into());
                    }
                    f.write_all(&buf[..k]).await.map_err(|e| format!("{}: {e}", p.display()))?;
                    left -= k as u64;
                    m.add(k as u64, &label);
                }
                f.shutdown().await.map_err(|e| format!("{}: {e}", p.display()))?;
                drop(f);
                ack(&mut st).await?;
                tokio::fs::rename(&tmp, &p).await.map_err(|e| format!("{}: {e}", p.display()))?;
                guard.0 = None;
                if cwd.is_empty() {
                    st.write_all(&[0]).await.map_err(w)?;
                    break;
                }
            }
        }
        st.write_all(&[0]).await.map_err(w)?;
    }
    let root = root.ok_or("SCP: nichts empfangen")?.to_string_lossy().into_owned();
    (m.report)(Progress::Done { target: root.clone() });
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use tokio::io::{DuplexStream, duplex};

    #[test]
    fn quoting() {
        assert_eq!(quote("/home/a b").unwrap(), "'/home/a b'");
        assert_eq!(quote("it's").unwrap(), r"'it'\''s'");
        assert_eq!(quote("$(rm -rf ~)").unwrap(), "'$(rm -rf ~)'");
        assert!(quote("a\nb").is_err());
        assert!(quote("a\rb").is_err());
        assert_eq!(words(&quote("x'y z").unwrap()), ["x'y z"]);
    }

    #[test]
    fn records() {
        assert_eq!(parse_record("C0644 12 a b.txt").unwrap(), Record::File { mode: 0o644, size: 12, name: "a b.txt".into() });
        assert_eq!(parse_record("D0755 0 ordner").unwrap(), Record::Dir { mode: 0o755, name: "ordner".into() });
        assert_eq!(parse_record("E").unwrap(), Record::End);
        assert_eq!(parse_record("T1700000000 0 1700000000 0").unwrap(), Record::Time);
        for bad in ["C0644 12 ../x", "C0644 12 a/b", "C0644 12 ..", "D0755 0 .", "C0644 x a", "C9 1 a", "C0644 1", "X", "", "Ex", "C0644 1 a\\b"] {
            assert!(parse_record(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn acks() {
        assert!(ack(&mut &[0u8][..]).await.is_ok());
        assert_eq!(ack(&mut &b"\x01scp: /x: Permission denied\n"[..]).await.unwrap_err(), "Server: /x: Permission denied");
        assert!(ack(&mut &b"\x02kaputt\n"[..]).await.unwrap_err().contains("kaputt"));
        assert!(ack(&mut &[][..]).await.is_err());
        assert!(ack(&mut &[7u8][..]).await.is_err());
    }

    #[test]
    fn ls_output() {
        let out = "total 12\n\
drwxr-xr-x  2 1000 1000  4096 Jan  1 12:00 Ordner mit Leerzeichen\n\
-rw-r--r--  1 1000 1000    42 Dec 31  2023 b.txt\n\
-rw-r--r--+ 1 0 0 7 Feb 3 09:15  zwei  Leer\n\
crw-rw-rw-  1 0 0 1, 3 Jan 1 12:00 null\n\
l?????????  ? ?    ?        ?            ? kaputt\n";
        let e = parse_ls(out);
        let got: Vec<_> = e.iter().map(|e| (e.name.as_str(), e.dir, e.size)).collect();
        assert_eq!(got, [("Ordner mit Leerzeichen", true, 0), (" zwei  Leer", false, 7), ("b.txt", false, 42)]);
    }

    // ---------- Gegenstelle im Speicher: Shell-Befehle und scp -t / scp -f ----------

    /// Wörter einer Befehlszeile mit einfachen Anführungszeichen (wie `quote` sie baut).
    fn words(cmd: &str) -> Vec<String> {
        let (mut out, mut cur, mut quoted, mut any, mut esc) = (Vec::new(), String::new(), false, false, false);
        for c in cmd.chars() {
            match c {
                c if esc => (cur.push(c), esc = false).1,
                '\'' => (quoted, any) = (!quoted, true),
                ' ' if !quoted => {
                    if any {
                        out.push(std::mem::take(&mut cur));
                    }
                    any = false;
                }
                '\\' if !quoted => esc = true,
                c => (cur.push(c), any = true).1,
            }
        }
        if any {
            out.push(cur);
        }
        out
    }

    type Fs = Arc<Mutex<BTreeMap<String, Option<Vec<u8>>>>>;

    struct Fake {
        fs: Fs,
        /// Senke meldet nach so vielen Bytes einer Datei einen Fehler (volle Platte).
        fail_above: Option<u64>,
    }

    impl Remote for Fake {
        type Stream = DuplexStream;
        async fn run(&self, cmd: String) -> Result<Output, String> {
            let w = words(&cmd);
            let mut fs = self.fs.lock().unwrap();
            let ok = |b: bool| Output { code: if b { 0 } else { 1 }, out: vec![], err: vec![] };
            Ok(match w.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
                ["test", "-e", p, "||", "test", "-L", _] => ok(fs.contains_key(*p)),
                ["mv", "--", a, b] => {
                    let v = fs.remove(*a);
                    ok(v.map(|v| fs.insert(b.to_string(), v)).is_some())
                }
                ["rm", "-f", "--", a] => ok(fs.remove(*a).is_some() || true),
                ["mkdir", "--", p] => ok(fs.insert(p.to_string(), None).is_none()),
                _ => panic!("unerwarteter Befehl: {cmd}"),
            })
        }
        async fn open(&self, cmd: String) -> Result<DuplexStream, String> {
            let w = words(&cmd);
            let (a, b) = duplex(1 << 16);
            let (fs, fail) = (self.fs.clone(), self.fail_above);
            match w.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
                ["scp", "-r", "-t", "--", dir] => drop(tokio::spawn(sink(b, fs, dir.to_string(), fail))),
                ["scp", "-r", "-f", "--", p] => drop(tokio::spawn(source(b, fs, p.to_string()))),
                _ => panic!("unerwarteter Befehl: {cmd}"),
            }
            Ok(a)
        }
    }

    async fn sink(mut s: DuplexStream, fs: Fs, dir: String, fail: Option<u64>) {
        let mut cwd = vec![dir];
        s.write_all(&[0]).await.unwrap();
        while let Ok(line) = read_line(&mut s).await {
            let cur = cwd.last().unwrap().clone();
            match parse_record(&line).unwrap() {
                Record::Dir { name, .. } => {
                    fs.lock().unwrap().insert(rjoin(&cur, &name), None);
                    cwd.push(rjoin(&cur, &name));
                }
                Record::End => drop(cwd.pop()),
                Record::File { size, name, .. } => {
                    s.write_all(&[0]).await.unwrap();
                    let mut data = vec![0; size as usize];
                    s.read_exact(&mut data).await.unwrap();
                    assert_eq!(s.read_u8().await.unwrap(), 0);
                    fs.lock().unwrap().insert(rjoin(&cur, &name), Some(data));
                    if fail.is_some_and(|f| size > f) {
                        s.write_all(b"\x02scp: Kein Platz\n").await.unwrap();
                        return;
                    }
                }
                Record::Time => {}
            }
            s.write_all(&[0]).await.unwrap();
        }
    }

    async fn source(mut s: DuplexStream, fs: Fs, path: String) {
        assert_eq!(s.read_u8().await.unwrap(), 0);
        let nodes: Vec<_> = fs.lock().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let Some(root) = nodes.iter().find(|(k, _)| *k == path) else {
            s.write_all(format!("\x01scp: {path}: No such file or directory\n").as_bytes()).await.unwrap();
            return;
        };
        // Tiefe-zuerst: Kopfzeile, Quittung, Daten, Quittung.
        let mut stack = vec![(root.0.clone(), root.1.clone(), false)];
        while let Some((p, node, leave)) = stack.pop() {
            let name = p.rsplit('/').next().unwrap();
            match (node, leave) {
                (_, true) => s.write_all(b"E\n").await.unwrap(),
                (None, false) => {
                    s.write_all(format!("D0755 0 {name}\n").as_bytes()).await.unwrap();
                    stack.push((p.clone(), None, true));
                    let prefix = format!("{p}/");
                    for (k, v) in nodes.iter().rev() {
                        if k.strip_prefix(&prefix).is_some_and(|r| !r.contains('/')) {
                            stack.push((k.clone(), v.clone(), false));
                        }
                    }
                }
                (Some(d), false) => {
                    s.write_all(format!("C0644 {} {name}\n", d.len()).as_bytes()).await.unwrap();
                    assert_eq!(s.read_u8().await.unwrap(), 0);
                    s.write_all(&d).await.unwrap();
                    s.write_all(&[0]).await.unwrap();
                }
            }
            assert_eq!(s.read_u8().await.unwrap(), 0);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn upload_and_download_over_scp() {
        let fs: Fs = Default::default();
        fs.lock().unwrap().insert("/home".into(), None);
        let scp = Arc::new(Scp::new(Fake { fs: fs.clone(), fail_above: None }));

        let base = std::env::temp_dir().join(format!("sesam-scp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("projekt");
        std::fs::create_dir_all(src.join("unter")).unwrap();
        std::fs::write(src.join("it's.txt"), "Grüezi").unwrap();
        let big: Vec<u8> = (0..700_000u32).map(|i| (i * 7 % 251) as u8).collect();
        std::fs::write(src.join("unter").join("gross.bin"), &big).unwrap();

        let last = Arc::new(Mutex::new(None));
        let l2 = last.clone();
        assert_eq!(upload(&scp, &src, "/home", move |p| *l2.lock().unwrap() = Some(p)).await.unwrap(), "/home/projekt");
        assert!(matches!(last.lock().unwrap().take(), Some(Progress::Done { .. })));
        {
            let fs = fs.lock().unwrap();
            assert_eq!(fs["/home/projekt/it's.txt"].as_deref(), Some("Grüezi".as_bytes()));
            assert_eq!(fs["/home/projekt/unter/gross.bin"].as_deref(), Some(big.as_slice()));
            assert!(!fs.keys().any(|k| k.ends_with(PARTIAL)));
        }
        // Nie überschreiben.
        assert_eq!(upload(&scp, &src, "/home", |_| {}).await.unwrap(), "/home/projekt (2)");

        let back = download(&scp, "/home/projekt", &base, |_| {}).await.unwrap();
        assert_eq!(PathBuf::from(&back), base.join("projekt (2)"));
        assert_eq!(std::fs::read(base.join("projekt (2)").join("unter").join("gross.bin")).unwrap(), big);
        assert_eq!(std::fs::read_to_string(base.join("projekt (2)").join("it's.txt")).unwrap(), "Grüezi");
        let one = download(&scp, "/home/projekt/it's.txt", &base, |_| {}).await.unwrap();
        assert_eq!(std::fs::read_to_string(one).unwrap(), "Grüezi");
        assert!(download(&scp, "/home/fehlt", &base, |_| {}).await.unwrap_err().contains("No such file"));

        // Fehler der Gegenstelle: Teildatei wird wieder gelöscht.
        let scp = Arc::new(Scp::new(Fake { fs: fs.clone(), fail_above: Some(1000) }));
        let e = upload(&scp, &src.join("unter").join("gross.bin"), "/home", |_| {}).await.unwrap_err();
        assert!(e.contains("Kein Platz"), "{e}");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let fs = fs.lock().unwrap();
        assert!(!fs.keys().any(|k| k.ends_with(PARTIAL) || k == "/home/gross.bin"), "{:?}", fs.keys().collect::<Vec<_>>());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
