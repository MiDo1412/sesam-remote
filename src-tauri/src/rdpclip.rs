//! RDP-Zwischenablage ausserhalb von Windows (macOS, iPadOS, Linux): Text in beide Richtungen über die Zwischenablage
//! des Systems (arboard). Windows nutzt das native Backend aus ironrdp-cliprdr-native mit Bildern und Dateien.
//! shortcut: nur Text (CF_UNICODETEXT); Bilder und Dateien erst, wenn jemand sie auf dem Mac braucht.

use crate::rdp::{ClipProxy, Input};
use ironrdp_cliprdr::backend::{ClipboardMessage, ClipboardMessageProxy, CliprdrBackend, CliprdrBackendFactory};
use ironrdp_cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest, FileContentsResponse, FormatDataRequest,
    FormatDataResponse, LockDataId, OwnedFormatDataResponse,
};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

pub struct Factory(pub UnboundedSender<Input>);

impl CliprdrBackendFactory for Factory {
    fn build_cliprdr_backend(&self) -> Box<dyn CliprdrBackend> {
        Box::new(Backend::new(ClipProxy(self.0.clone())))
    }
}

#[derive(Debug)]
struct Backend {
    proxy: Arc<ClipProxy>,
    /// Zuletzt abgeglichener Text (in beide Richtungen), damit nichts im Kreis läuft.
    last: Arc<Mutex<Option<String>>>,
    ready: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    /// Zuletzt gesehener `changeCount` der Zwischenablage (Apple), damit eigene Änderungen nicht zurücklaufen.
    seen: Arc<AtomicIsize>,
}

fn local_text() -> Option<String> {
    crate::clipshare::get_text().ok().filter(|t| !t.is_empty())
}

/// `changeCount` der allgemeinen Zwischenablage: billig und ohne Rückfrage des Systems (anders als das Lesen
/// des Inhalts, das unter iOS 16+ jedes Mal «Einfügen erlauben?» zeigt). `None`, wo es so etwas nicht gibt.
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn change_count() -> Option<isize> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    #[cfg(target_os = "macos")]
    let pb: *mut AnyObject = unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
    #[cfg(target_os = "ios")]
    let pb: *mut AnyObject = unsafe { msg_send![class!(UIPasteboard), generalPasteboard] };
    let pb = unsafe { pb.as_ref() }?;
    Some(unsafe { msg_send![pb, changeCount] })
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn change_count() -> Option<isize> {
    None
}

/// Liegt lokal Text bereit? Auf iPadOS ohne den Inhalt zu lesen (`hasStrings` fragt nicht nach).
#[cfg(target_os = "ios")]
fn has_local_text() -> bool {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    let pb: *mut AnyObject = unsafe { msg_send![class!(UIPasteboard), generalPasteboard] };
    unsafe { pb.as_ref() }.is_some_and(|pb| {
        let b: Bool = unsafe { msg_send![pb, hasStrings] };
        b.as_bool()
    })
}

#[cfg(not(target_os = "ios"))]
fn has_local_text() -> bool {
    local_text().is_some()
}

/// Nach einer Änderung von `before` auf `now` lesen bzw. anbieten? Ohne Zähler (Linux) immer nachsehen.
fn changed(before: Option<isize>, now: Option<isize>) -> bool {
    now.is_none() || now != before
}

fn announce(proxy: &ClipProxy) {
    proxy.send_clipboard_message(ClipboardMessage::SendInitiateCopy(vec![ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]));
}

/// Text aus einer CF_UNICODETEXT-Antwort, ohne die abschliessenden Nullzeichen.
fn decode(r: &FormatDataResponse<'_>) -> Option<String> {
    let t = r.to_unicode_string().ok()?;
    Some(t.trim_end_matches('\0').to_string())
}

