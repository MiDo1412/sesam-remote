//! Systemweite Tastenkombinationen (Win, Alt+Tab, Alt+Esc, Strg+Esc, Druck) an den entfernten Rechner
//! statt an Windows geben, wie mstsc «Windows-Tastenkombinationen anwenden». Nur aktiv, solange die App
//! es verlangt (Desktop-Tab hat den Fokus) und ein Fenster dieser App vorne ist.
#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SNAPSHOT, VK_TAB};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYUP, WM_SYSKEYUP,
    };

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    pub static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static NOT_FRONT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static APP: OnceLock<AppHandle> = OnceLock::new();
    /// Tasten, deren Drücken abgefangen wurde: ihr Loslassen gehört auch nach drüben.
    static HELD: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];
    /// Abgefangene Tasten gehen über diesen Kanal an einen eigenen Thread: der Hook selbst muss sofort zurückkehren,
    /// sonst wertet Windows ihn als zu langsam, gibt die Taste trotzdem weiter und entfernt ihn still.
    static OUT: OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<HookKey>>> = OnceLock::new();

    #[derive(Clone, serde::Serialize)]
    struct HookKey {
        code: &'static str,
        down: bool,
    }

    /// DOM-`code` der abgefangenen Taste, `None` = an Windows weiterreichen.
    pub(super) fn grab(vk: u16, alt: bool, ctrl: bool) -> Option<&'static str> {
        match vk {
            v if v == VK_LWIN.0 => Some("MetaLeft"),
            v if v == VK_RWIN.0 => Some("MetaRight"),
            v if v == VK_SNAPSHOT.0 => Some("PrintScreen"),
            v if v == VK_TAB.0 && alt => Some("Tab"),
            v if v == VK_ESCAPE.0 && (alt || ctrl) => Some("Escape"),
            0x73 if alt => Some("F4"), // Alt+F4 schliesst sonst Sesam statt des Fensters drüben (wie mstsc)
            _ => None,
        }
    }

    /// Prozess-ID des Fensters vorne (nur fürs Protokoll).
    static FRONT_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    fn front_pid() -> u32 {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
        pid
    }

    unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        CALLS.fetch_add(1, Ordering::Relaxed);
        // Kein eigener Vordergrund-Vergleich mehr: er schlug bei Mike immer fehl (Fenster vorne hatte nicht die eigene
        // Prozess-ID). ACTIVE ist ohnehin nur an, solange Sesam den Fokus hat und das Remote-Bild aktiv ist.
        let pid = front_pid();
        FRONT_PID.store(pid, Ordering::Relaxed);
        if pid != std::process::id() {
            NOT_FRONT.fetch_add(1, Ordering::Relaxed);
        }
        if code >= 0 {
            let k = unsafe { &*(l.0 as *const KBDLLHOOKSTRUCT) };
            let vk = (k.vkCode & 0xFF) as usize;
            let down = !matches!(w.0 as u32, WM_KEYUP | WM_SYSKEYUP);
            let pressed = |vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
            // Loslassen einer abgefangenen Taste immer abfangen, auch wenn ACTIVE inzwischen aus ist: sonst öffnet
            // ein durchgereichtes Win-Up lokal das Startmenü und drüben bleibt die Taste hängen.
            let held = !down && HELD[vk].swap(false, Ordering::Relaxed);
            let caught = if held { grab(k.vkCode as u16, true, true) } else if ACTIVE.load(Ordering::Relaxed) { grab(k.vkCode as u16, pressed(VK_MENU), pressed(VK_CONTROL)) } else { None };
            if let Some(code) = caught {
                if down {
                    HELD[vk].store(true, Ordering::Relaxed);
                }
                if let Some(tx) = OUT.get().and_then(|m| m.try_lock().ok()) {
                    let _ = tx.send(HookKey { code, down });
                }
                return LRESULT(1); // nicht an Windows (kein Startmenü, kein Fensterwechsel); die Webansicht sieht sie so auch nicht, also kein doppeltes Senden
            }
        }
        unsafe { CallNextHookEx(None, code, w, l) }
    }

    /// Beim Start einmal: Hook auf einem eigenen, dauerhaft laufenden Thread mit Nachrichtenschleife (wie mstsc).
    /// Ob abgefangen wird, entscheidet nur `ACTIVE`; der Hook bleibt installiert.
    pub fn init(app: AppHandle) {
        let _ = APP.set(app);
        let (tx, rx) = std::sync::mpsc::channel::<HookKey>();
        let _ = OUT.set(std::sync::Mutex::new(tx));
        std::thread::spawn(move || {
            for k in rx {
                tracing::info!(code = k.code, down = k.down, "Systemtaste an den Remotecomputer");
                if let Some(app) = APP.get() {
                    let _ = app.emit("sesam-hook-key", k);
                }
            }
        });
        install();
    }

    pub(super) fn install() {
        use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, TranslateMessage};
        std::thread::spawn(|| unsafe {
            let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), Some(module.into()), 0) {
                Ok(_) => tracing::info!("Tastatur-Hook installiert"),
                Err(e) => return tracing::warn!(error = %e, "Tastatur-Hook liess sich nicht installieren"),
            }
            let mut msg = MSG::default();
            loop {
                let r = GetMessageW(&mut msg, None, 0, 0).0;
                if r <= 0 {
                    tracing::warn!(r, "Tastatur-Hook: Nachrichtenschleife beendet");
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
    }

    pub fn set_active(on: bool) {
        if ACTIVE.swap(on, Ordering::Relaxed) != on {
            tracing::info!(
                on,
                hook_aufrufe = CALLS.load(Ordering::Relaxed),
                davon_nicht_vorne = NOT_FRONT.load(Ordering::Relaxed),
                vorne_pid = FRONT_PID.load(Ordering::Relaxed),
                eigene_pid = std::process::id(),
                "Windows-Tastenkombinationen an den Remotecomputer"
            );
        }
    }
}

