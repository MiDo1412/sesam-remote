//! Dateien: SFTP, SCP, FTP und lokale Dateien.

use crate::*;

// ---------- Dateien (SFTP, sonst SCP; oder FTP) ----------

#[derive(Clone)]
pub(crate) enum FileSession {
    Sftp(Arc<sftp::Sftp>),
    Scp(Arc<scp::Scp>),
    Ftp(Arc<ftp::Ftp>),
}

impl FileSession {
    fn closed(&self) -> bool {
        match self {
            FileSession::Sftp(s) => s.closed(),
            FileSession::Scp(s) => s.closed(),
            FileSession::Ftp(_) => false, // FTP meldet den Abbruch beim nächsten Befehl selbst
        }
    }
}

/// Offene Dateisitzungen und, für SSH, die Verbindung, mit der sie sich nach einem Abbruch neu aufbauen.
#[derive(Default)]
pub(crate) struct Files(pub(crate) Mutex<HashMap<String, FileSession>>, pub(crate) Mutex<HashMap<String, Connection>>);

#[derive(Default)]
/// Laufende Übertragungen: Aufgabe und Pause-Schalter.
pub(crate) struct Transfers(pub(crate) Arc<Mutex<HashMap<String, (tauri::async_runtime::JoinHandle<()>, Arc<std::sync::atomic::AtomicBool>)>>>);

/// Sitzung zu `sid`. Ist ihre SSH-Verbindung weg (Server neu gestartet, Netzwechsel), baut Sesam sie einmal unter
/// derselben Kennung neu auf, damit Vorschau, Bearbeiten, Übertragungen usw. einfach weiterlaufen.
pub(crate) async fn files_get(files: &Files, sid: &str) -> Result<FileSession, String> {
    files_live(files, sid, FileSession::closed, |c| async move { ssh_files_open(&c, None).await }).await
}

pub(crate) async fn files_live<F, Fut>(files: &Files, sid: &str, closed: fn(&FileSession) -> bool, reopen: F) -> Result<FileSession, String>
where
    F: FnOnce(Connection) -> Fut,
    Fut: std::future::Future<Output = Result<FileSession, String>>,
{
    let s = files.0.lock().unwrap().get(sid).cloned().ok_or("Dateisitzung geschlossen")?;
    if !closed(&s) {
        return Ok(s);
    }
    let Some(c) = files.1.lock().unwrap().get(sid).cloned() else { return Ok(s) };
    tracing::info!("Dateisitzung {sid}: Verbindung weg, baue sie neu auf");
    let fresh = reopen(c).await?;
    // Wurde der Tab inzwischen geschlossen, nicht wieder eintragen.
    let mut map = files.0.lock().unwrap();
    if !map.contains_key(sid) {
        return Err("Dateisitzung geschlossen".into());
    }
    map.insert(sid.to_string(), fresh.clone());
    Ok(fresh)
}

#[derive(serde::Serialize)]
pub(crate) struct FilesOpened {
    pub(crate) sid: String,
    /// `true` = Server ohne SFTP, Übertragung über SCP (eingeschränkt).
    pub(crate) scp: bool,
}

/// FTP-Sitzung öffnen; `trust` = bestätigter Zertifikat-Fingerabdruck, gemerkt unter "host:port" wie bei RDP/VNC.
pub(crate) async fn ftp_open(path: &LibraryPath, c: &Connection, trust: Option<String>, remember: bool) -> Result<ftp::Ftp, String> {
    let port = c.ftp_port.ok_or("FTP ist für diese Verbindung ausgeschaltet")?;
    let pins_path = path.0.with_file_name("known_certs.json");
    let pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let password = login_secret(c, Kind::Password)?.unwrap_or_default();
    diag::add_secret(&password);
    let t = ftp::Target {
        host: c.address.clone(),
        port,
        user: c.user.clone(),
        password,
        security: ftp::Security::parse(&c.ftp_security)?,
        active: c.ftp_active,
        cert: trust.clone().or_else(|| pins.get(&pin_key).cloned()),
        skip_cert_check: c.skip_cert_check,
    };
    let (s, seen) = ftp::Ftp::open(t).await?;
    if let Some(fp) = trust.filter(|fp| remember && seen.as_ref() == Some(fp)) {
        let _ = save_pins(&pins_path, &pin_key, Some(fp));
    }
    Ok(s)
}

