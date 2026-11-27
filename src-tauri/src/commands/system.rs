//! Tunnel, Fenster, Zwischenablage, Vorschaubilder, Verknüpfungen, lokale Dateien.

use crate::*;

/// Laufende SSH-Tunnel je Desktop-Sitzung (beim Trennen beendet).
pub(crate) type Tunnel = (Arc<russh::client::Handle<ssh::Client>>, tokio::task::JoinHandle<()>);
/// Beendet den Tunnel, wenn er nicht an eine Sitzung übergeben wurde (Fehler beim Verbinden).
pub(crate) struct TunnelGuard(pub(crate) Option<Tunnel>);
impl Drop for TunnelGuard {
    fn drop(&mut self) {
        if let Some((_, task)) = self.0.take() {
            task.abort();
        }
    }
}
pub(crate) static TUNNELS: std::sync::LazyLock<Mutex<HashMap<String, Tunnel>>> = std::sync::LazyLock::new(Default::default);
pub(crate) fn tunnel_end(sid: &str) {
    if let Some((_, task)) = TUNNELS.lock().unwrap().remove(sid) {
        task.abort();
    }
}

/// «Sicher über SSH verbinden»: SSH zum Rechner, lokaler Port → `port` dort. Gibt (Host, Port) für RDP/VNC zurück.
pub(crate) async fn desktop_via_ssh(c: &Connection, port: u16) -> Result<(String, u16, Option<Tunnel>), String> {
    if !c.ssh_tunnel {
        return Ok((c.address.clone(), port, None));
    }
    if c.ssh_port.is_none() {
        return Err("«Sicher über SSH verbinden» braucht SSH bei dieser Verbindung (Dienste › Shell).".into());
    }
    let t = ssh_target(c, None)?;
    let h = ssh::connect(&t).await.map_err(|e| {
        if e.starts_with(ssh::HOSTKEY_UNKNOWN) {
            "SSH-Tunnel: Der Schlüssel des Servers ist noch nicht bestätigt. Bitte einmal die Shell öffnen und ihn bestätigen.".to_string()
        } else {
            format!("SSH-Tunnel: {e}")
        }
    })?;
    let h = Arc::new(h);
    let (local, task) = forward::tunnel(h.clone(), port).await?;
    tracing::info!(port, local, "SSH-Tunnel für den Desktop steht");
    Ok(("127.0.0.1".into(), local, Some((h, task))))
}

/// Erreichbarkeit einer Verbindung (Latenz in ms). Merkt sich dabei die MAC-Adresse für Wake-on-LAN.
#[tauri::command]
pub(crate) async fn conn_reach(path: State<'_, LibraryPath>, id: String) -> Result<Option<u32>, String> {
    let c = find(&path, &id)?;
    let ports: Vec<u16> = [c.rdp_port, c.vnc_port, c.ssh_port, c.ftp_port].into_iter().flatten().collect();
    let ms = wake::reach(&c.address, &ports).await;
    if ms.is_some() && c.mac.is_empty() {
        let host = c.address.clone();
        if let Some(mac) = tauri::async_runtime::spawn_blocking(move || wake::mac_of(&host)).await.ok().flatten() {
            let _g = LIBRARY_LOCK.lock().unwrap();
            if let Ok(mut items) = library::load(&path.0)
                && let Some(x) = items.iter_mut().find(|x| x.id == id) {
                    x.mac = mac;
                    let _ = library::save(&path.0, &items);
                }
        }
    }
    Ok(ms)
}

/// Rechner per Wake-on-LAN aufwecken (MAC aus der Verbindung).
#[tauri::command]
pub(crate) fn conn_wake(path: State<'_, LibraryPath>, id: String) -> Result<(), String> {
    let c = find(&path, &id)?;
    if c.mac.is_empty() {
        return Err("Für diesen Rechner ist keine MAC-Adresse bekannt. Sesam merkt sie sich, sobald er einmal eingeschaltet erreichbar war; sie lässt sich auch unter «Erweitert» eintragen.".into());
    }
    wake::wake(&c.mac)
}

/// Selbsttest der Windows-Tastenkombinationen (Befehlspalette «Diagnose: Tastentest»), läuft ausserhalb des Hauptthreads.
#[tauri::command]
pub(crate) async fn kbhook_selftest() -> String {
    tauri::async_runtime::spawn_blocking(kbhook::selftest).await.unwrap_or_default()
}

/// Windows-Tastenkombinationen an den entfernten Rechner geben (Desktop-Tab mit Fokus), siehe kbhook.
#[tauri::command]
pub(crate) fn kbhook_set(active: bool) -> usize {
    kbhook::set_active(active);
    kbhook::calls()
}

