//! RDP-Sitzungen.

use crate::*;

// ---------- RDP ----------

#[derive(Default)]
pub(crate) struct RdpSessions(pub(crate) Mutex<HashMap<String, rdp::Session>>);

// Das Windows-Zwischenablage-Backend braucht ein Fenster auf einem Thread mit Nachrichtenschleife:
// es entsteht und lebt auf dem Tauri-Hauptthread, je RDP-Sitzung eines.
#[cfg(windows)]
thread_local! {
    static WIN_CLIPS: std::cell::RefCell<HashMap<String, ironrdp_cliprdr_native::WinClipboard>> = Default::default();
}

#[cfg(windows)]
pub(crate) async fn rdp_clipboard(app: &AppHandle, key: String, tx: tokio::sync::mpsc::UnboundedSender<rdp::Input>) -> Option<rdp::ClipFactory> {
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

/// macOS und Linux: Text über die Zwischenablage des Systems (rdpclip).
#[cfg(not(windows))]
pub(crate) async fn rdp_clipboard(_: &AppHandle, _: String, tx: tokio::sync::mpsc::UnboundedSender<rdp::Input>) -> Option<rdp::ClipFactory> {
    Some(Arc::new(Mutex::new(Box::new(rdpclip::Factory(tx)))))
}

pub(crate) fn rdp_clipboard_drop(app: &AppHandle, key: String) {
    #[cfg(windows)]
    let _ = app.run_on_main_thread(move || {
        WIN_CLIPS.with(|m| m.borrow_mut().remove(&key));
    });
    #[cfg(not(windows))]
    let _ = (app, key);
}

pub(crate) fn screen_frame(e: vnc::Event) -> Option<InvokeResponseBody> {
    Some(match e {
        vnc::Event::Frame(list) => frame_rects(&list),
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
pub(crate) async fn rdp_open(
    app: AppHandle,
    path: State<'_, LibraryPath>,
    sessions: State<'_, RdpSessions>,
    id: String,
    trust: Option<String>,
    remember: Option<bool>,
    width: u16,
    height: u16,
    scale: Option<u32>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<String, String> {
    let c = find(&path, &id)?;
    let port = c.rdp_port.ok_or("Desktop über RDP ist für diese Verbindung ausgeschaltet")?;
    let (user, password) = rdp_login(&path, &c)?;
    let domain = (!c.rdp_domain.trim().is_empty()).then(|| c.rdp_domain.trim().to_string());
    let gateway = rdp_gateway(&path, &c, &user, &domain, &password)?;
    let pins_path = path.0.with_file_name("known_certs.json");
    let pins = load_pins(&pins_path);
    let pin_key = format!("{}:{port}", c.address);
    let (via_host, via_port, tunnel) = desktop_via_ssh(&c, port).await?;
    let mut tunnel = TunnelGuard(tunnel);
    let target = rdp::Target {
        host: via_host,
        server_name: c.address.clone(),
        port: via_port,
        user,
        domain,
        password,
        expected_cert: trust.clone().or_else(|| pins.get(&pin_key).cloned()),
        skip_cert_check: c.skip_cert_check,
        width: width.clamp(200, 8192),
        height: height.clamp(200, 8192),
        scale: scale.unwrap_or(100).clamp(100, 500),
        clipboard: None,
        audio: c.play_audio,
        share: (cfg!(desktop) && !c.shared_folder.trim().is_empty()).then(|| PathBuf::from(c.shared_folder.trim())),
        kdc: kdc::kdc_url(&c.rdp_kdc)?,
        no_ntlm: c.rdp_no_ntlm,
        gateway,
    };
    let sid = new_sid(&c.id);
    touch(&path, &c.id);
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
    if let Some(t) = tunnel.0.take() {
        TUNNELS.lock().unwrap().insert(sid.clone(), t);
    }
    if let Some(fp) = trust.filter(|fp| remember.unwrap_or(true) && seen.lock().unwrap().as_ref() == Some(fp)) {
        let _ = save_pins(&pins_path, &pin_key, Some(fp));
    }
    sessions.0.lock().unwrap().insert(sid.clone(), session);
    Ok(sid)
}

pub(crate) fn rdp_send(s: &RdpSessions, sid: &str, i: rdp::Input) -> Result<(), String> {
    s.0.lock().unwrap().get(sid).map(|x| x.send(i)).ok_or_else(|| "RDP-Sitzung geschlossen".into())
}

#[tauri::command]
pub(crate) fn rdp_pointer(sessions: State<RdpSessions>, sid: String, x: u16, y: u16, buttons: u8) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Pointer { x, y, buttons })
}

#[tauri::command]
pub(crate) fn rdp_wheel(sessions: State<RdpSessions>, sid: String, vertical: bool, units: i16) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Wheel { vertical, units })
}

/// Scancode oder Unicode je nach Taste (siehe `rdp::key_input`).
#[tauri::command]
pub(crate) fn rdp_key(sessions: State<RdpSessions>, sid: String, code: String, key: String, down: bool, altgr: bool) -> Result<(), String> {
    match rdp::key_input(&code, &key, down, altgr) {
        Some(i) => rdp_send(&sessions, &sid, i),
        None => Ok(()),
    }
}

#[tauri::command]
pub(crate) fn rdp_set_monitors(sessions: State<RdpSessions>, sid: String, monitors: Vec<rdp::Monitor>) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Monitors(monitors))
}

#[tauri::command]
pub(crate) fn rdp_release_all(sessions: State<RdpSessions>, sid: String) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::ReleaseAll)
}

#[tauri::command]
pub(crate) fn rdp_set_size(sessions: State<RdpSessions>, sid: String, width: u16, height: u16, scale: Option<u32>) -> Result<(), String> {
    rdp_send(&sessions, &sid, rdp::Input::Resize { width, height, scale: scale.unwrap_or(100).clamp(100, 500) })
}

#[tauri::command]
pub(crate) fn rdp_close(app: AppHandle, sessions: State<RdpSessions>, sid: String) {
    tunnel_end(&sid);
    relay::DESKTOPS.lock().unwrap().remove(&sid);
    if let Some(s) = sessions.0.lock().unwrap().remove(&sid) {
        s.close();
    }
    rdp_clipboard_drop(&app, sid);
}
