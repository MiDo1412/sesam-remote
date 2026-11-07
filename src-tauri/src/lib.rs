mod clipshare;
mod creds;
mod diag;
mod h264;
mod kdc;
mod forward;
mod library;
mod rdp;
mod relay;
mod rdp_redir;
mod rdstls;
mod scp;
mod sftp;
mod ssh;
mod vault;
mod vnc;

use library::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, State};
use clipshare::{ClipShare, Last, Local};
use vault::Kind;

struct LibraryPath(PathBuf);

/// Serialisiert Lese-Ändern-Schreiben der Bibliothek.
static LIBRARY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tauri::command]
fn library_list(path: State<LibraryPath>) -> Result<Vec<Connection>, String> {
    library::load(&path.0)
}

/// `password`/`vnc_password`/`ssh_key`: `None` = unverändert, `Some("")` = löschen, sonst neu setzen.
#[tauri::command]
fn library_save(
    path: State<LibraryPath>,
    mut connection: Connection,
    password: Option<String>,
    vnc_password: Option<String>,
    rdp_password: Option<String>,
    gateway_password: Option<String>,
    ssh_key: Option<String>,
) -> Result<Connection, String> {
    connection.validate()?;
    if let Some(k) = ssh_key.as_deref().filter(|k| !k.trim().is_empty()) {
        ssh::check_private_key(k)?;
    }
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut items = library::load(&path.0)?;
    let old = items.iter().find(|c| c.id == connection.id);
    connection.has_password = old.is_some_and(|c| c.has_password);
    connection.has_vnc_password = old.is_some_and(|c| c.has_vnc_password);
    connection.has_rdp_password = old.is_some_and(|c| c.has_rdp_password);
    connection.has_gateway_password = old.is_some_and(|c| c.has_gateway_password);
    connection.has_key = old.is_some_and(|c| c.has_key);
    for (secret, kind, flag) in [
        (password, Kind::Password, &mut connection.has_password),
        (vnc_password, Kind::VncPassword, &mut connection.has_vnc_password),
        (rdp_password, Kind::RdpPassword, &mut connection.has_rdp_password),
        (gateway_password, Kind::GatewayPassword, &mut connection.has_gateway_password),
        (ssh_key, Kind::SshKey, &mut connection.has_key),
    ] {
        match secret.as_deref() {
            None => {}
            Some("") => {
                vault::delete(&connection.id, kind)?;
                *flag = false;
            }
            Some(s) => {
                vault::set(&connection.id, kind, s)?;
                *flag = true;
            }
        }
    }
    library::upsert(&mut items, connection.clone());
    library::save(&path.0, &items)?;
    Ok(connection)
}

#[tauri::command]
fn library_delete(path: State<LibraryPath>, id: String) -> Result<(), String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut items = library::load(&path.0)?;
    if library::remove(&mut items, &id) {
        vault::delete(&id, Kind::Password)?;
        vault::delete(&id, Kind::SshKey)?;
        vault::delete(&id, Kind::VncPassword)?;
        vault::delete(&id, Kind::RdpPassword)?;
        vault::delete(&id, Kind::GatewayPassword)?;
        library::save(&path.0, &items)?;
    }
    Ok(())
}

