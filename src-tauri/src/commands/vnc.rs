//! VNC-Sitzungen und Berechtigungen.

use crate::*;

// ---------- VNC ----------

pub(crate) struct Desktop {
    pub(crate) session: Arc<vnc::Session>,
    pub(crate) share: Arc<ClipShare>,
    pub(crate) events: relay::Relay<InvokeResponseBody>,
}

#[derive(Default)]
pub(crate) struct Desktops(pub(crate) Mutex<HashMap<String, Desktop>>);

/// Gepinnte TLS-Zertifikate der VNC-Server: "host:port" → SHA256.
pub(crate) fn load_pins(path: &std::path::Path) -> HashMap<String, String> {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Einen Fingerabdruck merken (`Some`) oder vergessen (`None`): unter Sperre frisch lesen, ändern und atomar
/// schreiben, damit gleichzeitige Verbindungen einander nichts überschreiben und die Datei nie halb ist.
pub(crate) fn save_pins(path: &std::path::Path, key: &str, fp: Option<String>) -> Result<(), String> {
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut pins = load_pins(path);
    match fp {
        Some(fp) => pins.insert(key.to_string(), fp),
        None => pins.remove(key),
    };
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&pins).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}


/// Binärrahmen ans Frontend (Big Endian): 0 Rechteck, 1 Grösse, 3 Ende, 4 Glocke, 5 Hinweis, 6 Mauszeiger.
/// Ein Bild mit allen geänderten Rechtecken (Art 7): Anzahl, dann je x, y, b, h und RGBA.
pub(crate) fn frame_rects(list: &[(vnc::decode::Rect, Vec<u8>)]) -> InvokeResponseBody {
    let mut b = Vec::with_capacity(3 + list.iter().map(|(_, p)| 8 + p.len()).sum::<usize>());
    b.push(7);
    b.extend((list.len() as u16).to_be_bytes());
    for (r, px) in list {
        [r.x, r.y, r.w, r.h].iter().for_each(|n| b.extend(n.to_be_bytes()));
        b.extend_from_slice(px);
    }
    InvokeResponseBody::Raw(b)
}

pub(crate) fn frame(kind: u8, nums: &[u16], tail: &[u8]) -> InvokeResponseBody {
    let mut b = Vec::with_capacity(1 + nums.len() * 2 + tail.len());
    b.push(kind);
    nums.iter().for_each(|n| b.extend(n.to_be_bytes()));
    b.extend_from_slice(tail);
    InvokeResponseBody::Raw(b)
}

/// Verbinden und anmelden (direkt oder durch den SSH-Tunnel), mit Zertifikat-Pinning wie beim Öffnen.
pub(crate) async fn vnc_login(path: &LibraryPath, c: &Connection, trust: Option<String>, remember: Option<bool>) -> Result<(vnc::Stream, vnc::ServerInit, TunnelGuard), String> {
    let port = c.vnc_port.ok_or("Desktop (VNC) ist für diese Verbindung ausgeschaltet")?;
    let password = login_secret(c, Kind::Password)?;
    let vnc_password = match &c.vnc_credential_id {
        Some(cid) => cred_login(path, cid)?.1,
        None => vault::get(&c.id, Kind::VncPassword)?,
    };
    let pins_path = path.0.with_file_name("known_certs.json");
    let pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let cert = if c.skip_cert_check {
        vnc::tls::Pinned::accept_any()
    } else {
        vnc::tls::Pinned::new(trust.clone().or_else(|| pins.get(&pin_key).cloned()))
    };
    let login = vnc::Login { host: &c.address, user: &c.user, password: password.as_deref(), vnc_password: vnc_password.as_deref(), cert };
    let timeout = std::time::Duration::from_secs(15);
    let (via_host, via_port, tunnel) = desktop_via_ssh(c, port).await?;
    let tunnel = TunnelGuard(tunnel);
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect((via_host.as_str(), via_port)))
        .await
        .map_err(|_| "Zeitüberschreitung beim Verbinden".to_string())?
        .map_err(|e| firstaid::net_error(&via_host, via_port, &e))?;
    let _ = stream.set_nodelay(true);
    let (stream, init) = tokio::time::timeout(timeout, vnc::handshake(stream, &login))
        .await
        .map_err(|_| "Zeitüberschreitung bei der Anmeldung".to_string())??;
    // Bestätigtes Zertifikat merken, aber nur wenn TLS es wirklich gesehen hat.
    if let Some(fp) = trust.filter(|fp| remember.unwrap_or(true) && login.cert.seen.lock().unwrap().as_ref() == Some(fp)) {
        let _ = save_pins(&pins_path, &pin_key, Some(fp));
    }
    Ok((stream, init, tunnel))
}