/// `trust`: bei SSH der Host-Schlüssel, bei FTPS das Zertifikat (`remember` = merken).
#[tauri::command]
pub(crate) async fn sftp_open(path: State<'_, LibraryPath>, files: State<'_, Files>, id: String, trust: Option<String>, remember: Option<bool>) -> Result<FilesOpened, String> {
    let c = find(&path, &id)?;
    if c.files_over_ftp() {
        let s = ftp_open(&path, &c, trust, remember.unwrap_or(true)).await?;
        let sid = new_sid(&c.id);
        files.0.lock().unwrap().insert(sid.clone(), FileSession::Ftp(Arc::new(s)));
        touch(&path, &c.id);
        return Ok(FilesOpened { sid, scp: false });
    }
    let session = ssh_files_open(&c, trust).await?;
    let sid = new_sid(&c.id);
    touch(&path, &c.id);
    let scp = matches!(session, FileSession::Scp(_));
    files.0.lock().unwrap().insert(sid.clone(), session);
    files.1.lock().unwrap().insert(sid.clone(), c);
    Ok(FilesOpened { sid, scp })
}

/// SSH-Dateisitzung: SFTP, sonst (wenn erlaubt) SCP.
pub(crate) async fn ssh_files_open(c: &Connection, trust: Option<String>) -> Result<FileSession, String> {
    let h = ssh::connect(&ssh_target(c, trust)?).await?;
    Ok(match c.file_transfer.as_str() {
        "scp" => FileSession::Scp(Arc::new(scp::Scp::new(h))),
        mode => match sftp::start(h).await {
            Ok(s) => FileSession::Sftp(Arc::new(s)),
            Err((_, e)) if mode == "sftp" => return Err(e),
            Err((h, e)) => {
                tracing::info!("SFTP nicht verfügbar ({e}), weiter mit SCP");
                FileSession::Scp(Arc::new(scp::Scp::new(h)))
            }
        },
    })
}

#[tauri::command]
pub(crate) async fn sftp_list(files: State<'_, Files>, sid: String, path: String) -> Result<sftp::Listing, String> {
    match files_get(&files, &sid).await? {
        FileSession::Sftp(s) => s.list(&path).await,
        FileSession::Scp(s) => s.list(&path).await,
        FileSession::Ftp(s) => s.list(&path).await,
    }
}

#[tauri::command]
pub(crate) async fn sftp_mkdir(files: State<'_, Files>, sid: String, dir: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid).await? {
        FileSession::Sftp(s) => s.mkdir(&dir, &name).await,
        FileSession::Scp(s) => s.mkdir(&dir, &name).await,
        FileSession::Ftp(s) => s.mkdir(&dir, &name).await,
    }
}