fn new_sid(id: &str) -> String {
    format!("{id}-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

/// Schnelle Verbindungen: nicht in der Bibliothek, nur für diese Sitzung der App.
static QUICK: Mutex<Vec<Connection>> = Mutex::new(Vec::new());

/// Schnelle Verbindung anlegen (nichts wird gespeichert). Gibt die ID zurück.
#[tauri::command]
/// `key_file`: Pfad einer privaten Schlüsseldatei (OpenSSH oder PuTTY .ppk), nur im Arbeitsspeicher gehalten.
/// Gespeicherte Zugangsdaten (`credential_id`) liefern Schlüssel/Passwort aus dem Tresor; ohne Benutzer auch diesen.
fn quick_register(path: State<LibraryPath>, mut connection: Connection, password: Option<String>, key_file: Option<String>) -> Result<String, String> {
    connection.id = format!("quick-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    connection.validate()?;
    if let Some(cid) = &connection.credential_id {
        let cred = creds::load(&creds_path(&path))?.into_iter().find(|x| &x.id == cid).ok_or("Diese Zugangsdaten gibt es nicht mehr")?;
        if connection.user.trim().is_empty() {
            connection.user = cred.user;
        }
    }
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        diag::add_secret(&pw);
        vault::set_ephemeral(&connection.id, Kind::Password, &pw);
        connection.has_password = true;
    }
    if let Some(f) = key_file.filter(|f| !f.is_empty()) {
        let key = std::fs::read_to_string(&f).map_err(|e| format!("Schlüsseldatei nicht lesbar: {e}"))?;
        ssh::check_private_key(&key)?;
        vault::set_ephemeral(&connection.id, Kind::SshKey, &key);
        connection.has_key = true;
    }
    let id = connection.id.clone();
    QUICK.lock().unwrap().push(connection);
    Ok(id)
}

fn find(path: &LibraryPath, id: &str) -> Result<Connection, String> {
    if id.starts_with("quick-") {
        return QUICK.lock().unwrap().iter().find(|c| c.id == id).cloned().ok_or_else(|| "Schnelle Verbindung nicht mehr vorhanden".into());
    }
    let mut c = find_raw(path, id)?;
    // Gespeicherte Zugangsdaten: Benutzername von dort (Geheimnisse über login_secret).
    if let Some(cid) = &c.credential_id {
        let cred = creds::load(&creds_path(path))?.into_iter().find(|x| &x.id == cid).ok_or("Zugewiesene Zugangsdaten gibt es nicht mehr")?;
        c.user = cred.user;
    }
    Ok(c)
}

/// Benutzer und Passwort gespeicherter Zugangsdaten (für RDP, VNC, Gateway).
fn cred_login(path: &LibraryPath, cid: &str) -> Result<(String, Option<String>), String> {
    let cred = creds::load(&creds_path(path))?.into_iter().find(|x| x.id == cid).ok_or("Zugewiesene Zugangsdaten gibt es nicht mehr")?;
    let pw = vault::get(&creds::vault_id(cid), Kind::Password)?;
    if let Some(p) = &pw {
        diag::add_secret(p);
    }
    Ok((cred.user, pw))
}

fn creds_path(path: &LibraryPath) -> PathBuf {
    path.0.with_file_name("credentials.json")
}

#[tauri::command]
fn creds_list(path: State<LibraryPath>) -> Result<Vec<(creds::Cred, usize)>, String> {
    let lib = library::load(&path.0)?;
    Ok(creds::load(&creds_path(&path))?
        .into_iter()
        .map(|c| {
            let uses = lib.iter().filter(|x| x.cred_ids().contains(&Some(c.id.as_str()))).count();
            (c, uses)
        })
        .collect())
}

/// Anlegen/ändern. `password`/`ssh_key`: None = unverändert, "" = löschen.
#[tauri::command]
fn cred_save(path: State<LibraryPath>, mut cred: creds::Cred, password: Option<String>, ssh_key: Option<String>) -> Result<creds::Cred, String> {
    creds::validate(&cred)?;
    if let Some(k) = ssh_key.as_deref().filter(|k| !k.trim().is_empty()) {
        ssh::check_private_key(k)?;
    }
    let _g = LIBRARY_LOCK.lock().unwrap();
    let p = creds_path(&path);
    let mut all = creds::load(&p)?;
    let old = all.iter().find(|x| x.id == cred.id).cloned();
    cred.has_password = old.as_ref().is_some_and(|o| o.has_password);
    cred.has_key = old.as_ref().is_some_and(|o| o.has_key);
    let vid = creds::vault_id(&cred.id);
    for (secret, kind, flag) in [(password, Kind::Password, &mut cred.has_password), (ssh_key, Kind::SshKey, &mut cred.has_key)] {
        match secret.as_deref() {
            None => {}
            Some("") => {
                vault::delete(&vid, kind)?;
                *flag = false;
            }
            Some(s) => {
                vault::set(&vid, kind, s)?;
                *flag = true;
            }
        }
    }
    match all.iter_mut().find(|x| x.id == cred.id) {
        Some(x) => *x = cred.clone(),
        None => all.push(cred.clone()),
    }
    creds::save(&p, &all)?;
    Ok(cred)
}

/// Löschen; Verbindungen, die es nutzen, fallen auf «Eigene» zurück.
#[tauri::command]
fn cred_delete(path: State<LibraryPath>, id: String) -> Result<(), String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let p = creds_path(&path);
    let mut all = creds::load(&p)?;
    all.retain(|x| x.id != id);
    creds::save(&p, &all)?;
    let vid = creds::vault_id(&id);
    vault::delete(&vid, Kind::Password)?;
    vault::delete(&vid, Kind::SshKey)?;
    let mut lib = library::load(&path.0)?;
    for c in lib.iter_mut() {
        for slot in [&mut c.credential_id, &mut c.rdp_credential_id, &mut c.vnc_credential_id, &mut c.gateway_credential_id] {
            if slot.as_deref() == Some(id.as_str()) {
                *slot = None;
            }
        }
    }
    library::save(&path.0, &lib)
}

/// Bestehende Zugangsdaten einer Verbindung als gespeichertes Profil übernehmen und zuweisen.
#[tauri::command]
fn cred_from_connection(path: State<LibraryPath>, id: String, cred_id: String, name: String) -> Result<creds::Cred, String> {
    let _g = LIBRARY_LOCK.lock().unwrap();
    let mut lib = library::load(&path.0)?;
    let conn = lib.iter_mut().find(|c| c.id == id).ok_or("Verbindung nicht gefunden")?;
    let mut cred = creds::Cred { id: cred_id, name, user: conn.user.clone(), ..Default::default() };
    creds::validate(&cred)?;
    let vid = creds::vault_id(&cred.id);
    if let Some(pw) = vault::get(&conn.id, Kind::Password)? {
        vault::set(&vid, Kind::Password, &pw)?;
        cred.has_password = true;
    }
    if let Some(k) = vault::get(&conn.id, Kind::SshKey)? {
        vault::set(&vid, Kind::SshKey, &k)?;
        cred.has_key = true;
    }
    conn.credential_id = Some(cred.id.clone());
    let p = creds_path(&path);
    let mut all = creds::load(&p)?;
    all.push(cred.clone());
    creds::save(&p, &all)?;
    library::save(&path.0, &lib)?;
    Ok(cred)
}

fn find_raw(path: &LibraryPath, id: &str) -> Result<Connection, String> {
    library::load(&path.0)?.into_iter().find(|c| c.id == id).ok_or_else(|| "Verbindung nicht gefunden".into())
}

/// Passwort/SSH-Schlüssel: aus den gespeicherten Zugangsdaten, wenn zugewiesen, sonst aus der Verbindung.
fn login_secret(c: &Connection, kind: Kind) -> Result<Option<String>, String> {
    // Schnelle Verbindung: hier eingetippt/gewählt geht vor gespeicherten Zugangsdaten.
    if c.id.starts_with("quick-") {
        if let Some(s) = vault::get(&c.id, kind)? {
            return Ok(Some(s));
        }
    }
    match (&c.credential_id, kind) {
        (Some(cid), Kind::Password | Kind::SshKey) => vault::get(&creds::vault_id(cid), kind),
        _ => vault::get(&c.id, kind),
    }
}

/// Pfad der Bibliothek für Stellen ohne Tauri-State (Sprungserver nachschlagen).
static LIBRARY_FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn ssh_target(c: &Connection, trust: Option<String>) -> Result<ssh::Target, String> {
    let mut t = ssh_target_direct(c, trust)?;
    if !c.jump_host.is_empty() {
        let path = LIBRARY_FILE.get().ok_or("Bibliothek nicht geladen")?;
        let j = find(&LibraryPath(path.clone()), &c.jump_host).map_err(|_| "Sprungserver nicht mehr in der Bibliothek".to_string())?;
        // Nur eine Stufe: der Sprungserver selbst verbindet direkt.
        t.jump = Some(Box::new(ssh_target_direct(&j, None)?));
    }
    Ok(t)
}

fn ssh_target_direct(c: &Connection, trust: Option<String>) -> Result<ssh::Target, String> {
    Ok(ssh::Target {
        host: c.address.clone(),
        port: c.ssh_port.ok_or_else(|| format!("SSH ist für «{}» ausgeschaltet", c.name))?,
        user: c.user.clone(),
        password: login_secret(c, Kind::Password)?,
        key: login_secret(c, Kind::SshKey)?,
        trusted: trust,
        opts: ssh::Options {
            legacy: c.ssh_legacy,
            keepalive: c.ssh_keepalive,
            compression: c.ssh_compression,
            agent: c.ssh_agent,
            agent_forward: c.agent_forward,
        },
        forwards: Vec::new(),
        jump: None,
    })
}

#[tauri::command]
async fn shell_open(
    path: State<'_, LibraryPath>,
    shells: State<'_, ssh::Shells>,
    id: String,
    cols: u32,
    rows: u32,
    trust: Option<String>,
    on_event: Channel<ssh::ShellEvent>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let mut t = ssh_target(&c, trust)?;
    t.forwards = c.forwards.iter().map(|l| forward::parse(l)).collect::<Result<_, _>>()?;
    let relay = relay::Relay::new(on_event);
    let shell = ssh::open(t, cols, rows, relay.clone()).await?;
    let sid = new_sid(&c.id);
    shells.0.lock().await.insert(sid.clone(), shell);
    relay::SHELLS.lock().unwrap().insert(sid.clone(), relay);
    Ok(sid)
}

#[tauri::command]
async fn shell_write(shells: State<'_, ssh::Shells>, sid: String, data: Vec<u8>) -> Result<(), String> {
    shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.write(data).await
}

#[tauri::command]
async fn shell_resize(shells: State<'_, ssh::Shells>, sid: String, cols: u32, rows: u32) -> Result<(), String> {
    shells.0.lock().await.get(&sid).ok_or("Shell geschlossen")?.resize(cols, rows).await
}

#[tauri::command]
async fn shell_close(shells: State<'_, ssh::Shells>, sid: String) -> Result<(), String> {
    relay::SHELLS.lock().unwrap().remove(&sid);
    if let Some(s) = shells.0.lock().await.remove(&sid) {
        s.close().await;
    }
    Ok(())
}

// ---------- Dateien (SFTP, sonst SCP) ----------

#[derive(Clone)]
enum FileSession {
    Sftp(Arc<sftp::Sftp>),
    Scp(Arc<scp::Scp>),
}

#[derive(Default)]
struct Files(Mutex<HashMap<String, FileSession>>);

#[derive(Default)]
struct Transfers(Arc<Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>>);

fn files_get(files: &Files, sid: &str) -> Result<FileSession, String> {
    files.0.lock().unwrap().get(sid).cloned().ok_or_else(|| "Dateisitzung geschlossen".into())
}

#[derive(serde::Serialize)]
struct FilesOpened {
    sid: String,
    /// `true` = Server ohne SFTP, Übertragung über SCP (eingeschränkt).
    scp: bool,
}

#[tauri::command]
async fn sftp_open(path: State<'_, LibraryPath>, files: State<'_, Files>, id: String, trust: Option<String>) -> Result<FilesOpened, String> {
    let c = find(&path, &id)?;
    let h = ssh::connect(&ssh_target(&c, trust)?).await?;
    let session = match c.file_transfer.as_str() {
        "scp" => FileSession::Scp(Arc::new(scp::Scp::new(h))),
        mode => match sftp::start(h).await {
            Ok(s) => FileSession::Sftp(Arc::new(s)),
            Err((_, e)) if mode == "sftp" => return Err(e),
            Err((h, e)) => {
                tracing::info!("SFTP nicht verfügbar ({e}), weiter mit SCP");
                FileSession::Scp(Arc::new(scp::Scp::new(h)))
            }
        },
    };
    let sid = new_sid(&c.id);
    let scp = matches!(session, FileSession::Scp(_));
    files.0.lock().unwrap().insert(sid.clone(), session);
    Ok(FilesOpened { sid, scp })
}

#[tauri::command]
async fn sftp_list(files: State<'_, Files>, sid: String, path: String) -> Result<sftp::Listing, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.list(&path).await,
        FileSession::Scp(s) => s.list(&path).await,
    }
}

