//! Automatische Updates (docs/release.md): still im Hintergrund prüfen und herunterladen, beim Beenden
//! installieren. Signatur ed25519 (minisign) und HTTPS sind Pflicht (Tauri-Updater, `plugins.updater` in
//! tauri.conf.json); nur neuere Versionen, deren Signatur die Version selbst enthält (kein Downgrade).
//!
//! Wie installiert wird, hängt von der Art der Installation ab:
//! - macOS-App, AppImage, portable Fassungen: still beim Beenden, nächster Start ist neu.
//! - Windows-MSI (Programme-Ordner, braucht Administratorrechte): nur anzeigen; «Jetzt installieren» fragt einmal
//!   nach Rechten. Mit der Richtlinie `HKLM\Software\Policies\Sesam Remote\AutoUpdate = 0` (MSI-Eigenschaft
//!   `AUTOUPDATE=0`, docs/intune.md) ganz aus, Intune verteilt die Versionen.
//! - Linux-Paket (.deb): nur anzeigen, die Paketverwaltung aktualisiert.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

/// Wie lange zwischen zwei Prüfungen im Hintergrund.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// Erste Prüfung nach dem Start (die App soll erst in Ruhe starten).
const FIRST: Duration = Duration::from_secs(45);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Install {
    /// Still beim Beenden.
    OnQuit,
    /// Braucht Rechte oder eine Paketverwaltung: nur melden, auf Wunsch «Jetzt installieren».
    Manual,
    /// Gar nicht (iPad: App Store; Richtlinie aus).
    Never,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub current: String,
    /// «Automatisch aktualisieren» (Einstellung des Benutzers).
    pub auto: bool,
    /// Von einer Richtlinie festgelegt (Windows, Intune): Schalter nicht änderbar.
    pub locked: bool,
    pub checking: bool,
    /// Neue Version, heruntergeladen und geprüft (OnQuit) bzw. verfügbar (Manual).
    pub ready: Option<String>,
    pub install: Option<Install>,
    pub error: Option<String>,
    /// Zeitpunkt der letzten Prüfung (Unix-Sekunden).
    pub checked: Option<i64>,
}

#[derive(Default)]
pub struct State {
    status: Mutex<Status>,
    /// Heruntergeladen und signaturgeprüft, wird beim Beenden installiert.
    pending: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
    busy: tokio::sync::Mutex<()>,
}

/// Einstellung des Benutzers neben der Bibliothek (`update.json`), Vorgabe: an.
fn settings_file() -> Option<PathBuf> {
    crate::LIBRARY_FILE.get().and_then(|p| p.parent()).map(|d| d.join("update.json"))
}

fn user_auto() -> bool {
    settings_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("auto").and_then(|a| a.as_bool()))
        .unwrap_or(true)
}

/// Richtlinie des Computers: `Some(false)` = aus (Intune), `Some(true)` = erzwungen an, `None` = keine.
pub fn policy() -> Option<bool> {
    #[cfg(windows)]
    {
        // Voller Pfad: Windows sucht Programme zuerst im Ordner der App (portabel beschreibbar).
        let reg = std::path::Path::new(&std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32").join("reg.exe");
        let out = std::process::Command::new(reg)
            .args(["query", r"HKLM\Software\Policies\Sesam Remote", "/v", "AutoUpdate"])
            .output()
            .ok()?;
        return parse_reg_dword(&String::from_utf8_lossy(&out.stdout)).map(|v| v != 0);
    }
    #[allow(unreachable_code)]
    None
}

#[cfg_attr(not(windows), allow(dead_code))]
/// `reg query`-Ausgabe → Wert von AutoUpdate (REG_DWORD, hexadezimal).
pub fn parse_reg_dword(out: &str) -> Option<u32> {
    let line = out.lines().find(|l| l.trim_start().to_ascii_lowercase().starts_with("autoupdate"))?;
    let v = line.split_whitespace().last()?;
    u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()
}

/// Wie diese Installation aktualisiert werden kann.
pub fn install_kind(exe: &Path, portable: bool, appimage: bool) -> Install {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return Install::Never;
    }
    if portable || appimage {
        return Install::OnQuit;
    }
    if cfg!(target_os = "macos") {
        // Das .app-Bündel muss ersetzt werden können (Ordner beschreibbar), sonst nur melden.
        let bundle_dir = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).and_then(Path::parent);
        return match bundle_dir {
            Some(d) if writable(d) => Install::OnQuit,
            _ => Install::Manual,
        };
    }
    // Windows-MSI und Linux-Pakete brauchen Rechte bzw. die Paketverwaltung.
    Install::Manual
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".sesam-update-{}", std::process::id()));
    let ok = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Aktiv, wenn Richtlinie es nicht verbietet und der Benutzer es nicht ausgeschaltet hat.