#[cfg(windows)]
pub use imp::{init, set_active, CALLS};

// shortcut: auf macOS/Linux noch kein Abfangen (macOS bräuchte einen CGEventTap mit Bedienungshilfen-Freigabe), die Kombinationen bleiben beim System.
#[cfg(not(windows))]
pub fn init(_app: tauri::AppHandle) {}
#[cfg(not(windows))]
pub fn set_active(_on: bool) {}

#[cfg(all(test, windows))]
mod tests {
    use super::imp::grab;

    /// Hook sieht einen künstlichen Tastendruck (F15). Ignoriert: braucht eine echte Windows-Sitzung.
    #[test]
    #[ignore]
    fn hook_sieht_tasten() {
        use std::sync::atomic::Ordering;
        use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
        super::imp::install();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let before = super::imp::CALLS.load(Ordering::Relaxed);
        unsafe {
            keybd_event(0x7E, 0, Default::default(), 0);
            keybd_event(0x7E, 0, KEYEVENTF_KEYUP, 0);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        let after = super::imp::CALLS.load(Ordering::Relaxed);
        println!("Hook-Aufrufe: {before} -> {after}");
        assert!(after > before);
    }
    #[test]
    fn faengt_nur_systemkombinationen() {
        assert_eq!(grab(0x5B, false, false), Some("MetaLeft")); // Win links
        assert_eq!(grab(0x09, true, false), Some("Tab")); // Alt+Tab
        assert_eq!(grab(0x09, false, false), None); // Tab allein bleibt in der Webansicht
        assert_eq!(grab(0x1B, false, true), Some("Escape")); // Strg+Esc
        assert_eq!(grab(0x1B, false, false), None);
        assert_eq!(grab(0x41, true, true), None); // Strg+Alt+A
        assert_eq!(grab(0x2C, false, false), Some("PrintScreen"));
        assert_eq!(grab(0x73, true, false), Some("F4")); // Alt+F4
        assert_eq!(grab(0x73, false, false), None);
    }
}