#[tauri::command]
async fn sftp_mkdir(files: State<'_, Files>, sid: String, dir: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.mkdir(&dir, &name).await,
        FileSession::Scp(s) => s.mkdir(&dir, &name).await,
    }
}

#[tauri::command]
async fn sftp_rename(files: State<'_, Files>, sid: String, path: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.rename(&path, &name).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

#[tauri::command]
async fn sftp_remove(files: State<'_, Files>, sid: String, path: String) -> Result<(), String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.remove(&path).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

#[tauri::command]
async fn sftp_chmod(files: State<'_, Files>, sid: String, path: String, mode: u32) -> Result<(), String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.chmod(&path, mode).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

#[tauri::command]
async fn sftp_touch(files: State<'_, Files>, sid: String, dir: String, name: String) -> Result<String, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.touch(&dir, &name).await,
        FileSession::Scp(_) => Err(scp::LIMITED.into()),
    }
}

/// Prüfsumme einer Datei auf dem Server (für «Inhalt prüfen» im Vergleich).
#[tauri::command]
async fn sftp_sha256(files: State<'_, Files>, sid: String, path: String) -> Result<String, String> {
    match files_get(&files, &sid)? {
        FileSession::Sftp(s) => s.sha256(&path).await,
        FileSession::Scp(s) => s.sha256(&path).await,
    }
}

#[tauri::command]
async fn local_sha256(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || sftp::local_sha256(&path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn local_mkdir(dir: String, name: String) -> Result<String, String> {
    sftp::local_mkdir(&dir, &name)
}

#[tauri::command]
fn local_rename(path: String, name: String) -> Result<String, String> {
    sftp::local_rename(&path, &name)
}

/// Laufende Desktop-Sitzung (VNC/RDP) an das Fenster dieses Kanals hängen.
#[tauri::command]
fn desktop_attach(sid: String, on_event: Channel<InvokeResponseBody>) -> Result<(), String> {
    relay::DESKTOPS.lock().unwrap().get(&sid).ok_or("Desktop-Sitzung geschlossen")?.swap(on_event);
    Ok(())
}

/// Laufende Shell an das Fenster dieses Kanals hängen.
#[tauri::command]
fn shell_attach(sid: String, on_event: Channel<ssh::ShellEvent>) -> Result<(), String> {
    relay::SHELLS.lock().unwrap().get(&sid).ok_or("Shell geschlossen")?.swap(on_event);
    Ok(())
}

/// Sitzung in ein eigenes Fenster abkoppeln; `token` verweist auf die Übergabe im localStorage.
/// Asynchron: ein Fenster in einem synchronen Befehl zu erzeugen, blockiert unter Windows (leeres Fenster, X reagiert nicht).
#[tauri::command]
async fn open_detached(app: AppHandle, token: String, title: String) -> Result<(), String> {
    if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Ungültige Übergabe".into());
    }
    let builder = tauri::WebviewWindowBuilder::new(&app, format!("detached-{token}"), tauri::WebviewUrl::App("index.html".into()))
        .initialization_script(format!("window.__SESAM_DETACH__ = '{token}';"))
        .title(title)
        .inner_size(1200.0, 800.0)
        .min_inner_size(480.0, 320.0)
        .decorations(cfg!(target_os = "macos"))
        .shadow(true);
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    builder
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn sftp_close(files: State<Files>, sid: String) {
    files.0.lock().unwrap().remove(&sid);
}

/// Setup-Script für GNOME Remote Desktop, unverändert eingebettet (LF, ohne BOM).
const SETUP_SCRIPT: &[u8] = include_bytes!("../resources/sesam-rdp-setup.sh");

/// Script-Inhalt und SHA-256 (zum Herunterladen und als Versionsangabe).
#[tauri::command]
fn setup_script() -> (Vec<u8>, String) {
    use sha2::Digest;
    (SETUP_SCRIPT.to_vec(), sha2::Sha256::digest(SETUP_SCRIPT).iter().map(|b| format!("{b:02x}")).collect())
}

/// Script per SFTP nach `~/sesam-rdp-setup.sh` hochladen (Bytes 1:1). Ausführen tut der Benutzer selbst in der Shell.
#[tauri::command]
async fn setup_upload(path: State<'_, LibraryPath>, id: String, trust: Option<String>) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let c = find(&path, &id)?;
    let s = sftp::open(ssh_target(&c, trust)?).await?;
    let home = s.s.canonicalize(".").await.map_err(|e| e.to_string())?;
    let target = sftp::rjoin(&home, "sesam-rdp-setup.sh");
    let mut f = s.s.create(&target).await.map_err(|e| format!("{target}: {e}"))?;
    f.write_all(SETUP_SCRIPT).await.map_err(|e| e.to_string())?;
    f.shutdown().await.map_err(|e| e.to_string())?;
    Ok(target)
}

fn thumb_path(path: &LibraryPath, id: &str) -> Result<PathBuf, String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || id.is_empty() {
        return Err("Ungültige ID".into());
    }
    Ok(path.0.with_file_name("thumbs").join(format!("{id}.jpg")))
}

/// Vorschaubild speichern (JPEG vom Frontend: letzter Desktop beim Trennen).
#[tauri::command]
fn thumb_save(path: State<LibraryPath>, id: String, data: Vec<u8>) -> Result<(), String> {
    if id.starts_with("quick-") {
        return Ok(()); // schnelle Verbindungen hinterlassen nichts
    }
    let p = thumb_path(&path, &id)?;
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())
}

#[tauri::command]
fn thumb_get(path: State<LibraryPath>, id: String) -> Option<Vec<u8>> {
    std::fs::read(thumb_path(&path, &id).ok()?).ok()
}

/// Eigenes Bild übernehmen (vom Benutzer im Dialog gewählt).
#[tauri::command]
fn thumb_import(path: State<LibraryPath>, id: String, file: String) -> Result<(), String> {
    let data = std::fs::read(&file).map_err(|e| format!("{file}: {e}"))?;
    if data.len() > 8 << 20 {
        return Err("Bild zu gross (höchstens 8 MB)".into());
    }
    let p = thumb_path(&path, &id)?;
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())
}

/// Verknüpfung auf dem Schreibtisch: öffnet `sesam://open?id=…&view=…`.
/// Windows: .url mit dem Sesam-Symbol, macOS: .webloc.
#[tauri::command]
fn create_shortcut(app: AppHandle, path: State<LibraryPath>, id: String, view: String) -> Result<String, String> {
    let c = find(&path, &id)?;
    if !["desktop", "shell", "files"].contains(&view.as_str()) {
        return Err("Unbekannte Ansicht".into());
    }
    let label = match view.as_str() { "desktop" => "Desktop", "shell" => "Shell", _ => "Dateien" };
    let safe: String = c.name.chars().map(|ch| if r#"\/:*?"<>|"#.contains(ch) || ch.is_control() { '_' } else { ch }).collect();
    let url = format!("sesam://open?id={}&view={view}", c.id);
    let desk = app.path().desktop_dir().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let (file, body) = (
        desk.join(format!("{safe} ({label}).webloc")),
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>URL</key><string>{url}</string></dict></plist>\n"),
    );
    #[cfg(not(target_os = "macos"))]
    let (file, body) = {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        (desk.join(format!("{safe} ({label}).url")), format!("[InternetShortcut]\r\nURL={url}\r\nIconFile={exe}\r\nIconIndex=0\r\n"))
    };
    std::fs::write(&file, body).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file.display().to_string())
}

/// Dateien aufs Desktop-Fenster gezogen: per SFTP nach ~/Sesam-Ablage (wenn SSH an ist),
/// sonst in den per RDP freigegebenen Ordner kopieren. Gibt eine Meldung für die Statuszeile zurück.
#[tauri::command]
async fn drop_files(path: State<'_, LibraryPath>, id: String, files: Vec<String>) -> Result<String, String> {
    let c = find(&path, &id)?;
    let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
    if c.ssh_port.is_some() {
        let share = ClipShare::new(c.clone(), std::env::temp_dir());
        let done = share.upload_files(&files).await?;
        return Ok(format!("{} Element(e) nach ~/Sesam-Ablage hochgeladen.", done.len()));
    }
    let dir = c.shared_folder.trim();
    if c.rdp_port.is_some() && !dir.is_empty() {
        let dir = PathBuf::from(dir);
        for f in &files {
            let name = f.file_name().ok_or("Ungültige Datei")?;
            let mut target = dir.join(name);
            let mut n = 2;
            while target.exists() {
                target = dir.join(format!("{} ({n})", name.to_string_lossy()));
                n += 1;
            }
            if f.is_dir() {
                return Err("Ordner lassen sich nur mit SSH übertragen.".into());
            }
            std::fs::copy(f, &target).map_err(|e| format!("{}: {e}", f.display()))?;
        }
        return Ok(format!("{} Datei(en) in den freigegebenen Ordner kopiert (auf dem Server als Laufwerk sichtbar).", files.len()));
    }
    Err("Für Dateien SSH einschalten oder unter RDP einen Ordner freigeben.".into())
}

/// Gemerkte TLS-Zertifikate (RDP/VNC) für die Einstellungen.
#[tauri::command]
fn certs_list(path: State<LibraryPath>) -> Vec<(String, String)> {
    let mut v: Vec<_> = load_pins(&path.0.with_file_name("known_certs.json")).into_iter().collect();
    v.sort();
    v
}

#[tauri::command]
fn cert_remove(path: State<LibraryPath>, host: String) -> Result<(), String> {
    let p = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&p);
    pins.remove(&host);
    std::fs::write(&p, serde_json::to_string_pretty(&pins).unwrap_or_default()).map_err(|e| e.to_string())
}

