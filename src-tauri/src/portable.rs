//! Portabler Betrieb (docs/portabel.md): Liegt neben der App eine Datei `portable` oder ein Ordner `Sesam-Daten`,
//! legt Sesam Bibliothek, Einstellungen der Dateien, Protokolle und gemerkte Zertifikate in `Sesam-Daten` ab statt
//! im Datenordner des Benutzers. Zugangsdaten bleiben im Schlüsselbund des Systems.

use std::path::{Path, PathBuf};

pub const FOLDER: &str = "Sesam-Daten";
pub const MARKER: &str = "portable";

/// Ordner, neben dem die Marke liegen muss: bei einer Mac-App der Ordner mit `Sesam Remote.app`, sonst der
/// Ordner mit der ausführbaren Datei (Windows-ZIP, AppImage: dort `$APPIMAGE`, weil die Datei selbst eingehängt läuft).
fn base(exe: &Path, appimage: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(a) = appimage {
        return a.parent().map(Path::to_path_buf);
    }
    let dir = exe.parent()?;
    // …/Sesam Remote.app/Contents/MacOS/sesam-remote
    if dir.ends_with("Contents/MacOS") {
        return dir.parent()?.parent()?.parent().map(Path::to_path_buf);
    }
    Some(dir.to_path_buf())
}

/// Datenordner im portablen Betrieb, sonst `None`.
pub fn data_dir_for(exe: &Path, appimage: Option<PathBuf>, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let b = base(exe, appimage)?;
    let data = b.join(FOLDER);
    (exists(&data) || exists(&b.join(MARKER))).then_some(data)
}

/// Einmal beim Start bestimmt; legt den Ordner an.
pub fn data_dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        if cfg!(any(target_os = "ios", target_os = "android")) {
            return None;
        }
        let exe = std::env::current_exe().ok()?;
        let d = data_dir_for(&exe, std::env::var_os("APPIMAGE").map(PathBuf::from), |p| p.exists())?;
        std::fs::create_dir_all(&d).ok()?;
        Some(d)
    })
    .clone()
}

/// Vor dem Start der Webview: deren Speicher (Einstellungen der Oberfläche) ebenfalls nach `Sesam-Daten`.
/// Windows: WebView2 liest `WEBVIEW2_USER_DATA_FOLDER`; Linux: WebKitGTK folgt den XDG-Ordnern. macOS: WKWebView
/// lässt sich nicht verlegen, die Einstellungen der Oberfläche bleiben dort im Benutzerprofil.
pub fn prepare_webview() {
    let Some(d) = data_dir() else { return };
    #[cfg(windows)]
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", d.join("webview"));
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("XDG_DATA_HOME", d.join("xdg-data"));
        std::env::set_var("XDG_CACHE_HOME", d.join("xdg-cache"));
    }
    let _ = d;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with<'a>(existing: &'a [&'a str]) -> impl Fn(&Path) -> bool + 'a {
        move |p| existing.iter().any(|e| p == Path::new(e))
    }

    #[test]
    fn windows_zip_mit_marke() {
        let exe = Path::new("/usb/Sesam/sesam-remote.exe");
        assert_eq!(data_dir_for(exe, None, with(&["/usb/Sesam/portable"])), Some(PathBuf::from("/usb/Sesam/Sesam-Daten")));
        assert_eq!(data_dir_for(exe, None, with(&["/usb/Sesam/Sesam-Daten"])), Some(PathBuf::from("/usb/Sesam/Sesam-Daten")));
        assert_eq!(data_dir_for(exe, None, with(&[])), None);
    }

    #[test]
    fn mac_app_schaut_neben_das_buendel() {
        let exe = Path::new("/Volumes/Stick/Sesam Remote.app/Contents/MacOS/sesam-remote");
        assert_eq!(data_dir_for(exe, None, with(&["/Volumes/Stick/portable"])), Some(PathBuf::from("/Volumes/Stick/Sesam-Daten")));
        // Eine Marke im Bündel zählt nicht (würde beim Signieren stören)
        assert_eq!(data_dir_for(exe, None, with(&["/Volumes/Stick/Sesam Remote.app/Contents/MacOS/portable"])), None);
    }

    #[test]
    fn appimage_schaut_neben_die_datei() {
        let exe = Path::new("/tmp/.mount_SesamX/usr/bin/sesam-remote");
        let img = Some(PathBuf::from("/home/m/Apps/Sesam.AppImage"));
        assert_eq!(data_dir_for(exe, img, with(&["/home/m/Apps/Sesam-Daten"])), Some(PathBuf::from("/home/m/Apps/Sesam-Daten")));
    }
}