/// Text aus der Zwischenablage des Systems (ohne Berechtigungsabfrage der Webansicht).
#[tauri::command]
pub(crate) fn clip_read() -> Result<String, String> {
    clipshare::get_text()
}

/// Text in die Zwischenablage des Systems.
#[tauri::command]
pub(crate) fn clip_write(text: String) -> Result<(), String> {
    clipshare::set_text(&text)
}

/// Vollbild auf dem iPad: Statusleiste aus, Home-Leiste blendet sich aus, Wischen vom oberen Rand braucht zwei
/// Anläufe (die erste Geste geht ans Remote-Bild). Setzt die Eigenschaften des Tao-View-Controllers.
#[tauri::command]
pub(crate) fn immersive(window: tauri::WebviewWindow, on: bool) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    window
        .with_webview(move |w| {
            use objc2::{msg_send, runtime::{AnyObject, Bool}, sel};
            unsafe {
                let web = w.inner() as *mut AnyObject;
                let win: *mut AnyObject = msg_send![web, window];
                if win.is_null() {
                    return;
                }
                let vc: *mut AnyObject = msg_send![win, rootViewController];
                let Some(vc) = vc.as_ref() else { return };
                let b = Bool::new(on);
                if vc.class().responds_to(sel!(setPrefersStatusBarHidden:)) {
                    let () = msg_send![vc, setPrefersStatusBarHidden: b];
                    let () = msg_send![vc, setPrefersHomeIndicatorAutoHidden: b];
                    let edges: usize = if on { 1 } else { 0 }; // UIRectEdgeTop
                    let () = msg_send![vc, setPreferredScreenEdgesDeferringSystemGestures: edges];
                }
            }
        })
        .map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "ios"))]
    let _ = (window, on);
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UiMaterial {
    /// "vibrancy" (macOS), "mica" (Windows 11) oder "none" (deckend).
    pub(crate) material: &'static str,
    pub(crate) reduce_transparency: bool,
    pub(crate) increase_contrast: bool,
}

/// Fenstermaterial hinter der Webansicht (docs/design-konzept.md) und die Einstellungen «Transparenz reduzieren» /
/// «Kontrast erhöhen» des Systems. WebKit kennt prefers-reduced-transparency nicht, darum hier nativ (macOS).
#[tauri::command]
pub(crate) fn ui_material(window: tauri::WebviewWindow) -> UiMaterial {
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::{AnyObject, Bool};
        use objc2::{class, msg_send};
        use tauri::window::{Effect, EffectState, EffectsBuilder};
        let (reduce, contrast) = unsafe {
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let r: Bool = msg_send![ws, accessibilityDisplayShouldReduceTransparency];
            let c: Bool = msg_send![ws, accessibilityDisplayShouldIncreaseContrast];
            (r.as_bool(), c.as_bool())
        };
        let ok = !reduce && !contrast && window.set_effects(EffectsBuilder::new().effect(Effect::Sidebar).state(EffectState::FollowsWindowActiveState).build()).is_ok();
        return UiMaterial { material: if ok { "vibrancy" } else { "none" }, reduce_transparency: reduce, increase_contrast: contrast };
    }
    #[cfg(windows)]
    {
        use tauri::window::{Effect, EffectsBuilder};
        // Windows 10 kennt Mica nicht (Fehler) → deckend. «Transparenzeffekte aus» schaltet Windows selbst ab.
        let ok = window.set_effects(EffectsBuilder::new().effect(Effect::Mica).build()).is_ok();
        return UiMaterial { material: if ok { "mica" } else { "none" }, reduce_transparency: false, increase_contrast: false };
    }
    #[allow(unreachable_code)]
    {
        let _ = window;
        UiMaterial { material: "none", reduce_transparency: false, increase_contrast: false }
    }
}

/// Lage des eigenen Maximieren-Knopfs in physischen Pixeln (Andocklayouts, src/snap.rs); andere Systeme: nichts.
#[tauri::command]
pub(crate) fn snap_button(window: tauri::WebviewWindow, x: i32, y: i32, w: i32, h: i32) {
    #[cfg(windows)]
    crate::snap::place(window, x, y, w, h);
    #[cfg(not(windows))]
    let _ = (window, x, y, w, h);
}

/// «Beenden» aus dem eigenen Menü (Windows/Linux).
#[tauri::command]
pub(crate) fn app_exit(app: AppHandle) {
    app.exit(0);
}

