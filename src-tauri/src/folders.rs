//! iPad: Ordner aus der App «Dateien» für die linke Seite des Dateifensters (Swift-Teil: SesamFolders.swift).
//! Die Oberfläche merkt sich die Lesezeichen; hier nur Auswahl, Öffnen (Zugriff starten) und welche Ordner frei sind.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Folder {
    pub path: String,
    /// Lesezeichen (Base64); nach dem Öffnen ggf. erneuert, die Oberfläche speichert dann dieses.
    pub bookmark: String,
}

/// Freigegebene Ordner (kanonisch). Nur darin darf die linke Seite auf dem iPad lesen und schreiben.
static ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))] // Ordner aus «Dateien» gibt es nur auf dem iPad
pub fn add_root(p: &Path) {
    let c = canon(p);
    let mut r = ROOTS.lock().unwrap();
    if !r.contains(&c) {
        r.push(c);
    }
}

pub fn remove_root(p: &Path) {
    let c = canon(p);
    ROOTS.lock().unwrap().retain(|r| *r != c);
}

/// Liegt `p` in einem freigegebenen Ordner? `Some(true)`, wenn `p` selbst ein freigegebener Ordner ist.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub fn inside(p: &Path) -> Option<bool> {
    let c = canon(p);
    ROOTS.lock().unwrap().iter().find(|r| c.starts_with(r)).map(|r| *r == c)
}

#[cfg(target_os = "ios")]
mod ios {
    //! UIDocumentPickerViewController und Lesezeichen über die Objective-C-Schnittstelle (objc2), ohne Swift-Teil.
    use super::Folder;
    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{class, msg_send, sel};
    use objc2_foundation::NSString;
    use std::sync::Mutex;

    #[link(name = "UniformTypeIdentifiers", kind = "framework")]
    unsafe extern "C" {}

    static PICK: Mutex<Option<tokio::sync::oneshot::Sender<Option<Folder>>>> = Mutex::new(None);
    /// Geöffnete Ordner (Pfad → NSURL, behalten), damit stopAccessing… dieselbe URL bekommt.
    static OPEN: Mutex<Vec<(String, usize)>> = Mutex::new(Vec::new());
    static DELEGATE: Mutex<usize> = Mutex::new(0);

    unsafe fn string(s: *mut AnyObject) -> Option<String> {
        unsafe { s.cast::<NSString>().as_ref() }.map(|s| s.to_string())
    }

    /// Zugriff starten (einmal je Pfad) und die URL behalten.
    unsafe fn keep(url: *mut AnyObject) -> Option<String> {
        unsafe {
            let path = string(msg_send![url, path])?;
            let mut open = OPEN.lock().unwrap();
            if !open.iter().any(|(p, _)| *p == path) {
                let ok: Bool = msg_send![url, startAccessingSecurityScopedResource];
                if ok.as_bool() {
                    let _: *mut AnyObject = msg_send![url, retain];
                    open.push((path.clone(), url as usize));
                }
            }
            Some(path)
        }
    }

    unsafe fn bookmark(url: *mut AnyObject) -> Option<String> {
        unsafe {
            let data: *mut AnyObject = msg_send![url, bookmarkDataWithOptions: 0usize, includingResourceValuesForKeys: std::ptr::null_mut::<AnyObject>(), relativeToURL: std::ptr::null_mut::<AnyObject>(), error: std::ptr::null_mut::<*mut AnyObject>()];
            if data.is_null() {
                return None;
            }
            string(msg_send![data, base64EncodedStringWithOptions: 0usize])
        }
    }

    fn finish(f: Option<Folder>) {
        if let Some(tx) = PICK.lock().unwrap().take() {
            let _ = tx.send(f);
        }
    }

    extern "C-unwind" fn did_pick(_this: *mut AnyObject, _cmd: Sel, _picker: *mut AnyObject, urls: *mut AnyObject) {
        let f = unsafe {
            let url: *mut AnyObject = msg_send![urls, firstObject];
            if url.is_null() { None } else { keep(url).zip(bookmark(url)).map(|(path, bookmark)| Folder { path, bookmark }) }
        };
        finish(f);
    }

    extern "C-unwind" fn cancelled(_this: *mut AnyObject, _cmd: Sel, _picker: *mut AnyObject) {
        finish(None);
    }

