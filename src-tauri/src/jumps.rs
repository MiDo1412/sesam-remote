//! Zuletzt verwendete Verbindungen am App-Symbol: Dock-Menü (macOS), Sprungliste (Windows) und Daten für das
//! Widget (iPad, nur mit App Group). Jeder Eintrag ist ein Link wie bei den Verknüpfungen: sesam://open?id=…,
//! dazu sesam://new und sesam://quick. Keine Zugangsdaten, nur ID und Name.

use crate::library::Connection;
use std::path::Path;

pub const MAX: usize = 8;
pub const NEW: (&str, &str) = ("Neue Verbindung …", "sesam://new");
pub const QUICK: (&str, &str) = ("Schnelle Verbindung …", "sesam://quick");

pub fn open_url(id: &str) -> String {
    // IDs sind UUIDs; sicherheitshalber alles ausser [A-Za-z0-9-_] prozentkodieren.
    let id: String = id.bytes().map(|b| if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    format!("sesam://open?id={id}")
}

/// Zuletzt verbundene zuerst (RFC 3339 sortiert sich als Text richtig), höchstens `MAX`.
pub fn recent(items: &[Connection]) -> Vec<&Connection> {
    let mut v: Vec<&Connection> = items.iter().filter(|c| c.last_used.is_some()).collect();
    v.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    v.truncate(MAX);
    v
}

/// Nach jedem Speichern der Bibliothek (library::save): Sprungliste bzw. Widget-Daten nachführen.
pub fn changed(path: &Path) {
    if path.file_name().is_none_or(|n| n != "library.json") {
        return;
    }
    #[cfg(windows)]
    win::refresh(path.to_path_buf());
    #[cfg(target_os = "ios")]
    widget::export(path);
    #[cfg(not(any(windows, target_os = "ios")))]
    let _ = path;
}

// ---------- macOS: Dock-Menü ----------

#[cfg(target_os = "macos")]
pub mod dock {
    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
    use objc2::{class, msg_send, sel};
    use objc2_foundation::NSString;
    use std::sync::OnceLock;
    use tauri::{AppHandle, Emitter, Manager};

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static TARGET: OnceLock<usize> = OnceLock::new();

    /// Klick auf einen Eintrag: Fenster nach vorne, Link an die Oberfläche (wie ein sesam://-Link).
    extern "C-unwind" fn clicked(_this: *mut AnyObject, _cmd: Sel, item: *mut AnyObject) {
        let url: Option<String> = unsafe {
            let o: *mut AnyObject = msg_send![item, representedObject];
            o.cast::<NSString>().as_ref().map(|s| s.to_string())
        };
        if let (Some(app), Some(url)) = (APP.get(), url) {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            let _ = app.emit_to("main", "link", url);
        }
    }

    unsafe fn item(menu: *mut AnyObject, title: &str, url: &str) {
        unsafe {
            let it: *mut AnyObject = msg_send![class!(NSMenuItem), alloc];
            let it: *mut AnyObject = msg_send![it, initWithTitle: &*NSString::from_str(title), action: sel!(sesamOpen:), keyEquivalent: &*NSString::from_str("")];
            let _: () = msg_send![it, setTarget: *TARGET.get().unwrap() as *mut AnyObject];
            let _: () = msg_send![it, setRepresentedObject: &*NSString::from_str(url)];
            let _: () = msg_send![menu, addItem: it];
            let _: () = msg_send![it, release];
        }
    }

    /// NSApplicationDelegate applicationDockMenu: – das Dock fragt bei jedem Rechtsklick neu, also immer aktuell.
    extern "C-unwind" fn dock_menu(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject) -> *mut AnyObject {
        let items = crate::LIBRARY_FILE.get().and_then(|p| crate::library::load(p).ok()).unwrap_or_default();
        unsafe {
            let menu: *mut AnyObject = msg_send![class!(NSMenu), new];
            for c in super::recent(&items) {
                item(menu, &c.name, &super::open_url(&c.id));
            }
            if !super::recent(&items).is_empty() {
                let sep: *mut AnyObject = msg_send![class!(NSMenuItem), separatorItem];
                let _: () = msg_send![menu, addItem: sep];
            }
            item(menu, super::NEW.0, super::NEW.1);
            item(menu, super::QUICK.0, super::QUICK.1);
            msg_send![menu, autorelease]
        }
    }

    /// Nach dem Start (Hauptthread): Ziel-Klasse anlegen und dem App-Delegate von Tao das Dock-Menü beibringen.
    pub fn install(app: AppHandle) {
        let _ = APP.set(app);
        install_on_delegate();
    }

    #[cfg(test)]
    pub fn install_for_test() {
        install_on_delegate();
    }

    fn install_on_delegate() {
        unsafe {
            let Some(b) = ClassBuilder::new(c"SesamDockTarget", class!(NSObject)) else { return };
            let cls = b.register() as *const AnyClass as *mut AnyClass;
            let imp: objc2::runtime::Imp = std::mem::transmute(clicked as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject));
            objc2::ffi::class_addMethod(cls, sel!(sesamOpen:), imp, c"v@:@".as_ptr());
            let cls = &*cls;
            let target: *mut AnyObject = msg_send![cls, new]; // bleibt bis zum Ende (NSMenuItem hält das Ziel nicht)
            let _ = TARGET.set(target as usize);
            let nsapp: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let delegate: *mut AnyObject = msg_send![nsapp, delegate];
            let Some(delegate) = delegate.as_ref() else { return };
            let cls = delegate.class() as *const AnyClass as *mut AnyClass;
            let imp: objc2::runtime::Imp = std::mem::transmute(dock_menu as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> *mut AnyObject);
            objc2::ffi::class_addMethod(cls, sel!(applicationDockMenu:), imp, c"@@:@".as_ptr());
        }
    }
}

