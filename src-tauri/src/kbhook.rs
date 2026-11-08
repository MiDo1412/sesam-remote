//! Systemweite Tastenkombinationen (Win, Alt+Tab, Alt+Esc, Strg+Esc, Druck, Alt+F4) an den entfernten Rechner
//! statt an Windows geben, wie mstsc «Windows-Tastenkombinationen anwenden». Nur aktiv, solange die App
//! es verlangt (Sesam hat den Fokus und das Remote-Bild ist aktiv).
//!
//! Der Hook läuft in einem kleinen Hilfsprozess (dieselbe exe mit `--kbhook-helper`): Ein Tastatur-Hook im
//! Sesam-Prozess selbst bekam unter Windows 11 keinen einzigen Aufruf, solange ein Sesam-Fenster vorne war
//! (Selbsttest: 0 Aufrufe, lokales Startmenü öffnet), ein Hook in einem anderen Prozess dagegen schon.
//! Protokoll Hilfsprozess ↔ Sesam: Zeilen über stdin/stdout.
//!   Sesam → Hilfe:  `1` scharf, `0` aus, `?` Zähler erfragen. stdin zu = Hilfsprozess beendet sich.
//!   Hilfe → Sesam:  `k <code> <1|0>` abgefangene Taste (gedrückt/losgelassen), `n <aufrufe> <abgefangen>` Zähler.
#[cfg(windows)]
mod imp {
    use std::io::{BufRead, Write};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SNAPSHOT, VK_TAB};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYUP, WM_SYSKEYUP,
    };

    // ---- Hilfsprozess ----

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    pub static CALLS: AtomicUsize = AtomicUsize::new(0);
    static CAUGHT: AtomicUsize = AtomicUsize::new(0);
    /// Prozess-ID von Sesam: abgefangen wird nur, solange ein Sesam-Fenster vorne ist (der Schalter allein
    /// könnte nach einem verpassten Fokuswechsel hängen bleiben).
    static OWNER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn owner_in_front() -> bool {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
        pid == OWNER.load(Ordering::Relaxed)
    }
    /// Tasten, deren Drücken abgefangen wurde: ihr Loslassen gehört auch nach drüben.
    static HELD: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];
    /// Der Hook schreibt nicht selbst (könnte blockieren, dann entfernt Windows ihn still), sondern übergibt hierhin.
    static OUT: OnceLock<Mutex<std::sync::mpsc::Sender<String>>> = OnceLock::new();

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

    /// Solange eine abgefangene Win-Taste gedrückt ist, gehen auch diese Tasten über den Hook nach drüben: sonst
    /// überholt z. B. das R von Win+R (direkt über die Webansicht) das Win (über den Hilfsprozess), und drüben
    /// kommt nur ein R an.
    pub(super) fn with_win(vk: u16) -> Option<&'static str> {
        const LETTERS: [&str; 26] = [
            "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ",
        ];
        const DIGITS: [&str; 10] = ["Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9"];
        match vk {
            0x41..=0x5A => Some(LETTERS[(vk - 0x41) as usize]),
            0x30..=0x39 => Some(DIGITS[(vk - 0x30) as usize]),
            0x25 => Some("ArrowLeft"),
            0x26 => Some("ArrowUp"),
            0x27 => Some("ArrowRight"),
            0x28 => Some("ArrowDown"),
            0x09 => Some("Tab"),
            0x20 => Some("Space"),
            0x0D => Some("Enter"),
            0xA0 => Some("ShiftLeft"),
            0xA1 => Some("ShiftRight"),
            0xA2 => Some("ControlLeft"),
            0xA3 => Some("ControlRight"),
            0xA4 => Some("AltLeft"),
            0xBE => Some("Period"),
            0xBC => Some("Comma"),
            _ => None,
        }
    }

    fn send(line: String) {
        if let Some(tx) = OUT.get().and_then(|m| m.try_lock().ok()) {
            let _ = tx.send(line);
        }
    }

    unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        CALLS.fetch_add(1, Ordering::Relaxed);
        if code >= 0 {
            let k = unsafe { &*(l.0 as *const KBDLLHOOKSTRUCT) };
            let vk = (k.vkCode & 0xFF) as usize;
            let down = !matches!(w.0 as u32, WM_KEYUP | WM_SYSKEYUP);
            let pressed = |vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
            // Loslassen einer abgefangenen Taste immer abfangen, auch wenn inzwischen aus: sonst öffnet ein
            // durchgereichtes Win-Up lokal das Startmenü und drüben bleibt die Taste hängen.
            let held = !down && HELD[vk].swap(false, Ordering::Relaxed);
            let vk16 = k.vkCode as u16;
            let win_held = HELD[VK_LWIN.0 as usize].load(Ordering::Relaxed) || HELD[VK_RWIN.0 as usize].load(Ordering::Relaxed);
            let caught = if held {
                grab(vk16, true, true).or_else(|| with_win(vk16))
            } else if ACTIVE.load(Ordering::Relaxed) && owner_in_front() {
                grab(vk16, pressed(VK_MENU), pressed(VK_CONTROL)).or_else(|| if win_held { with_win(vk16) } else { None })
            } else {
                None
            };
            if let Some(code) = caught {
                CAUGHT.fetch_add(1, Ordering::Relaxed);
                if down {
                    HELD[vk].store(true, Ordering::Relaxed);
                }
                send(format!("k {code} {}", down as u8));
                return LRESULT(1); // nicht an Windows (kein Startmenü, kein Fensterwechsel)
            }
        }
        unsafe { CallNextHookEx(None, code, w, l) }
    }

    /// Hook installieren und auf diesem Thread die Nachrichtenschleife laufen lassen (kehrt nicht zurück).
    pub(super) fn run_hook() {
        use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, TranslateMessage};
        unsafe {
            let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
            if SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), Some(module.into()), 0).is_err() {
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    /// Einstieg des Hilfsprozesses (`sesam-remote.exe --kbhook-helper`).
    pub fn helper(owner: u32) {
        OWNER.store(owner, Ordering::Relaxed);
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let _ = OUT.set(Mutex::new(tx));
        std::thread::spawn(move || {
            let mut out = std::io::stdout().lock();
            for line in rx {
                if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
                    std::process::exit(0);
                }
            }
        });
        std::thread::spawn(|| {
            for line in std::io::stdin().lock().lines() {
                match line.as_deref() {
                    Ok("1") => ACTIVE.store(true, Ordering::Relaxed),
                    Ok("0") => ACTIVE.store(false, Ordering::Relaxed),
                    Ok("?") => send(format!("n {} {}", CALLS.load(Ordering::Relaxed), CAUGHT.load(Ordering::Relaxed))),
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            std::process::exit(0); // Sesam ist weg
        });
        run_hook();
    }

    // ---- Sesam-Seite ----

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static TO_HELPER: OnceLock<Mutex<std::process::ChildStdin>> = OnceLock::new();
    static ON: AtomicBool = AtomicBool::new(false);
    /// Zähler des Hilfsprozesses, Stand der letzten `n`-Antwort.
    static H_CALLS: AtomicUsize = AtomicUsize::new(0);
    static H_CAUGHT: AtomicUsize = AtomicUsize::new(0);

    #[derive(Clone, serde::Serialize)]
    struct HookKey {
        code: String,
        down: bool,
    }

    fn tell(cmd: &str) {
        if let Some(mut w) = TO_HELPER.get().and_then(|m| m.lock().ok()) {
            let _ = writeln!(w, "{cmd}").and_then(|_| w.flush());
        }
    }

    /// Beim Start einmal: Hilfsprozess starten und seine Meldungen lesen.
    pub fn init(app: AppHandle) {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        let _ = APP.set(app);
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => return tracing::warn!(error = %e, "Tastatur-Hilfsprozess: eigene exe nicht gefunden"),
        };
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let child = Command::new(exe).arg("--kbhook-helper").arg(std::process::id().to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).creation_flags(CREATE_NO_WINDOW).spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => return tracing::warn!(error = %e, "Tastatur-Hilfsprozess liess sich nicht starten"),
        };
        tracing::info!(pid = child.id(), "Tastatur-Hilfsprozess gestartet");
        let _ = TO_HELPER.set(Mutex::new(child.stdin.take().expect("stdin")));
        let stdout = child.stdout.take().expect("stdout");
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                let mut p = line.split(' ');
                match (p.next(), p.next(), p.next()) {
                    (Some("k"), Some(code), Some(down)) => {
                        let k = HookKey { code: code.to_string(), down: down == "1" };
                        tracing::info!(code = k.code, down = k.down, "Systemtaste an den Remotecomputer");
                        if let Some(app) = APP.get() {
                            let _ = app.emit("sesam-hook-key", k);
                        }
                    }
                    (Some("n"), Some(c), Some(k)) => {
                        H_CALLS.store(c.parse().unwrap_or(0), Ordering::Relaxed);
                        H_CAUGHT.store(k.parse().unwrap_or(0), Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            tracing::warn!("Tastatur-Hilfsprozess beendet");
        });
    }

    pub fn set_active(on: bool) {
        tell(if on { "1" } else { "0" });
        tell("?");
        if ON.swap(on, Ordering::Relaxed) != on {
            tracing::info!(
                on,
                hook_aufrufe = H_CALLS.load(Ordering::Relaxed),
                abgefangen = H_CAUGHT.load(Ordering::Relaxed),
                "Windows-Tastenkombinationen an den Remotecomputer"
            );
        }
    }

    pub fn calls() -> usize {
        H_CALLS.load(Ordering::Relaxed)
    }

    fn counters() -> (usize, usize) {
        tell("?");
        std::thread::sleep(std::time::Duration::from_millis(200));
        (H_CALLS.load(Ordering::Relaxed), H_CAUGHT.load(Ordering::Relaxed))
    }

    /// Programmname des Fensters vorne (z. B. SearchHost.exe = lokales Startmenü).
    fn front_exe() -> String {
        use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let ok = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .and_then(|h| unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len) });
        let path = if ok.is_ok() { String::from_utf16_lossy(&buf[..len as usize]) } else { format!("pid {pid}") };
        let name = path.rsplit('\\').next().unwrap_or(&path).to_string();
        if pid == std::process::id() { format!("{name} (Sesam)") } else { name }
    }

    /// Selbsttest: drückt Win, Win+R und Alt+Tab (wie eine Tastatur) und meldet, was der Hook gesehen und
    /// abgefangen hat und welches Fenster danach lokal vorne ist. Bestanden: alles abgefangen, Sesam bleibt vorne.
    pub fn selftest() -> String {
        use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_R};
        let key = |vk: u16, up: bool| unsafe { keybd_event(vk as u8, 0, if up { KEYEVENTF_KEYUP } else { Default::default() }, 0) };
        let wait = |ms| std::thread::sleep(std::time::Duration::from_millis(ms));
        let mut out = vec![format!("vorher: vorne {}, scharf {}", front_exe(), ON.load(Ordering::Relaxed))];
        let steps: [(&str, &[u16], usize); 3] = [("Win", &[VK_LWIN.0], 2), ("Win+R", &[VK_LWIN.0, VK_R.0], 4), ("Alt+Tab", &[VK_MENU.0, VK_TAB.0], 2)];
        let mut ok = true;
        for (name, keys, want) in steps {
            let (c0, k0) = counters();
            for &k in keys {
                key(k, false);
                wait(60);
            }
            for &k in keys.iter().rev() {
                key(k, true);
                wait(60);
            }
            wait(1000);
            let (c1, k1) = counters();
            let front = front_exe();
            ok &= k1 - k0 == want && front.ends_with("(Sesam)");
            out.push(format!("{name}: Hook-Aufrufe {}, abgefangen {}, danach vorne {front}", c1 - c0, k1 - k0));
        }
        out.push(if ok { "Bestanden: alles abgefangen, lokal nichts geöffnet.".into() } else { "Nicht bestanden.".into() });
        let r = out.join("\n");
        tracing::info!(ergebnis = %r.replace('\n', " | "), "Tastentest");
        r
    }
}