    /// Delegate-Klasse einmal anlegen; die Instanz bleibt (der Picker hält sie nur schwach).
    unsafe fn delegate() -> *mut AnyObject {
        let mut d = DELEGATE.lock().unwrap();
        if *d == 0 {
            unsafe {
                let Some(b) = ClassBuilder::new(c"SesamFolderPickerDelegate", class!(NSObject)) else { return std::ptr::null_mut() };
                let cls = b.register() as *const AnyClass as *mut AnyClass;
                let pick: objc2::runtime::Imp = std::mem::transmute(did_pick as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject));
                let cancel: objc2::runtime::Imp = std::mem::transmute(cancelled as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject));
                objc2::ffi::class_addMethod(cls, sel!(documentPicker:didPickDocumentsAtURLs:), pick, c"v@:@@".as_ptr());
                objc2::ffi::class_addMethod(cls, sel!(documentPickerWasCancelled:), cancel, c"v@:@".as_ptr());
                let obj: *mut AnyObject = msg_send![&*cls, new];
                *d = obj as usize;
            }
        }
        *d as *mut AnyObject
    }

    /// Auf dem Hauptthread aufrufen (`presenter` = View-Controller, der die Auswahl zeigt).
    pub unsafe fn present(presenter: *mut AnyObject, tx: tokio::sync::oneshot::Sender<Option<Folder>>) {
        *PICK.lock().unwrap() = Some(tx);
        unsafe {
            let folder: *mut AnyObject = msg_send![class!(UTType), typeWithIdentifier: &*NSString::from_str("public.folder")];
            let types: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: folder];
            let picker: *mut AnyObject = msg_send![class!(UIDocumentPickerViewController), alloc];
            let picker: *mut AnyObject = msg_send![picker, initForOpeningContentTypes: types];
            let _: () = msg_send![picker, setDelegate: delegate()];
            let _: () = msg_send![picker, setAllowsMultipleSelection: Bool::NO];
            let mut top = presenter;
            loop {
                let next: *mut AnyObject = msg_send![top, presentedViewController];
                if next.is_null() {
                    break;
                }
                top = next;
            }
            let desc: *mut AnyObject = msg_send![top, description];
            tracing::info!(folder = !folder.is_null(), picker = !picker.is_null(), top = %string(desc).unwrap_or_default(), "Ordnerauswahl");
            let _: () = msg_send![top, presentViewController: picker, animated: Bool::YES, completion: std::ptr::null_mut::<AnyObject>()];
            let _: () = msg_send![picker, release];
        }
    }

    pub fn open(b64: &str) -> Result<Folder, String> {
        const GONE: &str = "Dieser Ort ist nicht mehr erreichbar (gelöscht, ausgeworfen oder Zugriff entzogen).";
        unsafe {
            let data: *mut AnyObject = msg_send![class!(NSData), alloc];
            let data: *mut AnyObject = msg_send![data, initWithBase64EncodedString: &*NSString::from_str(b64), options: 0usize];
            if data.is_null() {
                return Err(GONE.into());
            }
            let mut stale = Bool::NO;
            let url: *mut AnyObject = msg_send![class!(NSURL), URLByResolvingBookmarkData: data, options: 0usize, relativeToURL: std::ptr::null_mut::<AnyObject>(), bookmarkDataIsStale: &mut stale as *mut Bool, error: std::ptr::null_mut::<*mut AnyObject>()];
            let _: () = msg_send![data, release];
            if url.is_null() {
                return Err(GONE.into());
            }
            let path = keep(url).ok_or(GONE)?;
            let bookmark = if stale.as_bool() { bookmark(url).unwrap_or_else(|| b64.to_string()) } else { b64.to_string() };
            Ok(Folder { path, bookmark })
        }
    }

    pub fn close(path: &str) {
        let mut open = OPEN.lock().unwrap();
        if let Some(i) = open.iter().position(|(p, _)| p == path) {
            let (_, url) = open.remove(i);
            unsafe {
                let url = url as *mut AnyObject;
                let _: () = msg_send![url, stopAccessingSecurityScopedResource];
                let _: () = msg_send![url, release];
            }
        }
    }
}

/// Ordnerauswahl zeigen (iPad). `window`: Fenster, über dem sie erscheint.
pub async fn pick(window: tauri::WebviewWindow) -> Result<Option<Folder>, String> {
    #[cfg(target_os = "ios")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        window
            .with_webview(move |w| unsafe {
                use objc2::msg_send;
                use objc2::runtime::AnyObject;
                let web = w.inner() as *mut AnyObject;
                let win: *mut AnyObject = msg_send![web, window];
                let vc: *mut AnyObject = if win.is_null() { std::ptr::null_mut() } else { msg_send![win, rootViewController] };
                if !vc.is_null() {
                    ios::present(vc, tx);
                }
            })
            .map_err(|e| e.to_string())?;
        let f = rx.await.map_err(|_| "Ordnerauswahl nicht verfügbar".to_string())?;
        if let Some(f) = &f {
            add_root(Path::new(&f.path));
        }
        return Ok(f);
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = window;
        Err("Ordner aus «Dateien» gibt es nur auf dem iPad".into())
    }
}

pub fn open(bookmark: &str) -> Result<Folder, String> {
    #[cfg(target_os = "ios")]
    {
        let f = ios::open(bookmark)?;
        add_root(Path::new(&f.path));
        return Ok(f);
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = bookmark;
        Err("Ordner aus «Dateien» gibt es nur auf dem iPad".into())
    }
}

pub fn close(path: &str) {
    remove_root(Path::new(path));
    #[cfg(target_os = "ios")]
    ios::close(path);
}

/// «Dateien wählen …»: gewählte Dateien in den Ordner `dir` kopieren (gleiche Namen bekommen eine Nummer).
pub fn import(paths: &[String], dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for p in paths {
        let src = Path::new(p);
        let name = src.file_name().ok_or("Ungültige Datei")?.to_string_lossy().into_owned();
        let target = crate::sftp::local_unique(dir, &name, false);
        std::fs::copy(src, &target).map_err(|e| format!("{name}: {e}"))?;
        out.push(target.to_string_lossy().into_owned());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn freigegebene_ordner() {
        let base = std::env::temp_dir().join(format!("sesam-roots-{}", std::process::id()));
        std::fs::create_dir_all(base.join("unter")).unwrap();
        assert_eq!(super::inside(&base), None);
        super::add_root(&base);
        assert_eq!(super::inside(&base), Some(true));
        assert_eq!(super::inside(&base.join("unter")), Some(false));
        assert_eq!(super::inside(&std::env::temp_dir()), None, "darüber nicht");
        std::fs::write(base.join("a.txt"), "x").unwrap();
        let got = super::import(&[base.join("a.txt").to_string_lossy().into()], &base.join("unter")).unwrap();
        assert!(got[0].ends_with("a.txt"));
        let again = super::import(&[base.join("a.txt").to_string_lossy().into()], &base.join("unter")).unwrap();
        assert_ne!(again[0], got[0], "zweites Mal mit Nummer");
        super::remove_root(&base);
        assert_eq!(super::inside(&base), None);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