pub fn effective_auto(policy: Option<bool>, user: bool) -> bool {
    policy.unwrap_or(user)
}

/// Windows-Portable: eigenes Ziel in latest.json (die ausführbare Datei selbst statt eines Installers).
fn target() -> Option<String> {
    if cfg!(windows) && crate::portable::data_dir().is_some() {
        return Some("windows-x86_64-portable".into());
    }
    None
}

fn kind() -> Install {
    let exe = std::env::current_exe().unwrap_or_default();
    install_kind(&exe, crate::portable::data_dir().is_some(), std::env::var_os("APPIMAGE").is_some())
}

pub fn status<R: Runtime>(app: &AppHandle<R>) -> Status {
    let st = app.state::<State>();
    let mut s = st.status.lock().unwrap().clone();
    s.current = app.package_info().version.to_string();
    let p = policy();
    s.locked = p.is_some();
    s.auto = effective_auto(p, user_auto());
    s
}

pub fn set_auto(auto: bool) -> Result<(), String> {
    if policy().is_some() {
        return Err("Von der Richtlinie dieses Computers festgelegt".into());
    }
    let f = settings_file().ok_or("Datenordner unbekannt")?;
    std::fs::write(f, serde_json::json!({ "auto": auto }).to_string()).map_err(|e| e.to_string())
}

/// Einmal prüfen und (wenn still installierbar) herunterladen. `manual`: auch bei ausgeschaltetem Automatismus.
pub async fn check<R: Runtime>(app: &AppHandle<R>, manual: bool) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let st = app.state::<State>();
    let Ok(_busy) = st.busy.try_lock() else { return Ok(()) };
    let kind = kind();
    if kind == Install::Never || (!manual && !effective_auto(policy(), user_auto())) || (manual && policy() == Some(false)) {
        return Ok(());
    }
    st.status.lock().unwrap().checking = true;
    let r: Result<(), String> = async {
        let mut b = app.updater_builder();
        if let Some(t) = target() {
            b = b.target(t);
        }
        let updater = b.build().map_err(|e| e.to_string())?;
        let found = updater.check().await.map_err(|e| e.to_string())?;
        let mut s = st.status.lock().unwrap().clone();
        s.checked = Some(chrono::Utc::now().timestamp());
        s.error = None;
        s.install = Some(kind);
        match found {
            None => s.ready = None,
            Some(u) if kind == Install::OnQuit => {
                if st.pending.lock().unwrap().as_ref().is_some_and(|(p, _)| p.version == u.version) {
                    s.ready = Some(u.version.clone());
                } else {
                    // download() prüft die Signatur, bevor es die Daten zurückgibt.
                    let bytes = u.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
                    s.ready = Some(u.version.clone());
                    tracing::info!(version = %u.version, "Update heruntergeladen, wird beim Beenden installiert");
                    *st.pending.lock().unwrap() = Some((u, bytes));
                }
            }
            Some(u) => {
                s.ready = Some(u.version.clone());
                *st.pending.lock().unwrap() = Some((u, Vec::new()));
            }
        }
        *st.status.lock().unwrap() = s;
        Ok(())
    }
    .await;
    let mut s = st.status.lock().unwrap();
    s.checking = false;
    if let Err(e) = &r {
        tracing::warn!(error = %e, "Update-Prüfung gescheitert");
        s.error = Some(e.clone());
    }
    r
}