#[tauri::command]
pub(crate) async fn sftp_rename(files: State<'_, Files>, sid: String, path: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid).await? {
        FileSession::Sftp(s) => s.rename(&path, &name).await,
        FileSession::Ftp(s) => s.rename(&path, &name).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

#[tauri::command]
pub(crate) async fn sftp_remove(files: State<'_, Files>, sid: String, path: String) -> Result<(), String> {
    match files_get(&files, &sid).await? {
        FileSession::Sftp(s) => s.remove(&path).await,
        FileSession::Ftp(s) => s.remove(&path).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

#[tauri::command]
pub(crate) async fn sftp_chmod(files: State<'_, Files>, sid: String, path: String, mode: u32) -> Result<(), String> {
    match files_get(&files, &sid).await? {
        FileSession::Sftp(s) => s.chmod(&path, mode).await,
        FileSession::Ftp(s) => s.chmod(&path, mode).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

pub(crate) async fn ssh_files(files: &Files, sid: &str) -> Result<Arc<sftp::Sftp>, String> {
    match files_get(files, sid).await? {
        FileSession::Sftp(s) => Ok(s),
        _ => Err("Nur bei Dateien über SSH möglich".into()),
    }
}

/// Besitzer und Gruppe («benutzer:gruppe») für den Rechte-Dialog; Linux `stat -c`, BSD/macOS `stat -f`.
#[tauri::command]
pub(crate) async fn sftp_owner(files: State<'_, Files>, sid: String, path: String) -> Result<String, String> {
    let q = scp::quote(&path)?;
    ssh_files(&files, &sid).await?.exec(&format!("stat -c '%U:%G' -- {q} 2>/dev/null || stat -f '%Su:%Sg' -- {q}"), false, None).await
}

/// Rechte und Besitzer setzen, auf Wunsch rekursiv und mit sudo (nur nach Bestätigung in der Oberfläche).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sftp_set_perms(files: State<'_, Files>, sid: String, path: String, mode: Option<u32>, owner: Option<String>, recursive: bool, sudo: bool, password: Option<String>) -> Result<(), String> {
    let script = sftp::perms_script(&path, mode, owner.as_deref(), recursive)?;
    ssh_files(&files, &sid).await?.exec(&script, sudo, password.as_deref()).await.map(|_| ())
}

/// Temporärer Ordner auf dem Server (für Hochladen mit Administratorrechten).
#[tauri::command]
pub(crate) async fn sftp_tempdir(files: State<'_, Files>, sid: String) -> Result<String, String> {
    ssh_files(&files, &sid).await?.exec("mktemp -d", false, None).await
}

/// Hochgeladenes aus dem temporären Ordner mit sudo an den Zielort verschieben (nie überschreiben).
#[tauri::command]
pub(crate) async fn sftp_sudo_move(files: State<'_, Files>, sid: String, from: String, dest_dir: String, replace: bool, password: Option<String>) -> Result<(), String> {
    let script = sftp::move_script(&from, &dest_dir, replace)?;
    ssh_files(&files, &sid).await?.exec(&script, true, password.as_deref()).await.map(|_| ())
}

/// In den Papierkorb auf dem Server statt löschen; gibt den Pfad im Papierkorb zurück.
#[tauri::command]
pub(crate) async fn sftp_trash(files: State<'_, Files>, sid: String, path: String) -> Result<String, String> {
    ssh_files(&files, &sid).await?.to_trash(&path).await
}

/// Zeitstempel-Ordner im Papierkorb anlegen (für die Erhöhung mit sudo).
#[tauri::command]
pub(crate) async fn sftp_trash_slot(files: State<'_, Files>, sid: String) -> Result<String, String> {
    ssh_files(&files, &sid).await?.trash_slot().await
}

/// Grösse (für das Grössenlimit des Papierkorbs).
#[tauri::command]
pub(crate) async fn sftp_size(files: State<'_, Files>, sid: String, path: String) -> Result<u64, String> {
    ssh_files(&files, &sid).await?.size(&path).await
}

/// Rückgängig: aus dem Papierkorb an den alten Ort.
#[tauri::command]
pub(crate) async fn sftp_restore(files: State<'_, Files>, sid: String, trashed: String, original: String) -> Result<(), String> {
    ssh_files(&files, &sid).await?.restore(&trashed, &original).await
}

/// Papierkorb aufräumen (älter als `days` Tage).
#[tauri::command]
pub(crate) async fn sftp_trash_cleanup(files: State<'_, Files>, sid: String, days: u32) -> Result<usize, String> {
    match files_get(&files, &sid).await? {
        FileSession::Ftp(f) => Ok(f.backup_cleanup(days)),
        _ => ssh_files(&files, &sid).await?.trash_cleanup(days).await,
    }
}

/// Löschen, Umbenennen, Ordner/Datei anlegen oder zum Herunterladen kopieren, mit sudo (nur nach Bestätigung).
#[tauri::command]
pub(crate) async fn sftp_sudo_op(files: State<'_, Files>, sid: String, op: String, path: String, arg: String, password: Option<String>) -> Result<(), String> {
    let script = sftp::sudo_op_script(&op, &path, &arg)?;
    ssh_files(&files, &sid).await?.exec(&script, true, password.as_deref()).await.map(|_| ())
}

#[tauri::command]
pub(crate) async fn sftp_touch(files: State<'_, Files>, sid: String, dir: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid).await? {
        FileSession::Sftp(s) => s.touch(&dir, &name).await,
        FileSession::Ftp(s) => s.touch(&dir, &name).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

/// Prüfsumme einer Datei auf dem Server (für «Inhalt prüfen» im Vergleich).
#[tauri::command]
pub(crate) async fn sftp_sha256(files: State<'_, Files>, sid: String, path: String) -> Result<String, String> {
    match files_get(&files, &sid).await? {
        FileSession::Sftp(s) => s.sha256(&path).await,
        FileSession::Scp(s) => s.sha256(&path).await,
        FileSession::Ftp(_) => Err("Inhalt prüfen geht bei FTP nicht (keine Prüfsummen auf dem Server); verglichen wird nach Grösse und Datum.".into()),
    }
}

#[tauri::command]
pub(crate) async fn local_sha256(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || sftp::local_sha256(&path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) fn local_mkdir(dir: String, name: String) -> Result<String, String> {
    sftp::local_mkdir(&dir, &name)
}

#[tauri::command]
pub(crate) fn local_rename(path: String, name: String) -> Result<String, String> {
    sftp::local_rename(&path, &name)
}

#[tauri::command]
pub(crate) fn local_touch(dir: String, name: String) -> Result<String, String> {
    sftp::local_touch(&dir, &name)
}

/// Eigener Thread: der Papierkorb unter Windows braucht COM im Single-Thread-Modus.
#[tauri::command]
pub(crate) async fn local_trash(paths: Vec<String>) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || tx.send(sftp::local_trash(&paths)));
    rx.await.map_err(|_| "Löschen abgebrochen".to_string())?
}

/// Profile des lokalen Terminals (installierte Shells und Interpreter).
#[tauri::command]
pub(crate) async fn term_profiles() -> Vec<profiles::Profile> {
    tauri::async_runtime::spawn_blocking(profiles::detect).await.unwrap_or_default()
}
