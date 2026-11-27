//! Updates, Öffnen lokaler Dateien, Übertragungen.

use crate::*;

// ---------- Automatische Updates (src/update.rs; iPad: App Store) ----------

#[tauri::command]
pub(crate) fn update_status(app: AppHandle) -> serde_json::Value {
    #[cfg(desktop)]
    return serde_json::to_value(update::status(&app)).unwrap_or_default();
    #[cfg(mobile)]
    {
        let _ = app;
        serde_json::json!({ "current": app.package_info().version.to_string(), "install": "never" })
    }
}

#[tauri::command]
pub(crate) fn update_set_auto(auto: bool) -> Result<(), String> {
    #[cfg(desktop)]
    return update::set_auto(auto);
    #[cfg(mobile)]
    {
        let _ = auto;
        Err("Auf dem iPad aktualisiert der App Store".into())
    }
}

#[tauri::command]
pub(crate) async fn update_check(app: AppHandle) -> Result<serde_json::Value, String> {
    #[cfg(desktop)]
    {
        update::check(&app, true).await?;
        Ok(serde_json::to_value(update::status(&app)).unwrap_or_default())
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Err("Auf dem iPad aktualisiert der App Store".into())
    }
}

#[tauri::command]
pub(crate) async fn update_install(app: AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    return update::install_now(&app).await;
    #[cfg(mobile)]
    {
        let _ = app;
        Err("Auf dem iPad aktualisiert der App Store".into())
    }
}

/// Endungen, deren Standardaktion auf diesem System etwas ausführt statt es zu öffnen.
#[cfg(windows)]
pub(crate) const RUNS_ON_OPEN: &[&str] = &[
    "exe", "com", "bat", "cmd", "vbs", "vbe", "js", "jse", "wsf", "wsh", "wsc", "hta", "msi", "msp", "msc", "scr", "pif", "cpl",
    "lnk", "url", "website", "scf", "reg", "inf", "jar", "chm", "xll", "gadget", "diagcab", "mht", "mhtml", "ps1xml", "iso", "img",
    "vhd", "vhdx", "appref-ms", "application", "settingcontent-ms", "library-ms", "search-ms", "appx", "msix",
];
#[cfg(target_os = "macos")]
pub(crate) const RUNS_ON_OPEN: &[&str] =
    &["app", "command", "tool", "terminal", "workflow", "pkg", "mpkg", "dmg", "fileloc", "inetloc", "webloc", "jar", "jnlp", "shortcut"];
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) const RUNS_ON_OPEN: &[&str] = &["desktop", "appimage", "jar"];

/// Würde die Standardaktion die Datei ausführen? (security-review-2026-11-11, S-05)
pub(crate) fn runs_on_open(path: &str) -> bool {
    let name = path.trim_end_matches(['/', '\\', '.', ' ']).rsplit(['/', '\\']).next().unwrap_or(path).to_ascii_lowercase();
    name.rsplit_once('.').is_some_and(|(_, ext)| RUNS_ON_OPEN.contains(&ext))
}

/// Datei mit dem Standardprogramm des Systems öffnen («Bearbeiten»). Nur Desktop. Dateien vom Server mit einer
/// Endung, die ausgeführt würde (z. B. .bat, .js unter Windows, .command, .terminal auf dem Mac), öffnet Sesam nicht:
/// ein Klick auf «Bearbeiten» darf nie Code vom Server starten.
#[tauri::command]
pub(crate) fn local_open(path: String) -> Result<(), String> {
    if runs_on_open(&path) {
        return Err("Diese Art Datei würde das System ausführen statt zum Bearbeiten öffnen. Bitte herunterladen und in einem Editor öffnen.".into());
    }
    #[cfg(desktop)]
    {
        mark_from_internet(&path);
        tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
    }
    #[cfg(mobile)]
    {
        let _ = path;
        Err("Auf dem iPad gibt es kein Standardprogramm zum Bearbeiten".into())
    }
}

