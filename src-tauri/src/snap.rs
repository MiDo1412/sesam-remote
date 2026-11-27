//! Windows 11: Andocklayouts beim Darüberfahren über den eigenen Maximieren-Knopf. Das WebView2-Fenster liegt über
//! der ganzen Fläche, deshalb sieht Windows den Knopf nicht. Ein unsichtbares Kindfenster genau über dem Knopf meldet
//! `HTMAXBUTTON` (wie die Ziehleiste von Windows Terminal), übernimmt Klick und Hover und meldet den Zustand an die
//! Oberfläche («snap-button»: "hover", "down" oder "").
use std::cell::Cell;
use tauri::{Emitter, WebviewWindow};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("SESAM_SNAP_BUTTON");

struct Overlay {
    window: WebviewWindow,
    state: Cell<&'static str>,
}

impl Overlay {
    fn set(&self, s: &'static str) {
        if self.state.replace(s) != s {
            let _ = self.window.emit_to(self.window.label(), "snap-button", s);
        }
    }
}

/// Legt das Kindfenster über den Knopf (physische Pixel relativ zum Fensterinhalt); Breite 0 blendet es aus.
pub fn place(window: WebviewWindow, x: i32, y: i32, w: i32, h: i32) {
    let win = window.clone();
    let _ = window.run_on_main_thread(move || unsafe {
        let Ok(parent) = win.hwnd() else { return };
        let child = match FindWindowExW(Some(parent), None, CLASS, PCWSTR::null()) {
            Ok(c) => c,
            Err(_) => {
                let Ok(module) = GetModuleHandleW(None) else { return };
                let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: module.into(), lpszClassName: CLASS, ..Default::default() };
                RegisterClassW(&class); // beim zweiten Fenster schon registriert: Fehler ohne Folgen
                let data = Box::into_raw(Box::new(Overlay { window: win.clone(), state: Cell::new("") }));
                match CreateWindowExW(WINDOW_EX_STYLE::default(), CLASS, PCWSTR::null(), WS_CHILD | WS_CLIPSIBLINGS, 0, 0, 0, 0, Some(parent), None, Some(module.into()), Some(data as _)) {
                    Ok(c) => c,
                    Err(_) => return drop(Box::from_raw(data)),
                }
            }
        };
        if w <= 0 || h <= 0 {
            let _ = ShowWindow(child, SW_HIDE);
            return;
        }
        // Direkt unter Tauris Kindfenster für die Ränder, damit oben am Knopf weiterhin die Grösse geändert werden kann.
        let above = FindWindowExW(Some(parent), None, w!("TAURI_DRAG_RESIZE_BORDERS"), PCWSTR::null()).unwrap_or(HWND_TOP);
        let _ = SetWindowPos(child, Some(above), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    });
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*(lp.0 as *const CREATESTRUCTW)).lpCreateParams as isize);
        }
        let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Overlay;
        let Some(o) = data.as_ref() else { return DefWindowProcW(hwnd, msg, wp, lp) };
        match msg {
            WM_NCHITTEST => return LRESULT(HTMAXBUTTON as _),
            WM_NCMOUSEMOVE => {
                if o.state.get().is_empty() {
                    let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE | TME_NONCLIENT, hwndTrack: hwnd, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut t);
                    o.set("hover");
                }
            }
            WM_NCMOUSELEAVE => o.set(""),
            // Selbst behandeln: DefWindowProc würde sonst einen klassischen Knopf zeichnen
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
                o.set("down");
                return LRESULT(0);
            }
            WM_NCLBUTTONUP => {
                if o.state.get() == "down" {
                    o.set("hover");
                    if let Ok(parent) = GetParent(hwnd) {
                        let _ = ShowWindow(parent, if IsZoomed(parent).as_bool() { SW_RESTORE } else { SW_MAXIMIZE });
                    }
                }
                return LRESULT(0);
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(data as *mut Overlay));
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}