// ---------- Windows: Sprungliste am Taskleisten-Symbol ----------
// shortcut: auf diesem Mac weder gebaut noch getestet (kein Windows-Ziel installiert); auf einem Windows-Rechner prüfen.

#[cfg(windows)]
mod win {
    use std::path::PathBuf;
    use windows::core::{Interface, HSTRING, PWSTR};
    use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
    use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, SHStrDupW, ShellLink};

    pub fn refresh(path: PathBuf) {
        std::thread::spawn(move || unsafe {
            if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
                return;
            }
            if let Err(e) = build(&path) {
                tracing::warn!("Sprungliste: {e}");
            }
            CoUninitialize();
        });
    }

    unsafe fn link(exe: &HSTRING, title: &str, url: &str) -> windows::core::Result<IShellLinkW> {
        unsafe {
            let l: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            l.SetPath(exe)?;
            l.SetArguments(&HSTRING::from(url))?;
            l.SetIconLocation(exe, 0)?;
            l.SetDescription(&HSTRING::from(title))?;
            let store: IPropertyStore = l.cast()?;
            let s: PWSTR = SHStrDupW(&HSTRING::from(title))?;
            let mut v = PROPVARIANT::default();
            v.Anonymous = PROPVARIANT_0 { Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 { vt: VT_LPWSTR, wReserved1: 0, wReserved2: 0, wReserved3: 0, Anonymous: PROPVARIANT_0_0_0 { pwszVal: s } }) };
            store.SetValue(&PKEY_Title, &v)?;
            store.Commit()?;
            Ok(l)
        }
    }

    unsafe fn build(path: &std::path::Path) -> windows::core::Result<()> {
        unsafe {
            let items = crate::library::load(path).unwrap_or_default();
            let exe = HSTRING::from(std::env::current_exe().unwrap_or_default().as_os_str());
            let list: ICustomDestinationList = CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
            let mut slots = 0u32;
            let _removed: IObjectArray = list.BeginList(&mut slots)?;
            let recent: IObjectCollection = CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
            for c in super::recent(&items).into_iter().take(slots.max(1) as usize) {
                recent.AddObject(&link(&exe, &c.name, &super::open_url(&c.id))?)?;
            }
            if !super::recent(&items).is_empty() {
                list.AppendCategory(&HSTRING::from("Zuletzt verwendet"), &recent.cast::<IObjectArray>()?)?;
            }
            let tasks: IObjectCollection = CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
            for (t, u) in [super::NEW, super::QUICK] {
                tasks.AddObject(&link(&exe, t, u)?)?;
            }
            list.AddUserTasks(&tasks.cast::<IObjectArray>()?)?;
            list.CommitList()
        }
    }
}

