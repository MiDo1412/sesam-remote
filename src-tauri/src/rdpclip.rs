//! RDP-Zwischenablage ausserhalb von Windows (macOS, iPadOS, Linux): Text in beide Richtungen über die Zwischenablage
//! des Systems (arboard), auf dem Mac auch Bilder (CF_DIB). Windows nutzt das native Backend aus ironrdp-cliprdr-native
//! mit Bildern und Dateien.
//! shortcut: Bilder nur auf dem Mac (dort zeigt `changeCount` Änderungen billig an; Linux müsste dafür jedes Mal das
//! ganze Bild lesen), Dateien noch gar nicht.

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
    /// Format der laufenden Anfrage an den Server (Text oder Bild).
    asked: Option<ClipboardFormatId>,
}

/// Bilder über die Zwischenablage (nur Mac, siehe oben).
const IMAGES: bool = cfg!(target_os = "macos");

#[cfg(desktop)]
fn local_image() -> Option<(usize, usize, Vec<u8>)> {
    crate::clipshare::get_image()
}

#[cfg(not(desktop))]
fn local_image() -> Option<(usize, usize, Vec<u8>)> {
    None
}

/// RGBA → geräteunabhängige Bitmap (BITMAPINFOHEADER, 32 Bit BGRA, von unten nach oben), wie Windows CF_DIB erwartet.
pub fn rgba_to_dib(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + w * h * 4);
    for v in [40u32, w as u32, h as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes()); // Ebenen
    out.extend_from_slice(&32u16.to_le_bytes()); // Bit je Pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&((w * h * 4) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]); // Auflösung, Farbtabelle
    for row in rgba.chunks_exact(w * 4).rev() {
        for p in row.chunks_exact(4) {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out
}

#[cfg_attr(not(desktop), allow(dead_code))]
/// CF_DIB → (Breite, Höhe, RGBA). 24 und 32 Bit, BI_RGB oder BI_BITFIELDS (Standardmasken), oben- oder untenherum.
pub fn dib_to_rgba(d: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let u32_at = |i: usize| d.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = u32_at(0)? as usize;
    let w = i32::from_le_bytes(d.get(4..8)?.try_into().ok()?);
    let h = i32::from_le_bytes(d.get(8..12)?.try_into().ok()?);
    let bits = u16::from_le_bytes(d.get(14..16)?.try_into().ok()?);
    let compression = u32_at(16)?;
    if header < 40 || w <= 0 || h == 0 || !(bits == 24 || bits == 32) || !(compression == 0 || compression == 3) {
        return None;
    }
    let (w, top_down, h) = (w as usize, h < 0, h.unsigned_abs() as usize);
    if w > 16384 || h > 16384 {
        return None;
    }
    // BI_BITFIELDS mit kleinem Kopf: drei Masken folgen dem Kopf.
    let start = header + if compression == 3 && header == 40 { 12 } else { 0 };
    let px = usize::from(bits / 8);
    let stride = (w * px).div_ceil(4) * 4;
    let data = d.get(start..start + stride * h)?;
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &data[(if top_down { y } else { h - 1 - y }) * stride..][..w * px];
        for p in row.chunks_exact(px) {
            // 32 Bit ohne Alpha (oft 0): deckend.
            let a = if px == 4 && compression == 0 && p[3] != 0 { p[3] } else { 255 };
            out.extend_from_slice(&[p[2], p[1], p[0], a]);
        }
    }
    Some((w, h, out))
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

fn announce_image(proxy: &ClipProxy) {
    proxy.send_clipboard_message(ClipboardMessage::SendInitiateCopy(vec![ClipboardFormat::new(ClipboardFormatId::CF_DIB)]));
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
            asked: None,
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
                let Some(t) = local_text() else {
                    // Kein Text, aber ein Bild (Bildschirmfoto, Vorschau …): als Bild anbieten, gelesen wird erst beim Einfügen.
                    if IMAGES && local_image().is_some() {
                        *last.lock().unwrap() = None;
                        announce_image(&proxy);
                    }
                    continue;
                };
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
        let has = |id| formats.iter().any(|f| f.id() == id);
        // Text geht vor (Office kopiert oft beides); Bild nur, wenn es keinen Text gibt.
        self.asked = if has(ClipboardFormatId::CF_UNICODETEXT) {
            Some(ClipboardFormatId::CF_UNICODETEXT)
        } else if IMAGES && has(ClipboardFormatId::CF_DIB) {
            Some(ClipboardFormatId::CF_DIB)
        } else {
            None
        };
        if let Some(f) = self.asked {
            self.proxy.send_clipboard_message(ClipboardMessage::SendInitiatePaste(f));
        }
    }
    fn on_format_data_request(&mut self, r: FormatDataRequest) {
        let data = if r.format == ClipboardFormatId::CF_UNICODETEXT {
            local_text().map(|t| OwnedFormatDataResponse::new_unicode_string(&t))
        } else if IMAGES && r.format == ClipboardFormatId::CF_DIB {
            local_image().map(|(w, h, rgba)| OwnedFormatDataResponse::new_data(rgba_to_dib(w, h, &rgba)))
        } else {
            None
        };
        self.proxy.send_clipboard_message(ClipboardMessage::SendFormatData(data.unwrap_or_else(OwnedFormatDataResponse::new_error)));
    }
    fn on_format_data_response(&mut self, r: FormatDataResponse<'_>) {
        if r.is_error() {
            return;
        }
        if self.asked.take() == Some(ClipboardFormatId::CF_DIB) {
            #[cfg(desktop)]
            match dib_to_rgba(r.data()) {
                Some((w, h, rgba)) => {
                    *self.last.lock().unwrap() = None;
                    if let Err(e) = crate::clipshare::set_image(w, h, rgba) {
                        tracing::warn!(error = %e, "Zwischenablage: Bild vom Server nicht übernommen");
                    }
                    if let Some(n) = change_count() {
                        self.seen.store(n, Ordering::Relaxed);
                    }
                }
                None => tracing::warn!(len = r.data().len(), "Zwischenablage: Bild vom Server nicht lesbar"),
            }
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
    fn bild_als_dib_hin_und_zurueck() {
        let rgba: Vec<u8> = (0..3 * 2).flat_map(|i| [i as u8 * 10, 100, 200, 255]).collect();
        let dib = rgba_to_dib(3, 2, &rgba);
        assert_eq!(dib.len(), 40 + 3 * 2 * 4);
        assert_eq!(dib_to_rgba(&dib), Some((3, 2, rgba)));
        // 24 Bit, von oben nach unten (negative Höhe), Zeilen auf 4 Byte aufgefüllt
        let mut d = vec![0u8; 40];
        d[0] = 40;
        d[4..8].copy_from_slice(&1i32.to_le_bytes());
        d[8..12].copy_from_slice(&(-2i32).to_le_bytes());
        d[14] = 24;
        d.extend_from_slice(&[1, 2, 3, 0, 4, 5, 6, 0]);
        assert_eq!(dib_to_rgba(&d), Some((1, 2, vec![3, 2, 1, 255, 6, 5, 4, 255])));
        assert_eq!(dib_to_rgba(&d[..44]), None, "zu kurz");
    }

    #[test]
    fn nur_nach_aenderung_lesen() {
        assert!(!changed(Some(4), Some(4)), "gleicher Zähler: nichts lesen");
        assert!(changed(Some(4), Some(5)));
        assert!(changed(Some(4), None), "ohne Zähler (Linux) immer nachsehen");
    }
}