/// Datei vom Server als «aus dem Internet» kennzeichnen, bevor das System sie öffnet (S-05, ergänzt die Sperrliste):
/// macOS Quarantäne (Gatekeeper fragt vor dem Ausführen), Windows Mark-of-the-Web (Zone Internet, Office öffnet in der
/// geschützten Ansicht). Linux kennt nichts Vergleichbares. Klappt es nicht (z. B. FAT-Laufwerk), geht es trotzdem weiter.
#[cfg(desktop)]
pub(crate) fn mark_from_internet(path: &str) {
    #[cfg(target_os = "macos")]
    {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let r = std::process::Command::new("/usr/bin/xattr").args(["-w", "com.apple.quarantine", &format!("0083;{now:x};Sesam Remote;"), path]).status();
        if !r.is_ok_and(|s| s.success()) {
            tracing::warn!("Quarantäne-Markierung nicht gesetzt");
        }
    }
    #[cfg(windows)]
    {
        if let Err(e) = std::fs::write(format!("{path}:Zone.Identifier"), "[ZoneTransfer]\r\nZoneId=3\r\n") {
            tracing::warn!(error = %e, "Mark-of-the-Web nicht gesetzt");
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = path;
}

#[tauri::command]
pub(crate) fn local_home(app: AppHandle) -> Result<String, String> {
    #[cfg(mobile)]
    {
        let d = app.path().document_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        return Ok(d.to_string_lossy().into_owned());
    }
    #[cfg(desktop)]
    app.path().home_dir().map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string())
}

/// iPad: Datei aus der App «Dateien» auf eine Sitzung gezogen. Die Webansicht liefert nur den Inhalt, also hier
/// in einen eigenen Ordner im Cache legen (Kopf `x-name`, Rumpf roh) und den Pfad zurückgeben; danach läuft es wie
/// beim Ziehen auf dem Desktop. Der Ordner wird bei jedem Start geleert.
#[tauri::command]
pub(crate) fn stage_file(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else { return Err("Keine Datei".into()) };
    let raw = request.headers().get("x-name").and_then(|v| v.to_str().ok()).unwrap_or("Datei");
    let name = percent_decode(raw);
    let name = std::path::Path::new(&name).file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty() && *n != "." && *n != "..").ok_or("Ungültiger Dateiname")?.to_string();
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("Ablage").join(new_sid("d"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(name);
    std::fs::write(&file, data).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().into_owned())
}

/// %XX in einem Kopfwert (Dateinamen mit Umlauten gehen kodiert über die Kopfzeile).
pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len()
            && let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Startet eine Übertragung im Hintergrund; Fortschritt über `on_progress`. Gibt die Übertragungs-ID zurück.
#[tauri::command]
pub(crate) async fn transfer_start(
    files: State<'_, Files>,
    transfers: State<'_, Transfers>,
    sid: String,
    upload: bool,
    src: String,
    dest_dir: String,
    replace: Option<bool>,
    // FTP-Ersetzen: alte Fassung vorher auf diesen Computer sichern (Standard); false beim Zurückholen.
    backup: Option<bool>,
    on_progress: Channel<sftp::Progress>,
) -> Result<String, String> {
    let s = files_get(&files, &sid).await?;
    let tid = new_sid("t");
    let (map, key) = (transfers.0.clone(), tid.clone());
    let mut running = transfers.0.lock().unwrap();
    let pause = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let task = tauri::async_runtime::spawn(sftp::PAUSE.scope(pause.clone(), async move {
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
            // FTP: «Ersetzen» gibt es nicht, es bleibt bei «Name (2)».
            (FileSession::Ftp(s), true) if replace => ftp::upload_replace(s, local, remote, backup.unwrap_or(true), report).await,
            (FileSession::Ftp(s), true) => ftp::upload(s, local, remote, report).await,
            (FileSession::Ftp(s), false) => ftp::download(s, remote, local, replace, report).await,
        };
        if let Err(error) = r {
            let _ = on_progress.send(sftp::Progress::Failed { error });
        }
        map.lock().unwrap().remove(&key);
    }));
    running.insert(tid.clone(), (task, pause));
    Ok(tid)
}

#[tauri::command]
pub(crate) fn transfer_cancel(transfers: State<Transfers>, tid: String) {
    if let Some((t, _)) = transfers.0.lock().unwrap().remove(&tid) {
        t.abort();
    }
}

/// Anhalten bzw. weiterlaufen lassen; die Übertragung setzt an derselben Stelle fort (Verbindung bleibt offen).
#[tauri::command]
pub(crate) fn transfer_pause(transfers: State<Transfers>, tid: String, paused: bool) -> Result<(), String> {
    let map = transfers.0.lock().unwrap();
    let (_, p) = map.get(&tid).ok_or("Übertragung läuft nicht mehr")?;
    p.store(paused, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}