/// Vom Benutzer im Speichern-Dialog gewählte Datei schreiben (z. B. Bildschirmfoto).
#[tauri::command]
fn save_bytes(path: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, data).map_err(|e| format!("{path}: {e}"))
}

#[tauri::command]
fn local_list(path: String) -> Result<sftp::Listing, String> {
    sftp::local_list(&path)
}

#[tauri::command]
fn local_home(app: AppHandle) -> Result<String, String> {
    app.path().home_dir().map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string())
}

/// Startet eine Übertragung im Hintergrund; Fortschritt über `on_progress`. Gibt die Übertragungs-ID zurück.
#[tauri::command]
fn transfer_start(
    files: State<Files>,
    transfers: State<Transfers>,
    sid: String,
    upload: bool,
    src: String,
    dest_dir: String,
    replace: Option<bool>,
    on_progress: Channel<sftp::Progress>,
) -> Result<String, String> {
    let s = files_get(&files, &sid)?;
    let tid = new_sid("t");
    let (map, key) = (transfers.0.clone(), tid.clone());
    let mut running = transfers.0.lock().unwrap();
    let task = tauri::async_runtime::spawn(async move {
        let ch = on_progress.clone();
        let report = move |p| {
            let _ = ch.send(p);
        };
        let (local, remote) = if upload { (&src, &dest_dir) } else { (&dest_dir, &src) };
        let local = std::path::Path::new(local);
        let replace = replace.unwrap_or(false);
        let r = match (&s, upload) {
            (FileSession::Sftp(s), true) => sftp::upload(s, local, remote, replace, report).await,
            (FileSession::Sftp(s), false) => sftp::download(s, remote, local, replace, report).await,
            (FileSession::Scp(s), true) => scp::upload(s, local, remote, replace, report).await,
            (FileSession::Scp(s), false) => scp::download(s, remote, local, replace, report).await,
        };
        if let Err(error) = r {
            let _ = on_progress.send(sftp::Progress::Failed { error });
        }
        map.lock().unwrap().remove(&key);
    });
    running.insert(tid.clone(), task);
    Ok(tid)
}