impl Backend {
    fn new(proxy: ClipProxy) -> Self {
        let b = Self {
            proxy: Arc::new(proxy),
            // iPadOS: beim Verbinden nichts lesen (sonst fragt das System sofort nach).
            last: Arc::new(Mutex::new(if cfg!(target_os = "ios") { None } else { local_text() })),
            ready: Arc::new(AtomicBool::new(false)),
            alive: Arc::new(AtomicBool::new(true)),
            seen: Arc::new(AtomicIsize::new(change_count().unwrap_or(0))),
        };
        // Lokale Änderungen erkennen: weder macOS noch iPadOS melden sie, darum alle 500 ms nachsehen. Auf Apple-Systemen
        // nur den `changeCount` (billig, ohne Rückfrage); den Text lesen nur nach einer Änderung. iPadOS liest ihn nie im
        // Hintergrund, sondern bietet nur an; gelesen wird erst, wenn auf dem Server eingefügt wird.
        let (proxy, last, ready, alive, seen) = (b.proxy.clone(), b.last.clone(), b.ready.clone(), b.alive.clone(), b.seen.clone());
        std::thread::spawn(move || {
            while alive.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let now = change_count();
                if !changed(Some(seen.load(Ordering::Relaxed)), now) {
                    continue;
                }
                if !ready.load(Ordering::Relaxed) {
                    continue;
                }
                if let Some(n) = now {
                    seen.store(n, Ordering::Relaxed);
                }
                if cfg!(target_os = "ios") {
                    *last.lock().unwrap() = None;
                    announce(&proxy);
                    continue;
                }
                let Some(t) = local_text() else { continue };
                let mut l = last.lock().unwrap();
                if l.as_deref() != Some(&t) {
                    *l = Some(t);
                    announce(&proxy);
                }
            }
        });
        b
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

ironrdp_pdu::impl_as_any!(Backend);

impl CliprdrBackend for Backend {
    fn temporary_directory(&self) -> &str {
        ".cliprdr"
    }
    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::empty()
    }
    fn on_ready(&mut self) {}
    fn on_request_format_list(&mut self) {
        // Verbindung steht: lokalen Text gleich anbieten.
        self.ready.store(true, Ordering::Relaxed);
        if has_local_text() {
            announce(&self.proxy);
        }
    }
    fn on_process_negotiated_capabilities(&mut self, _: ClipboardGeneralCapabilityFlags) {}
    fn on_remote_copy(&mut self, formats: &[ClipboardFormat]) {
        if formats.iter().any(|f| f.id() == ClipboardFormatId::CF_UNICODETEXT) {
            self.proxy.send_clipboard_message(ClipboardMessage::SendInitiatePaste(ClipboardFormatId::CF_UNICODETEXT));
        }
    }
    fn on_format_data_request(&mut self, r: FormatDataRequest) {
        let data = match local_text() {
            Some(t) if r.format == ClipboardFormatId::CF_UNICODETEXT => OwnedFormatDataResponse::new_unicode_string(&t),
            _ => OwnedFormatDataResponse::new_error(),
        };
        self.proxy.send_clipboard_message(ClipboardMessage::SendFormatData(data));
    }
    fn on_format_data_response(&mut self, r: FormatDataResponse<'_>) {
        if r.is_error() {
            return;
        }
        let Some(t) = decode(&r) else { return };
        *self.last.lock().unwrap() = Some(t.clone());
        if let Err(e) = crate::clipshare::set_text(&t) {
            tracing::warn!(error = %e, "Zwischenablage: Text vom Server nicht übernommen");
        }
        // Eigene Änderung: nicht als lokale Kopie zurück an den Server melden.
        if let Some(n) = change_count() {
            self.seen.store(n, Ordering::Relaxed);
        }
    }
    fn on_file_contents_request(&mut self, r: FileContentsRequest) {
        self.proxy.send_clipboard_message(ClipboardMessage::SendFileContentsResponse(FileContentsResponse::new_error(r.stream_id)));
    }
    fn on_file_contents_response(&mut self, _: FileContentsResponse<'_>) {}
    fn on_lock(&mut self, _: LockDataId) {}
    fn on_unlock(&mut self, _: LockDataId) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_hin_und_zurueck() {
        let owned = OwnedFormatDataResponse::new_unicode_string("Grüezi 👋");
        assert_eq!(decode(&owned).as_deref(), Some("Grüezi 👋"));
    }

    #[test]
    fn nur_nach_aenderung_lesen() {
        assert!(!changed(Some(4), Some(4)), "gleicher Zähler: nichts lesen");
        assert!(changed(Some(4), Some(5)));
        assert!(changed(Some(4), None), "ohne Zähler (Linux) immer nachsehen");
    }
}