#[tauri::command]
pub(crate) async fn vnc_open(
    app: AppHandle,
    path: State<'_, LibraryPath>,
    desktops: State<'_, Desktops>,
    id: String,
    trust: Option<String>,
    remember: Option<bool>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let (stream, init, mut tunnel) = vnc_login(&path, &c, trust, remember).await?;

    let downloads = app.path().download_dir().unwrap_or_else(|_| std::env::temp_dir());
    let share = Arc::new(ClipShare::new(c.clone(), downloads));
    let on_event = relay::Relay::new(on_event);
    let (sh, events) = (share.clone(), on_event.clone());
    let session = vnc::Session::start(stream, init, move |e| {
        let f = match e {
            vnc::Event::Frame(list) => frame_rects(&list),
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
    touch(&path, &c.id);
    relay::DESKTOPS.lock().unwrap().insert(sid.clone(), events.clone());
    desktops.0.lock().unwrap().insert(sid.clone(), Desktop { session: Arc::new(session), share, events });
    if let Some(t) = tunnel.0.take() {
        TUNNELS.lock().unwrap().insert(sid.clone(), t);
    }
    Ok(sid)
}

pub(crate) fn with_desktop<T>(d: &Desktops, sid: &str, f: impl FnOnce(&Desktop) -> T) -> Result<T, String> {
    d.0.lock().unwrap().get(sid).map(f).ok_or_else(|| "Desktop-Sitzung geschlossen".into())
}

/// Textdatei an einen im Speichern-Dialog gewählten Ort schreiben (z. B. exportiertes Terminal-Design).
#[tauri::command]
pub(crate) fn save_text_file(path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"))
}

/// Freigaben des Betriebssystems mit Status (Einstellungen › «Berechtigungen einrichten …»).
#[tauri::command]
pub(crate) fn perm_list() -> Vec<perms::Perm> {
    perms::list()
}

/// Freigabe anfragen (Systemdialog) und den neuen Stand liefern.
#[tauri::command]
pub(crate) fn perm_request(id: String) -> Vec<perms::Perm> {
    perms::request(&id);
    perms::list()
}

/// Passende Stelle in den Systemeinstellungen öffnen.
#[tauri::command]
pub(crate) fn perm_open(id: String) -> Result<(), String> {
    perms::open(&id)
}

/// Die Oberfläche hat ein Bild gezeichnet (VNC oder RDP): das nächste darf kommen.
#[tauri::command]
pub(crate) fn desktop_ack(desktops: State<Desktops>, sessions: State<RdpSessions>, sid: String) {
    if let Some(d) = desktops.0.lock().unwrap().get(&sid) {
        d.session.ack();
    } else if let Some(s) = sessions.0.lock().unwrap().get(&sid) {
        s.send(rdp::Input::Ack);
    }
}

#[tauri::command]
pub(crate) fn vnc_pointer(desktops: State<Desktops>, sid: String, mask: u8, x: u16, y: u16) -> Result<(), String> {
    with_desktop(&desktops, &sid, |d| d.session.send(vnc::pointer(mask, x, y)))
}

#[tauri::command]
pub(crate) fn vnc_key(desktops: State<Desktops>, sid: String, down: bool, keysym: u32) -> Result<(), String> {
    with_desktop(&desktops, &sid, |d| d.session.send(vnc::key(down, keysym)))
}

/// `false`, wenn der Server keine Grössenänderung kann.
#[tauri::command]
pub(crate) fn vnc_set_size(desktops: State<Desktops>, sid: String, width: u16, height: u16) -> Result<bool, String> {
    with_desktop(&desktops, &sid, |d| d.session.set_size(width, height))
}

/// Lokale Zwischenablage an den Server geben, falls sie sich geändert hat (beim Fokussieren aufgerufen).
/// Text direkt über VNC; Dateien und Bilder per SFTP nach `~/Sesam-Ablage`, dazu deren Pfade als Text.
#[tauri::command]
pub(crate) async fn vnc_clipboard_sync(desktops: State<'_, Desktops>, sid: String) -> Result<(), String> {
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
pub(crate) fn vnc_close(desktops: State<Desktops>, sid: String) {
    tunnel_end(&sid);
    relay::DESKTOPS.lock().unwrap().remove(&sid);
    if let Some(d) = desktops.0.lock().unwrap().remove(&sid) {
        d.session.close();
    }
}