/// Prüfschleife im Hintergrund.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST).await;
        loop {
            let _ = check(&app, false).await;
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// Beim Beenden: heruntergeladenes Update still installieren (nur OnQuit).
pub fn install_on_quit<R: Runtime>(app: &AppHandle<R>) {
    let Some(st) = app.try_state::<State>() else { return };
    let Some((u, bytes)) = st.pending.lock().unwrap().take() else { return };
    if bytes.is_empty() || kind() != Install::OnQuit || !effective_auto(policy(), user_auto()) {
        return;
    }
    let r = if target().is_some() { replace_exe(&bytes) } else { u.install(&bytes).map_err(|e| e.to_string()) };
    match r {
        Ok(()) => tracing::info!(version = %u.version, "Update installiert"),
        Err(e) => tracing::warn!(error = %e, "Update konnte nicht installiert werden"),
    }
}

/// Auf Wunsch sofort (MSI: fragt nach Administratorrechten, beendet die App).
pub async fn install_now<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let st = app.state::<State>();
    let Some((u, bytes)) = st.pending.lock().unwrap().take() else { return Err("Kein Update bereit".into()) };
    let bytes = if bytes.is_empty() {
        u.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?
    } else {
        bytes
    };
    if target().is_some() {
        replace_exe(&bytes)?;
        app.restart();
    }
    u.install(&bytes).map_err(|e| e.to_string())?;
    app.restart();
}

/// Windows-Portable: laufende Datei umbenennen (geht unter Windows), neue an ihren Platz; die alte räumt der
/// nächste Start weg.
fn replace_exe(bytes: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = exe.with_extension("alt");
    let new = exe.with_extension("neu");
    std::fs::write(&new, bytes).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(e.to_string());
    }
    Ok(())
}

/// Reste eines früheren Updates (Windows-Portable) wegräumen.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("alt"));
        let _ = std::fs::remove_file(exe.with_extension("neu"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn richtlinie_aus_der_registry() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\Software\\Policies\\Sesam Remote\r\n    AutoUpdate    REG_DWORD    0x0\r\n\r\n";
        assert_eq!(parse_reg_dword(out), Some(0));
        assert_eq!(parse_reg_dword("    AutoUpdate    REG_DWORD    0x1\n"), Some(1));
        assert_eq!(parse_reg_dword("FEHLER: Der angegebene Registrierungsschlüssel wurde nicht gefunden."), None);
    }

    #[test]
    fn richtlinie_vor_benutzer() {
        assert!(!effective_auto(Some(false), true), "Intune: aus, auch wenn der Benutzer an hat");
        assert!(effective_auto(Some(true), false));
        assert!(effective_auto(None, true) && !effective_auto(None, false));
    }

    #[test]
    fn art_der_installation() {
        let tmp = std::env::temp_dir().join(format!("sesam-upd-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("Sesam Remote.app/Contents/MacOS")).unwrap();
        let exe = tmp.join("Sesam Remote.app/Contents/MacOS/sesam-remote");
        assert_eq!(install_kind(&exe, true, false), Install::OnQuit, "portabel");
        assert_eq!(install_kind(&exe, false, true), Install::OnQuit, "AppImage");
        #[cfg(target_os = "macos")]
        assert_eq!(install_kind(&exe, false, false), Install::OnQuit, "beschreibbarer Ordner");
        #[cfg(target_os = "macos")]
        assert_eq!(install_kind(Path::new("/System/Applications/X.app/Contents/MacOS/x"), false, false), Install::Manual);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(install_kind(Path::new("C:/Program Files/Sesam Remote/sesam-remote.exe"), false, false), Install::Manual);
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
