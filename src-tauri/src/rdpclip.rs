//! RDP-Zwischenablage ausserhalb von Windows (macOS, Linux): Text in beide Richtungen über die Zwischenablage
//! des Systems (arboard). Windows nutzt das native Backend aus ironrdp-cliprdr-native mit Bildern und Dateien.
//! shortcut: nur Text (CF_UNICODETEXT); Bilder und Dateien erst, wenn jemand sie auf dem Mac braucht.

use crate::rdp::{ClipProxy, Input};
use ironrdp_cliprdr::backend::{ClipboardMessage, ClipboardMessageProxy, CliprdrBackend, CliprdrBackendFactory};
use ironrdp_cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest, FileContentsResponse, FormatDataRequest,
    FormatDataResponse, LockDataId, OwnedFormatDataResponse,
};
use std::sync::atomic::{AtomicBool, Ordering};
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
}

fn local_text() -> Option<String> {
    arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok().filter(|t| !t.is_empty())
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
            last: Arc::new(Mutex::new(local_text())),
            ready: Arc::new(AtomicBool::new(false)),
            alive: Arc::new(AtomicBool::new(true)),
        };
        // Lokale Änderungen erkennen: macOS meldet sie nicht, darum alle 500 ms nachsehen (nur Text, billig).
        let (proxy, last, ready, alive) = (b.proxy.clone(), b.last.clone(), b.ready.clone(), b.alive.clone());
        std::thread::spawn(move || {
            while alive.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let Some(t) = local_text() else { continue };
                let mut l = last.lock().unwrap();
                if ready.load(Ordering::Relaxed) && l.as_deref() != Some(&t) {
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
        if local_text().is_some() {
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
        if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(t)) {
            tracing::warn!(error = %e, "Zwischenablage: Text vom Server nicht übernommen");
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
}