/// Laufende Desktop-Sitzung (VNC/RDP) an das Fenster dieses Kanals hängen.
#[tauri::command]
pub(crate) fn desktop_attach(sid: String, on_event: Channel<InvokeResponseBody>) -> Result<(), String> {
    relay::DESKTOPS.lock().unwrap().get(&sid).ok_or("Desktop-Sitzung geschlossen")?.swap(on_event);
    Ok(())
}

/// Laufende Shell an das Fenster dieses Kanals hängen.
#[tauri::command]
pub(crate) fn shell_attach(sid: String, on_event: Channel<ssh::ShellEvent>) -> Result<(), String> {
    relay::SHELLS.lock().unwrap().get(&sid).ok_or("Shell geschlossen")?.swap(on_event);
    Ok(())
}

/// Sitzung in ein eigenes Fenster abkoppeln; `token` verweist auf die Übergabe im localStorage.
/// Einstellungen als eigenes Fenster (macOS, HIG): vorhandenes nach vorne holen und den Bereich wechseln, sonst öffnen.
#[tauri::command]
pub(crate) async fn open_settings_window(app: AppHandle, tab: String) -> Result<(), String> {
    if !tab.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Unbekannter Bereich".into());
    }
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        return w.emit("settings-tab", tab).map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App("index.html".into()))
        .initialization_script(format!("window.__SESAM_SETTINGS__ = '{tab}';"))
        .title("Einstellungen")
        .transparent(true)
        .inner_size(820.0, 600.0)
        .min_inner_size(640.0, 440.0)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Asynchron: ein Fenster in einem synchronen Befehl zu erzeugen, blockiert unter Windows (leeres Fenster, X reagiert nicht).
#[tauri::command]
pub(crate) async fn open_detached(app: AppHandle, token: String, title: String) -> Result<(), String> {
    if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Ungültige Übergabe".into());
    }
    // iPad: keine weiteren Fenster (das Frontend bietet Abkoppeln dort nicht an).
    #[cfg(mobile)]
    return {
        let _ = (app, title);
        Err("Eigene Fenster gibt es auf diesem Gerät nicht".into())
    };
    #[cfg(desktop)]
    let builder = tauri::WebviewWindowBuilder::new(&app, format!("detached-{token}"), tauri::WebviewUrl::App("index.html".into()))
        .initialization_script(format!("window.__SESAM_DETACH__ = '{token}';"))
        .title(title)
        .inner_size(1200.0, 800.0)
        .min_inner_size(480.0, 320.0)
        .decorations(cfg!(target_os = "macos"))
        .transparent(true)
        .shadow(true);
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    #[cfg(desktop)]
    {
        builder.build().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[tauri::command]
pub(crate) fn sftp_close(files: State<Files>, sid: String) {
    files.0.lock().unwrap().remove(&sid);
    files.1.lock().unwrap().remove(&sid);
}

/// Setup-Script für GNOME Remote Desktop, unverändert eingebettet (LF, ohne BOM).
pub(crate) const SETUP_SCRIPT: &[u8] = include_bytes!("../../resources/sesam-rdp-setup.sh");

/// Script-Inhalt und SHA-256 (zum Herunterladen und als Versionsangabe).
#[tauri::command]
pub(crate) fn setup_script() -> (Vec<u8>, String) {
    use sha2::Digest;
    (SETUP_SCRIPT.to_vec(), sha2::Sha256::digest(SETUP_SCRIPT).iter().map(|b| format!("{b:02x}")).collect())
}

/// Script per SFTP nach `~/sesam-rdp-setup.sh` hochladen (Bytes 1:1). Ausführen tut der Benutzer selbst in der Shell.
#[tauri::command]
pub(crate) async fn setup_upload(path: State<'_, LibraryPath>, id: String, trust: Option<String>) -> Result<String, String> {
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

pub(crate) fn thumb_path(path: &LibraryPath, id: &str) -> Result<PathBuf, String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || id.is_empty() {
        return Err("Ungültige ID".into());
    }
    Ok(path.0.with_file_name("thumbs").join(format!("{id}.jpg")))
}

/// Vorschaubild speichern (JPEG vom Frontend: letzter Desktop beim Trennen).
#[tauri::command]
pub(crate) fn thumb_save(path: State<LibraryPath>, id: String, data: Vec<u8>) -> Result<(), String> {
    if id.starts_with("quick-") {
        return Ok(()); // schnelle Verbindungen hinterlassen nichts
    }
    let p = thumb_path(&path, &id)?;
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn thumb_get(path: State<LibraryPath>, id: String) -> Option<Vec<u8>> {
    std::fs::read(thumb_path(&path, &id).ok()?).ok()
}

/// Eigenes Bild übernehmen (vom Benutzer im Dialog gewählt).
#[tauri::command]
pub(crate) fn thumb_import(path: State<LibraryPath>, id: String, file: String) -> Result<(), String> {
    let data = std::fs::read(&file).map_err(|e| format!("{file}: {e}"))?;
    if data.len() > 8 << 20 {
        return Err("Bild zu gross (höchstens 8 MB)".into());
    }
    let p = thumb_path(&path, &id)?;
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())
}

/// Freedesktop-Starter (Linux). Steuerzeichen im Namen würden neue Schlüssel einschleusen, darum weg damit.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn desktop_entry(name: &str, label: &str, url: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    format!("[Desktop Entry]\nType=Application\nName={name} ({label})\nExec=xdg-open \"{url}\"\nIcon=sesam-remote\nTerminal=false\n")
}