#[cfg(windows)]
pub use imp::{calls, helper, init, selftest, set_active};

// shortcut: auf macOS/Linux noch kein Abfangen (macOS bräuchte einen CGEventTap mit Bedienungshilfen-Freigabe), die Kombinationen bleiben beim System.
#[cfg(not(windows))]
pub fn init(_app: tauri::AppHandle) {}
#[cfg(not(windows))]
pub fn set_active(_on: bool) {}
#[cfg(not(windows))]
pub fn calls() -> usize {
    0
}
#[cfg(not(windows))]
pub fn helper(_owner: u32) {}
#[cfg(not(windows))]
pub fn selftest() -> String {
    "Nur unter Windows.".into()
}

#[cfg(all(test, windows))]
mod tests {
    use super::imp::grab;

    /// Hook sieht einen künstlichen Tastendruck (F15). Ignoriert: braucht eine echte Windows-Sitzung.
    #[test]
    #[ignore]
    fn hook_sieht_tasten() {
        use std::sync::atomic::Ordering;
        use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
        std::thread::spawn(super::imp::run_hook);
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
        use super::imp::with_win;
        assert_eq!(with_win(0x52), Some("KeyR")); // Win+R
        assert_eq!(with_win(0x44), Some("KeyD"));
        assert_eq!(with_win(0x31), Some("Digit1"));
        assert_eq!(with_win(0x25), Some("ArrowLeft"));
        assert_eq!(with_win(0x70), None); // F1 bleibt beim normalen Weg
    }
}