// ---------- iPad: Daten für das Widget ----------
// Nur mit App Group (bezahltes Entwicklerprogramm): ohne Berechtigung liefert iOS keinen Ordner, dann passiert nichts.

#[cfg(target_os = "ios")]
mod widget {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    pub const GROUP: &str = "group.ch.sesam.remote";

    pub fn export(path: &std::path::Path) {
        let dir: Option<String> = unsafe {
            let fm: *mut AnyObject = msg_send![class!(NSFileManager), defaultManager];
            let url: *mut AnyObject = msg_send![fm, containerURLForSecurityApplicationGroupIdentifier: &*NSString::from_str(GROUP)];
            if url.is_null() {
                None
            } else {
                let p: *mut AnyObject = msg_send![url, path];
                p.cast::<NSString>().as_ref().map(|s| s.to_string())
            }
        };
        let Some(dir) = dir else { return };
        let items = crate::library::load(path).unwrap_or_default();
        let list: Vec<serde_json::Value> = super::recent(&items).iter().map(|c| serde_json::json!({ "id": c.id, "name": c.name, "url": super::open_url(&c.id) })).collect();
        let _ = std::fs::write(std::path::Path::new(&dir).join("widget.json"), serde_json::to_vec(&list).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dock-Menü ohne laufende App: eigener Delegate, Menü abfragen, Einträge prüfen.
    #[cfg(target_os = "macos")]
    #[test]
    fn dock_menue() {
        use objc2::runtime::{AnyObject, ClassBuilder};
        use objc2::{class, msg_send, sel};
        use objc2_foundation::NSString;
        unsafe {
            let cls = ClassBuilder::new(c"SesamTestDelegate", class!(NSObject)).unwrap().register();
            let d: *mut AnyObject = msg_send![cls, new];
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, setDelegate: d];
            dock::install_for_test();
            let menu: *mut AnyObject = msg_send![d, applicationDockMenu: app];
            let n: isize = msg_send![menu, numberOfItems];
            assert!(n >= 2);
            let last: *mut AnyObject = msg_send![menu, itemAtIndex: n - 1];
            let title: *mut NSString = msg_send![last, title];
            assert_eq!((*title).to_string(), "Schnelle Verbindung …");
            let obj: *mut NSString = msg_send![last, representedObject];
            assert_eq!((*obj).to_string(), "sesam://quick");
            let target: *mut AnyObject = msg_send![last, target];
            let responds: bool = msg_send![target, respondsToSelector: sel!(sesamOpen:)];
            assert!(responds);
            let _: () = msg_send![target, sesamOpen: last]; // ohne App: tut nichts, darf nicht abstürzen
        }
    }

    #[test]
    fn zuletzt_verwendet_zuerst() {
        let c = |id: &str, last: Option<&str>| Connection { id: id.into(), name: id.into(), last_used: last.map(Into::into), ..Default::default() };
        let items = [c("a", Some("2026-11-01T10:00:00Z")), c("b", None), c("c", Some("2026-11-11T09:00:00Z"))];
        let ids: Vec<&str> = recent(&items).iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["c", "a"]);
        assert_eq!(open_url("ab-12"), "sesam://open?id=ab-12");
        assert_eq!(open_url("a&b"), "sesam://open?id=a%26b");
    }
}