/// Verknüpfung auf dem Schreibtisch: öffnet `sesam://open?id=…&view=…`.
/// Windows: .url mit dem Sesam-Symbol, macOS: .webloc, Linux: .desktop.
#[tauri::command]
pub(crate) fn create_shortcut(app: AppHandle, path: State<LibraryPath>, id: String, view: String) -> Result<String, String> {
    let c = find(&path, &id)?;
    if !["desktop", "shell", "files"].contains(&view.as_str()) {
        return Err("Unbekannte Ansicht".into());
    }
    let label = match view.as_str() { "desktop" => "Desktop", "shell" => "Shell", _ => "Dateien" };
    let safe: String = c.name.chars().map(|ch| if "\\/:*?\"<>|".contains(ch) || ch.is_control() { '_' } else { ch }).collect();
    let url = format!("sesam://open?id={}&view={view}", c.id);
    let desk = app.path().desktop_dir().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let (file, body) = (
        desk.join(format!("{safe} ({label}).webloc")),
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>URL</key><string>{url}</string></dict></plist>\n"),
    );
    #[cfg(target_os = "linux")]
    let (file, body) = (desk.join(format!("{safe} ({label}).desktop")), desktop_entry(&c.name, label, &url));
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let (file, body) = {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        (desk.join(format!("{safe} ({label}).url")), format!("[InternetShortcut]\r\nURL={url}\r\nIconFile={exe}\r\nIconIndex=0\r\n"))
    };
    std::fs::write(&file, body).map_err(|e| format!("{}: {e}", file.display()))?;
    // Freedesktop: Starter auf dem Schreibtisch müssen ausführbar sein.
    #[cfg(target_os = "linux")]
    std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o755)).map_err(|e| format!("{}: {e}", file.display()))?;
    #[cfg(target_os = "macos")]
    sfsymbol::set_app_icon(&file);
    Ok(file.display().to_string())
}

/// Dateien aufs Desktop-Fenster gezogen: per SFTP nach ~/Sesam-Ablage (wenn SSH an ist),
/// sonst in den per RDP freigegebenen Ordner kopieren. Gibt eine Meldung für die Statuszeile zurück.
#[tauri::command]
pub(crate) async fn drop_files(path: State<'_, LibraryPath>, id: String, files: Vec<String>) -> Result<String, String> {
    let c = find(&path, &id)?;
    let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
    if c.ssh_port.is_some() {
        let share = ClipShare::new(c.clone(), std::env::temp_dir());
        let done = share.upload_files(&files).await?;
        return Ok(match done.as_slice() {
            [one] => format!("Hochgeladen nach {one}"),
            _ => format!("{} Elemente hochgeladen nach {}", done.len(), done.first().map(|p| sftp::rparent(p).unwrap_or_default()).unwrap_or_default()),
        });
    }
    let dir = c.shared_folder.trim();
    // iPadOS: kein freier Dateizugriff, Ordnerfreigabe gibt es dort nicht (docs/plattformen.md)
    if cfg!(desktop) && c.rdp_port.is_some() && !dir.is_empty() {
        let dir = PathBuf::from(dir);
        for f in &files {
            let name = f.file_name().ok_or("Ungültige Datei")?;
            let mut target = dir.join(name);
            let mut n = 2;
            while target.exists() {
                target = dir.join(sftp::numbered(&name.to_string_lossy(), n, false));
                n += 1;
            }
            if f.is_dir() {
                return Err("Ordner lassen sich nur mit SSH übertragen.".into());
            }
            std::fs::copy(f, &target).map_err(|e| format!("{}: {e}", f.display()))?;
        }
        return Ok(format!("{} Datei(en) in den freigegebenen Ordner kopiert (auf dem Server als Laufwerk sichtbar).", files.len()));
    }
    Err(format!("«{}» hat kein SSH; darum kann Sesam keine Dateien hinüberlegen. SSH bei der Verbindung einschalten (Bearbeiten › Dienste) oder unter RDP einen Ordner freigeben.", c.name))
}