#[tauri::command]
fn transfer_cancel(transfers: State<Transfers>, tid: String) {
    if let Some(t) = transfers.0.lock().unwrap().remove(&tid) {
        t.abort();
    }
}

// ---------- VNC ----------

struct Desktop {
    session: Arc<vnc::Session>,
    share: Arc<ClipShare>,
    events: relay::Relay<InvokeResponseBody>,
}

#[derive(Default)]
struct Desktops(Mutex<HashMap<String, Desktop>>);

/// Gepinnte TLS-Zertifikate der VNC-Server: "host:port" → SHA256.
fn load_pins(path: &std::path::Path) -> HashMap<String, String> {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Binärrahmen ans Frontend (Big Endian): 0 Rechteck, 1 Grösse, 3 Ende, 4 Glocke, 5 Hinweis, 6 Mauszeiger.
fn frame(kind: u8, nums: &[u16], tail: &[u8]) -> InvokeResponseBody {
    let mut b = Vec::with_capacity(1 + nums.len() * 2 + tail.len());
    b.push(kind);
    nums.iter().for_each(|n| b.extend(n.to_be_bytes()));
    b.extend_from_slice(tail);
    InvokeResponseBody::Raw(b)
}

#[tauri::command]
async fn vnc_open(
    app: AppHandle,
    path: State<'_, LibraryPath>,
    desktops: State<'_, Desktops>,
    id: String,
    trust: Option<String>,
    remember: Option<bool>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let port = c.vnc_port.ok_or("Desktop (VNC) ist für diese Verbindung ausgeschaltet")?;
    let password = login_secret(&c, Kind::Password)?;
    let vnc_password = match &c.vnc_credential_id {
        Some(cid) => cred_login(&path, cid)?.1,
        None => vault::get(&c.id, Kind::VncPassword)?,
    };
    let pins_path = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let cert = if c.skip_cert_check {
        vnc::tls::Pinned::accept_any()
    } else {
        vnc::tls::Pinned::new(trust.clone().or_else(|| pins.get(&pin_key).cloned()))
    };
    let login = vnc::Login { host: &c.address, user: &c.user, password: password.as_deref(), vnc_password: vnc_password.as_deref(), cert };
    let timeout = std::time::Duration::from_secs(15);
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect((c.address.as_str(), port)))
        .await
        .map_err(|_| "Zeitüberschreitung beim Verbinden".to_string())?
        .map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
    let _ = stream.set_nodelay(true);
    let (stream, init) = tokio::time::timeout(timeout, vnc::handshake(stream, &login))
        .await
        .map_err(|_| "Zeitüberschreitung bei der Anmeldung".to_string())??;
    // Bestätigtes Zertifikat merken, aber nur wenn TLS es wirklich gesehen hat.
    if let Some(fp) = trust.filter(|fp| remember.unwrap_or(true) && login.cert.seen.lock().unwrap().as_ref() == Some(fp)) {
        pins.insert(pin_key, fp);
        let _ = std::fs::write(&pins_path, serde_json::to_string_pretty(&pins).unwrap_or_default());
    }

    let downloads = app.path().download_dir().unwrap_or_else(|_| std::env::temp_dir());
    let share = Arc::new(ClipShare::new(c.clone(), downloads));
    let on_event = relay::Relay::new(on_event);
    let (sh, events) = (share.clone(), on_event.clone());
    let session = vnc::Session::start(stream, init, move |e| {
        let f = match e {
            vnc::Event::Rect { rect: r, rgba } => frame(0, &[r.x, r.y, r.w, r.h], &rgba),
            vnc::Event::Resize { width, height } => frame(1, &[width, height], &[]),
            vnc::Event::Closed(why) => frame(3, &[], why.as_bytes()),
            vnc::Event::Bell => frame(4, &[], &[]),
            vnc::Event::Info(t) => frame(5, &[], t.as_bytes()),
            vnc::Event::Cursor { hot_x, hot_y, w, h, rgba } => frame(6, &[hot_x, hot_y, w, h], &rgba),
            vnc::Event::Clipboard(t) => {
                if !sh.enabled {
                    return true;
                }
                if let Some(paths) = clipshare::file_uris(&t).filter(|_| sh.has_ssh()) {
                    let (sh, ev) = (sh.clone(), on_event.clone());
                    tauri::async_runtime::spawn(async move {
                        let msg = match sh.download_to_clipboard(&paths).await {
                            Ok(n) => format!("{n} Datei(en) vom Server liegen in der Zwischenablage ({})", sh.download_dir().join("Sesam-Ablage").display()),
                            Err(e) => e,
                        };
                        let _ = ev.send(frame(5, &[], msg.as_bytes()));
                    });
                    return true;
                }
                clipshare::write_local_text(&t);
                *sh.last.lock().unwrap() = Last::Text(t);
                return true;
            }
        };
        on_event.send(f).is_ok()
    });
    let sid = new_sid(&c.id);
    relay::DESKTOPS.lock().unwrap().insert(sid.clone(), events.clone());
    desktops.0.lock().unwrap().insert(sid.clone(), Desktop { session: Arc::new(session), share, events });
    Ok(sid)
}

