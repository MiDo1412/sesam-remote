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
    static APP: OnceLock<AppHandle> = OnceLock::new();

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
            _ => None,
        }
    }

    fn our_window_in_front() -> bool {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
        pid == std::process::id()
    }

    unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        CALLS.fetch_add(1, Ordering::Relaxed);
        if code >= 0 && ACTIVE.load(Ordering::Relaxed) && our_window_in_front() {
            let k = unsafe { &*(l.0 as *const KBDLLHOOKSTRUCT) };
            let pressed = |vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
            if let Some(code) = grab(k.vkCode as u16, pressed(VK_MENU), pressed(VK_CONTROL)) {
                let down = !matches!(w.0 as u32, WM_KEYUP | WM_SYSKEYUP);
                tracing::debug!(code, down, "Systemtaste an den Remotecomputer");
                if let Some(app) = APP.get() {
                    let _ = app.emit("sesam-hook-key", HookKey { code, down });
                }
                return LRESULT(1); // nicht an Windows (kein Startmenü, kein Fensterwechsel)
            }
        }
        unsafe { CallNextHookEx(None, code, w, l) }
    }

    /// Hook einmal installieren (eigener Thread mit Nachrichtenschleife, wie Windows es verlangt).
    /// Nur die App merken; der Hook selbst kommt erst beim Einschalten (dann ist das Fenster vorne
    /// und wird von Windows nicht gedrosselt; ein gedrosselter Hook wird still entfernt).
    pub fn init(app: AppHandle) {
        let _ = APP.set(app);
    }

    /// Thread-ID des Hook-Threads (0 = kein Hook).
    static THREAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    pub(super) fn install() {
        use windows::Win32::System::Threading::{GetCurrentThreadId, GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL};
        use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, TranslateMessage, UnhookWindowsHookEx};
        if THREAD.load(Ordering::SeqCst) != 0 {
            return;
        }
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
            let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
            let h = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), Some(module.into()), 0) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(error = %e, "Tastatur-Hook liess sich nicht installieren");
                    let _ = ready_tx.send(());
                    return;
                }
            };
            THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
            let _ = ready_tx.send(());
            let mut msg = MSG::default();
            // Endet mit WM_QUIT aus `remove`.
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnhookWindowsHookEx(h);
        });
        let _ = ready_rx.recv_timeout(std::time::Duration::from_secs(2));
    }

    fn remove() {
        use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
        let id = THREAD.swap(0, Ordering::SeqCst);
        if id != 0 {
            unsafe { let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)); }
        }
    }

    pub fn set_active(on: bool) {
        if ACTIVE.swap(on, Ordering::Relaxed) != on {
            tracing::info!(on, "Windows-Tastenkombinationen an den Remotecomputer");
            if on { install() } else { remove() }
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
    }
}