/// Gemerkte TLS-Zertifikate (RDP/VNC) für die Einstellungen.
#[tauri::command]
pub(crate) fn certs_list(path: State<LibraryPath>) -> Vec<(String, String)> {
    let mut v: Vec<_> = load_pins(&path.0.with_file_name("known_certs.json")).into_iter().collect();
    v.sort();
    v
}

#[tauri::command]
pub(crate) fn cert_remove(path: State<LibraryPath>, host: String) -> Result<(), String> {
    save_pins(&path.0.with_file_name("known_certs.json"), &host, None)
}

/// Vom Benutzer im Speichern-Dialog gewählte Datei schreiben (z. B. Bildschirmfoto).
#[tauri::command]
pub(crate) fn save_bytes(path: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, data).map_err(|e| format!("{path}: {e}"))
}

#[tauri::command]
pub(crate) fn local_list(app: AppHandle, path: String) -> Result<sftp::Listing, String> {
    // iPad: nur der Ordner «Documents» der App (in der Dateien-App «Auf meinem iPad › Sesam Remote»), darüber nichts.
    #[cfg(mobile)]
    {
        // Freigegebene Ordner aus «Dateien» (folders.rs): darin frei, an ihrer Wurzel kein «übergeordneter Ordner».
        if let Some(at_root) = folders::inside(std::path::Path::new(&path)) {
            let mut l = sftp::local_list(&path)?;
            if at_root {
                l.parent = None;
            }
            return Ok(l);
        }
        let root = local_home(app)?;
        let inside = std::path::Path::new(&path).canonicalize().ok().zip(std::path::Path::new(&root).canonicalize().ok()).is_some_and(|(p, r)| p.starts_with(r));
        let mut l = sftp::local_list(if inside { &path } else { &root })?;
        if !inside || l.path.trim_end_matches('/') == root.trim_end_matches('/') {
            l.parent = None;
        }
        return Ok(l);
    }
    #[cfg(desktop)]
    {
        let _ = app;
        sftp::local_list(&path)
    }
}

/// Vorschau (Leertaste): höchstens `max` Byte einer lokalen Datei, roh. iPad: nur aus den Ordnern der App.
#[tauri::command]
pub(crate) fn local_read(app: AppHandle, path: String, max: u64) -> Result<tauri::ipc::Response, String> {
    #[cfg(mobile)]
    {
        let ok = folders::inside(std::path::Path::new(&path)).is_some()
            || [app.path().document_dir(), app.path().app_cache_dir()].into_iter().flatten().any(|d| std::path::Path::new(&path).canonicalize().ok().zip(d.canonicalize().ok()).is_some_and(|(p, r)| p.starts_with(r)));
        if !ok {
            return Err("Nur Dateien aus den Ordnern von Sesam".into());
        }
    }
    #[cfg(desktop)]
    let _ = &app;
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(&path).and_then(|f| f.take(max).read_to_end(&mut buf)).map_err(|e| format!("{path}: {e}"))?;
    Ok(tauri::ipc::Response::new(buf))
}

/// iPad: Ordner in «Dateien» wählen (Zugriff bleibt über ein Lesezeichen); `None` bei Abbruch.
#[tauri::command]
pub(crate) async fn folder_pick(window: tauri::WebviewWindow) -> Result<Option<folders::Folder>, String> {
    folders::pick(window).await
}

/// iPad: gemerkten Ort wieder öffnen (Lesezeichen auflösen, Zugriff starten).
#[tauri::command]
pub(crate) fn folder_open(bookmark: String) -> Result<folders::Folder, String> {
    folders::open(&bookmark)
}

/// iPad: Ort entfernen (Zugriff beenden).
#[tauri::command]
pub(crate) fn folder_close(path: String) {
    folders::close(&path)
}

/// «Dateien wählen …»: Kopien in einen Ordner der linken Seite legen.
#[tauri::command]
pub(crate) fn local_import(paths: Vec<String>, dir: String) -> Result<Vec<String>, String> {
    folders::import(&paths, std::path::Path::new(&dir))
}

/// Arbeitsordner im Cache (Vorschau, Bearbeiten); `sub` nur aus Buchstaben, Ziffern und «-».
#[tauri::command]
pub(crate) fn local_temp(app: AppHandle, sub: String) -> Result<String, String> {
    if sub.is_empty() || !sub.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Ungültiger Ordnername".into());
    }
    let d = app.path().app_cache_dir().map_err(|e| e.to_string())?.join(sub);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d.to_string_lossy().into_owned())
}