fn with_desktop<T>(d: &Desktops, sid: &str, f: impl FnOnce(&Desktop) -> T) -> Result<T, String> {
    d.0.lock().unwrap().get(sid).map(f).ok_or_else(|| "Desktop-Sitzung geschlossen".into())
}

#[tauri::command]
fn vnc_pointer(desktops: State<Desktops>, sid: String, mask: u8, x: u16, y: u16) -> Result<(), String> {
    with_desktop(&desktops, &sid, |d| d.session.send(vnc::pointer(mask, x, y)))
}

#[tauri::command]
fn vnc_key(desktops: State<Desktops>, sid: String, down: bool, keysym: u32) -> Result<(), String> {
    with_desktop(&desktops, &sid, |d| d.session.send(vnc::key(down, keysym)))
}

/// `false`, wenn der Server keine Grössenänderung kann.
#[tauri::command]
fn vnc_set_size(desktops: State<Desktops>, sid: String, width: u16, height: u16) -> Result<bool, String> {
    with_desktop(&desktops, &sid, |d| d.session.set_size(width, height))
}

/// Lokale Zwischenablage an den Server geben, falls sie sich geändert hat (beim Fokussieren aufgerufen).
/// Text direkt über VNC; Dateien und Bilder per SFTP nach `~/Sesam-Ablage`, dazu deren Pfade als Text.
#[tauri::command]
async fn vnc_clipboard_sync(desktops: State<'_, Desktops>, sid: String) -> Result<(), String> {
    let (session, share, events) = with_desktop(&desktops, &sid, |d| (d.session.clone(), d.share.clone(), d.events.clone()))?;
    if !share.enabled {
        return Ok(());
    }
    let info = |m: &str| {
        let _ = events.send(frame(5, &[], m.as_bytes()));
    };
    // `true`, wenn neu (und merkt es sich).
    let fresh = |next: Last| {
        let mut last = share.last.lock().unwrap();
        let new = *last != next;
        *last = next;
        new
    };
    let needs_ssh = "Dateien und Bilder über die Zwischenablage brauchen SSH (in der Verbindung einschalten).";
    match clipshare::read_local() {
        Local::Nothing => {}
        Local::Text(t) => {
            if fresh(Last::Text(t.clone())) {
                session.clipboard(&t);
            }
        }
        Local::Files(files) => {
            if !fresh(Last::Files(files.clone())) {
                return Ok(());
            }
            if !share.has_ssh() {
                info(needs_ssh);
                return Ok(());
            }
            info(&format!("Kopiere {} Element(e) nach ~/Sesam-Ablage …", files.len()));
            match share.upload_files(&files).await {
                Ok(paths) => {
                    session.clipboard(&paths.join("\n"));
                    info(&format!("{} Element(e) in ~/Sesam-Ablage; der Pfad liegt in der Zwischenablage des Servers.", paths.len()));
                }
                Err(e) => info(&e),
            }
        }
        Local::Image { width, height, rgba } => {
            if !fresh(Last::Image(clipshare::hash(&rgba))) {
                return Ok(());
            }
            if !share.has_ssh() {
                info(needs_ssh);
                return Ok(());
            }
            let png = clipshare::encode_png(width, height, &rgba)?;
            match share.upload_png(&png).await {
                Ok(path) => {
                    session.clipboard(&path);
                    info(&format!("Bild als {path} abgelegt; der Pfad liegt in der Zwischenablage des Servers."));
                }
                Err(e) => info(&e),
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn vnc_close(desktops: State<Desktops>, sid: String) {
    relay::DESKTOPS.lock().unwrap().remove(&sid);
    if let Some(d) = desktops.0.lock().unwrap().remove(&sid) {
        d.session.close();
    }
}

// ---------- RDP ----------

#[derive(Default)]
struct RdpSessions(Mutex<HashMap<String, rdp::Session>>);

// Das Windows-Zwischenablage-Backend braucht ein Fenster auf einem Thread mit Nachrichtenschleife:
// es entsteht und lebt auf dem Tauri-Hauptthread, je RDP-Sitzung eines.
#[cfg(windows)]
thread_local! {
    static WIN_CLIPS: std::cell::RefCell<HashMap<String, ironrdp_cliprdr_native::WinClipboard>> = Default::default();
}

#[cfg(windows)]
async fn rdp_clipboard(app: &AppHandle, key: String, tx: tokio::sync::mpsc::UnboundedSender<rdp::Input>) -> Option<rdp::ClipFactory> {
    let (otx, orx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let made = ironrdp_cliprdr_native::WinClipboard::with_file_transfer(rdp::ClipProxy(tx), true).ok().map(|c| {
            let f = c.backend_factory();
            WIN_CLIPS.with(|m| m.borrow_mut().insert(key, c));
            f
        });
        let _ = otx.send(made);
    })
    .ok()?;
    orx.await.ok().flatten().map(|f| Arc::new(Mutex::new(f)))
}

// shortcut: Zwischenablage über RDP nur unter Windows; für macOS fehlt ein natives cliprdr-Backend.
#[cfg(not(windows))]
async fn rdp_clipboard(_: &AppHandle, _: String, _: tokio::sync::mpsc::UnboundedSender<rdp::Input>) -> Option<rdp::ClipFactory> {
    None
}

fn rdp_clipboard_drop(app: &AppHandle, key: String) {
    #[cfg(windows)]
    let _ = app.run_on_main_thread(move || {
        WIN_CLIPS.with(|m| m.borrow_mut().remove(&key));
    });
    #[cfg(not(windows))]
    let _ = (app, key);
}

fn screen_frame(e: vnc::Event) -> Option<InvokeResponseBody> {
    Some(match e {
        vnc::Event::Rect { rect: r, rgba } => frame(0, &[r.x, r.y, r.w, r.h], &rgba),
        vnc::Event::Resize { width, height } => frame(1, &[width, height], &[]),
        vnc::Event::Closed(why) => frame(3, &[], why.as_bytes()),
        vnc::Event::Bell => frame(4, &[], &[]),
        vnc::Event::Info(t) => frame(5, &[], t.as_bytes()),
        vnc::Event::Cursor { hot_x, hot_y, w, h, rgba } => frame(6, &[hot_x, hot_y, w, h], &rgba),
        vnc::Event::Clipboard(_) => return None,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn rdp_open(
    app: AppHandle,
    path: State<'_, LibraryPath>,
    sessions: State<'_, RdpSessions>,
    id: String,
    trust: Option<String>,
    remember: Option<bool>,
    width: u16,
    height: u16,
    on_event: Channel<InvokeResponseBody>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let port = c.rdp_port.ok_or("Desktop über RDP ist für diese Verbindung ausgeschaltet")?;
    // «Eigene Zugangsdaten für RDP»: gespeicherte, eigener Benutzer/Passwort, sonst die der Verbindung.
    let (user, password) = match &c.rdp_credential_id {
        Some(cid) => {
            let (u, p) = cred_login(&path, cid)?;
            (u, p.unwrap_or_default())
        }
        None => (
            match c.rdp_user.trim() {
                "" => c.user.clone(),
                u => u.to_string(),
            },
            vault::get(&c.id, Kind::RdpPassword)?.or(login_secret(&c, Kind::Password)?).unwrap_or_default(),
        ),
    };
    let domain = (!c.rdp_domain.trim().is_empty()).then(|| c.rdp_domain.trim().to_string());
    let gateway = match c.rdp_gateway.trim() {
        "" => None,
        gw if c.gateway_credential_id.is_some() => {
            let (u, p) = cred_login(&path, c.gateway_credential_id.as_deref().unwrap_or_default())?;
            Some(rdp::Gateway { endpoint: gw.to_string(), user: u, password: p.unwrap_or_default() })
        }
        gw if c.rdp_gateway_user.trim().is_empty() => Some(rdp::Gateway {
            endpoint: gw.to_string(),
            // «Gleiche wie Verbindung»: Domäne wie bei mstsc vor den Benutzer.
            user: match &domain {
                Some(d) if !user.contains(['@', '\\']) => format!("{d}\\{}", user),
                _ => user.clone(),
            },
            password: password.clone(),
        }),
        gw => {
            let pw = vault::get(&c.id, Kind::GatewayPassword)?.unwrap_or_default();
            diag::add_secret(&pw);
            Some(rdp::Gateway { endpoint: gw.to_string(), user: c.rdp_gateway_user.trim().to_string(), password: pw })
        }
    };
    let pins_path = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let target = rdp::Target {
        host: c.address.clone(),
        port,
        user,
        domain,
        password,
        expected_cert: trust.clone().or_else(|| pins.get(&pin_key).cloned()),
        skip_cert_check: c.skip_cert_check,
        width: width.clamp(200, 8192),
        height: height.clamp(200, 8192),
        clipboard: None,
        audio: c.play_audio,
        share: (!c.shared_folder.trim().is_empty()).then(|| PathBuf::from(c.shared_folder.trim())),
        kdc: kdc::kdc_url(&c.rdp_kdc)?,
        no_ntlm: c.rdp_no_ntlm,
        gateway,
    };
    let sid = new_sid(&c.id);
    let (tx, rx) = rdp::channel();
    let mut target = target;
    if c.shared_clipboard {
        target.clipboard = rdp_clipboard(&app, sid.clone(), tx.clone()).await;
    }
    let seen: rdp::SeenCert = Default::default();
    let on_event = relay::Relay::new(on_event);
    relay::DESKTOPS.lock().unwrap().insert(sid.clone(), on_event.clone());
    let opened = rdp::open(target, seen.clone(), (tx, rx), move |e| screen_frame(e).is_none_or(|f| on_event.send(f).is_ok())).await;
    let session = match opened {
        Ok(s) => s,
        Err(e) => {
            relay::DESKTOPS.lock().unwrap().remove(&sid);
            rdp_clipboard_drop(&app, sid);
            return Err(e);
        }
    };
    if let Some(fp) = trust.filter(|fp| remember.unwrap_or(true) && seen.lock().unwrap().as_ref() == Some(fp)) {
        pins.insert(pin_key, fp);
        let _ = std::fs::write(&pins_path, serde_json::to_string_pretty(&pins).unwrap_or_default());
    }
    sessions.0.lock().unwrap().insert(sid.clone(), session);
    Ok(sid)
}

fn rdp_send(s: &RdpSessions, sid: &str, i: rdp::Input) -> Result<(), String> {
    s.0.lock().unwrap().get(sid).map(|x| x.send(i)).ok_or_else(|| "RDP-Sitzung geschlossen".into())
}

#[tauri::command]
fn rdp_pointer(sessions: State<RdpSessions>, sid: String, x: u16, y: u16, buttons: u8) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Pointer { x, y, buttons })
}

#[tauri::command]
fn rdp_wheel(sessions: State<RdpSessions>, sid: String, vertical: bool, units: i16) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Wheel { vertical, units })
}

/// Scancode oder Unicode je nach Taste (siehe `rdp::key_input`).
#[tauri::command]
fn rdp_key(sessions: State<RdpSessions>, sid: String, code: String, key: String, down: bool, altgr: bool) -> Result<(), String> {
    match rdp::key_input(&code, &key, down, altgr) {
        Some(i) => rdp_send(&sessions, &sid, i),
        None => Ok(()),
    }
}

#[tauri::command]
fn rdp_set_monitors(sessions: State<RdpSessions>, sid: String, monitors: Vec<rdp::Monitor>) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Monitors(monitors))
}

