mod library;
mod sftp;
mod ssh;
mod vault;
mod vnc;

use library::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
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
    connection.has_key = old.is_some_and(|c| c.has_key);
    for (secret, kind, flag) in [
        (password, Kind::Password, &mut connection.has_password),
        (vnc_password, Kind::VncPassword, &mut connection.has_vnc_password),
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
        library::save(&path.0, &items)?;
    }
    Ok(())
}

fn new_sid(id: &str) -> String {
    format!("{id}-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

fn find(path: &LibraryPath, id: &str) -> Result<Connection, String> {
    library::load(&path.0)?.into_iter().find(|c| c.id == id).ok_or_else(|| "Verbindung nicht gefunden".into())
}

fn ssh_target(c: &Connection, trust: Option<String>) -> Result<ssh::Target<'_>, String> {
    Ok(ssh::Target {
        host: &c.address,
        port: c.ssh_port.ok_or("SSH ist für diese Verbindung ausgeschaltet")?,
        user: &c.user,
        password: vault::get(&c.id, Kind::Password)?,
        key: vault::get(&c.id, Kind::SshKey)?,
        trusted: trust,
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
    let shell = ssh::open(ssh_target(&c, trust)?, cols, rows, on_event).await?;
    let sid = new_sid(&c.id);
    shells.0.lock().await.insert(sid.clone(), shell);
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
    if let Some(s) = shells.0.lock().await.remove(&sid) {
        s.close().await;
    }
    Ok(())
}

// ---------- Dateien (SFTP) ----------

#[derive(Default)]
struct Files(Mutex<HashMap<String, Arc<sftp::Sftp>>>);

#[derive(Default)]
struct Transfers(Arc<Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>>);

fn files_get(files: &Files, sid: &str) -> Result<Arc<sftp::Sftp>, String> {
    files.0.lock().unwrap().get(sid).cloned().ok_or_else(|| "Dateisitzung geschlossen".into())
}

#[tauri::command]
async fn sftp_open(path: State<'_, LibraryPath>, files: State<'_, Files>, id: String, trust: Option<String>) -> Result<String, String> {
    let c = find(&path, &id)?;
    let s = sftp::open(ssh_target(&c, trust)?).await?;
    let sid = new_sid(&c.id);
    files.0.lock().unwrap().insert(sid.clone(), Arc::new(s));
    Ok(sid)
}

#[tauri::command]
async fn sftp_list(files: State<'_, Files>, sid: String, path: String) -> Result<sftp::Listing, String> {
    files_get(&files, &sid)?.list(&path).await
}

#[tauri::command]
async fn sftp_mkdir(files: State<'_, Files>, sid: String, dir: String, name: String) -> Result<String, String> {
    files_get(&files, &sid)?.mkdir(&dir, &name).await
}

#[tauri::command]
fn sftp_close(files: State<Files>, sid: String) {
    files.0.lock().unwrap().remove(&sid);
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
        let r = if upload {
            sftp::upload(&s, std::path::Path::new(&src), &dest_dir, report).await
        } else {
            sftp::download(&s, &src, std::path::Path::new(&dest_dir), report).await
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
    // shortcut: eine abgebrochene Datei bleibt halb geschrieben liegen; bei Bedarf hier aufräumen.
    if let Some(t) = transfers.0.lock().unwrap().remove(&tid) {
        t.abort();
    }
}

// ---------- VNC ----------

struct Desktop {
    session: vnc::Session,
    shared_clipboard: bool,
    /// Zuletzt abgeglichener Text, damit nichts im Kreis läuft.
    last_clip: Arc<Mutex<String>>,
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
    on_event: Channel<InvokeResponseBody>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let port = c.vnc_port.ok_or("Desktop (VNC) ist für diese Verbindung ausgeschaltet")?;
    let password = vault::get(&c.id, Kind::Password)?;
    let vnc_password = vault::get(&c.id, Kind::VncPassword)?;
    let pins_path = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let cert = vnc::tls::Pinned::new(trust.clone().or_else(|| pins.get(&pin_key).cloned()));
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
    if let Some(fp) = trust.filter(|fp| login.cert.seen.lock().unwrap().as_ref() == Some(fp)) {
        pins.insert(pin_key, fp);
        let _ = std::fs::write(&pins_path, serde_json::to_string_pretty(&pins).unwrap_or_default());
    }

    let last_clip = Arc::new(Mutex::new(String::new()));
    let (clip, share) = (last_clip.clone(), c.shared_clipboard);
    let session = vnc::Session::start(stream, init, move |e| {
        let f = match e {
            vnc::Event::Rect { rect: r, rgba } => frame(0, &[r.x, r.y, r.w, r.h], &rgba),
            vnc::Event::Resize { width, height } => frame(1, &[width, height], &[]),
            vnc::Event::Closed(why) => frame(3, &[], why.as_bytes()),
            vnc::Event::Bell => frame(4, &[], &[]),
            vnc::Event::Info(t) => frame(5, &[], t.as_bytes()),
            vnc::Event::Cursor { hot_x, hot_y, w, h, rgba } => frame(6, &[hot_x, hot_y, w, h], &rgba),
            vnc::Event::Clipboard(t) => {
                if share && app.clipboard().write_text(t.clone()).is_ok() {
                    *clip.lock().unwrap() = t;
                }
                return true;
            }
        };
        on_event.send(f).is_ok()
    });
    let sid = new_sid(&c.id);
    desktops.0.lock().unwrap().insert(sid.clone(), Desktop { session, shared_clipboard: c.shared_clipboard, last_clip });
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
#[tauri::command]
fn vnc_clipboard_sync(app: AppHandle, desktops: State<Desktops>, sid: String) -> Result<(), String> {
    let Ok(text) = app.clipboard().read_text() else { return Ok(()) };
    with_desktop(&desktops, &sid, |d| {
        let mut last = d.last_clip.lock().unwrap();
        if d.shared_clipboard && *last != text {
            d.session.clipboard(&text);
            *last = text;
        }
    })
}

#[tauri::command]
fn vnc_close(desktops: State<Desktops>, sid: String) {
    if let Some(d) = desktops.0.lock().unwrap().remove(&sid) {
        d.session.close();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            app.manage(LibraryPath(dir.join("library.json")));
            app.manage(ssh::Shells::default());
            app.manage(Desktops::default());
            app.manage(Files::default());
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
            sftp_close,
            local_list,
            local_home,
            transfer_start,
            transfer_cancel
        ])
        .run(tauri::generate_context!())
        .expect("Sesam Remote konnte nicht starten");
}
