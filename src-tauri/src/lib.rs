mod clipshare;
mod diag;
mod library;
mod rdp;
mod rdp_redir;
mod rdstls;
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
    connection.has_key = old.is_some_and(|c| c.has_key);
    for (secret, kind, flag) in [
        (password, Kind::Password, &mut connection.has_password),
        (vnc_password, Kind::VncPassword, &mut connection.has_vnc_password),
        (rdp_password, Kind::RdpPassword, &mut connection.has_rdp_password),
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
fn quick_register(mut connection: Connection, password: Option<String>) -> Result<String, String> {
    connection.id = format!("quick-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    connection.validate()?;
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        diag::add_secret(&pw);
        vault::set_ephemeral(&connection.id, &pw);
        connection.has_password = true;
    }
    let id = connection.id.clone();
    QUICK.lock().unwrap().push(connection);
    Ok(id)
}

fn find(path: &LibraryPath, id: &str) -> Result<Connection, String> {
    if id.starts_with("quick-") {
        return QUICK.lock().unwrap().iter().find(|c| c.id == id).cloned().ok_or_else(|| "Schnelle Verbindung nicht mehr vorhanden".into());
    }
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
async fn sftp_rename(files: State<'_, Files>, sid: String, path: String, name: String) -> Result<String, String> {
    files_get(&files, &sid)?.rename(&path, &name).await
}

#[tauri::command]
async fn sftp_remove(files: State<'_, Files>, sid: String, path: String) -> Result<(), String> {
    files_get(&files, &sid)?.remove(&path).await
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
    if let Some(t) = transfers.0.lock().unwrap().remove(&tid) {
        t.abort();
    }
}

// ---------- VNC ----------

struct Desktop {
    session: Arc<vnc::Session>,
    share: Arc<ClipShare>,
    events: Channel<InvokeResponseBody>,
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
    let password = vault::get(&c.id, Kind::Password)?;
    let vnc_password = vault::get(&c.id, Kind::VncPassword)?;
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
    let password = vault::get(&c.id, Kind::RdpPassword)?.or(vault::get(&c.id, Kind::Password)?).unwrap_or_default();
    let pins_path = path.0.with_file_name("known_certs.json");
    let mut pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let target = rdp::Target {
        host: c.address.clone(),
        port,
        user: c.user.clone(),
        domain: (!c.rdp_domain.trim().is_empty()).then(|| c.rdp_domain.trim().to_string()),
        password,
        expected_cert: trust.clone().or_else(|| pins.get(&pin_key).cloned()),
        skip_cert_check: c.skip_cert_check,
        width: width.clamp(200, 8192),
        height: height.clamp(200, 8192),
        clipboard: None,
        audio: c.play_audio,
        share: (!c.shared_folder.trim().is_empty()).then(|| PathBuf::from(c.shared_folder.trim())),
    };
    let sid = new_sid(&c.id);
    let (tx, rx) = rdp::channel();
    let mut target = target;
    if c.shared_clipboard {
        target.clipboard = rdp_clipboard(&app, sid.clone(), tx.clone()).await;
    }
    let seen: rdp::SeenCert = Default::default();
    let opened = rdp::open(target, seen.clone(), (tx, rx), move |e| screen_frame(e).is_none_or(|f| on_event.send(f).is_ok())).await;
    let session = match opened {
        Ok(s) => s,
        Err(e) => {
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
    if let Some(s) = sessions.0.lock().unwrap().remove(&sid) {
        s.close();
    }
    rdp_clipboard_drop(&app, sid);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            let help = SubmenuBuilder::new(app, "Hilfe")
                .text("help:rdp-gnome", "RDP mit GNOME (Ubuntu)")
                .text("help:rdp-windows", "RDP mit Windows")
                .text("help:vnc", "VNC allgemein")
                .text("help:wayvnc", "wayvnc (wlroots)")
                .text("help:macos", "macOS-Bildschirmfreigabe")
                .text("help:ssh", "SSH und SFTP")
                .build()?;
            // macOS braucht das Standardmenü (Bearbeiten: Kopieren/Einfügen); Windows nur «Hilfe».
            let menu = if cfg!(target_os = "macos") {
                let m = tauri::menu::Menu::default(app.handle())?;
                m.insert(&file, 1)?;
                m.append(&help)?;
                m
            } else {
                MenuBuilder::new(app).item(&file).item(&help).build()?
            };
            app.set_menu(menu)?;
            app.on_menu_event(|app, e| {
                if let Some(topic) = e.id().as_ref().strip_prefix("help:") {
                    let _ = app.emit("help", topic);
                } else if let Some(cmd) = e.id().as_ref().strip_prefix("cmd:") {
                    let _ = app.emit("cmd", cmd);
                }
            });
            let dir = app.path().app_data_dir()?;
            diag::init(&dir);
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
            local_list,
            save_bytes,
            certs_list,
            quick_register,
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