#[tauri::command]
fn rdp_release_all(sessions: State<RdpSessions>, sid: String) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::ReleaseAll)
}

#[tauri::command]
fn rdp_set_size(sessions: State<RdpSessions>, sid: String, width: u16, height: u16) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Resize { width, height })
}

#[tauri::command]
fn rdp_close(app: AppHandle, sessions: State<RdpSessions>, sid: String) {
    relay::DESKTOPS.lock().unwrap().remove(&sid);
    if let Some(s) = sessions.0.lock().unwrap().remove(&sid) {
        s.close();
    }
    rdp_clipboard_drop(&app, sid);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Zweiter Start (z. B. Klick auf eine Verknüpfung): Link an die laufende App, Fenster nach vorne.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            use tauri::menu::{MenuBuilder, SubmenuBuilder};
            // IDs wie in src/help.ts.
            use tauri::menu::MenuItemBuilder;
            let file = SubmenuBuilder::new(app, "Ablage")
                .item(&MenuItemBuilder::with_id("cmd:new", "Neue Verbindung …").accelerator("CmdOrCtrl+N").build(app)?)
                .item(&MenuItemBuilder::with_id("cmd:quick", "Schnelle Verbindung …").accelerator("CmdOrCtrl+K").build(app)?)
                .separator()
                .item(&MenuItemBuilder::with_id("cmd:settings", "Einstellungen …").accelerator("CmdOrCtrl+,").build(app)?)
                .build()?;
            // Gleiche Beschriftungen wie die Werkzeugknöpfe (src/tabs.ts runTool); Kürzel bleiben bei der Sitzung.
            let session = SubmenuBuilder::new(app, "Sitzung")
                .text("tool:Bibliothek", "Bibliothek anzeigen")
                .text("tool:Vorheriger Tab", "Vorheriger Tab")
                .text("tool:Nächster Tab", "Nächster Tab")
                .separator()
                .text("tool:Neue Shell", "Neue Shell")
                .text("tool:Suchen", "Suchen …")
                .text("tool:Zwischenablage abtippen", "Zwischenablage abtippen")
                .text("tool:Bildschirmfoto", "Bildschirmfoto")
                .text("tool:Alle Monitore", "Alle Monitore")
                .text("tool:Vollbild", "Vollbild")
                .separator()
                .text("tool:Abkoppeln", "In eigenem Fenster öffnen")
                .text("tool:Andocken", "Ins Hauptfenster zurück")
                .text("tool:Minimieren", "Minimieren")
                .text("tool:Trennen", "Trennen")
                .build()?;
            let help = SubmenuBuilder::new(app, "Hilfe")
                .text("help:zugangsdaten", "Welche Zugangsdaten?")
                .separator()
                .text("help:rdp-gnome", "RDP mit GNOME (Ubuntu)")
                .text("help:rdp-windows", "RDP mit Windows")
                .text("help:vnc", "VNC allgemein")
                .text("help:wayvnc", "wayvnc (wlroots)")
                .text("help:macos", "macOS-Bildschirmfreigabe")
                .text("help:ssh", "SSH und SFTP")
                .build()?;
            // macOS braucht das Standardmenü (Bearbeiten: Kopieren/Einfügen); Windows nur «Hilfe».
            // macOS: native Menüleiste oben am Bildschirm. Windows/Linux: eigene Titelleiste mit Menüknopf (src/titlebar.ts).
            if cfg!(target_os = "macos") {
                let m = tauri::menu::Menu::default(app.handle())?;
                m.insert(&file, 1)?;
                m.append(&session)?;
                m.append(&help)?;
                app.set_menu(m)?;
            } else {
                let _ = MenuBuilder::new(app).item(&file).item(&session).item(&help).build()?;
                if let Some(w) = app.get_webview_window("main") {
                    w.set_decorations(false)?;
                    let _ = w.set_shadow(true);
                }
            }
            app.on_menu_event(|app, e| {
                if let Some(topic) = e.id().as_ref().strip_prefix("help:") {
                    let _ = app.emit("help", topic);
                } else if let Some(cmd) = e.id().as_ref().strip_prefix("cmd:") {
                    let _ = app.emit("cmd", cmd);
                } else if let Some(tool) = e.id().as_ref().strip_prefix("tool:") {
                    let _ = app.emit("tool", tool);
                }
            });
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();
            }
            let dir = app.path().app_data_dir()?;
            diag::init(&dir);
            let _ = LIBRARY_FILE.set(dir.join("library.json"));
            app.manage(LibraryPath(dir.join("library.json")));
            app.manage(ssh::Shells::default());
            app.manage(Desktops::default());
            app.manage(Files::default());
            app.manage(RdpSessions::default());
            app.manage(Transfers::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_list,
            library_save,
            library_delete,
            shell_open,
            shell_write,
            shell_resize,
            shell_close,
            vnc_open,
            vnc_pointer,
            vnc_key,
            vnc_set_size,
            vnc_clipboard_sync,
            vnc_close,
            sftp_open,
            sftp_list,
            sftp_mkdir,
            sftp_rename,
            sftp_remove,
            sftp_close,
            desktop_attach,
            shell_attach,
            open_detached,
            sftp_chmod,
            sftp_touch,
            sftp_sha256,
            local_sha256,
            local_mkdir,
            local_rename,
            local_list,
            save_bytes,
            certs_list,
            quick_register,
            creds_list,
            cred_save,
            cred_delete,
            cred_from_connection,
            create_shortcut,
            drop_files,
            thumb_save,
            thumb_get,
            thumb_import,
            setup_script,
            setup_upload,
            cert_remove,
            local_home,
            transfer_start,
            transfer_cancel,
            rdp_open,
            rdp_pointer,
            rdp_wheel,
            rdp_key,
            rdp_release_all,
            rdp_set_monitors,
            rdp_set_size,
            rdp_close
        ])
        .run(tauri::generate_context!())
        .expect("Sesam Remote konnte nicht starten");
}

#[cfg(test)]
mod setup_script_tests {
    /// Das Setup-Script wird unverändert ausgeliefert (LF, ohne BOM); die Summe muss zur freigegebenen Fassung passen.
    #[test]
    fn setup_script_is_the_released_version() {
        let (bytes, sha) = super::setup_script();
        assert_eq!(sha, "0dd5318c82655441f520b955244d6b41e0038286799328ae9772adab397fa3ef");
        assert_eq!(bytes.len(), 16556);
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]) && !bytes.windows(2).any(|w| w == b"\r\n"));
    }
}
