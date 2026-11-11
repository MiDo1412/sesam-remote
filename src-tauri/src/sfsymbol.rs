//! SF Symbols für die Oberfläche auf macOS: `sfsymbol://localhost/<name>` liefert das Symbol als PNG.
//! Die Weboberfläche nutzt es als CSS-Maske, die Farbe kommt also weiter aus dem Thema (src/icons.ts).
//! SF Symbols dürfen nur auf Apple-Geräten gezeigt werden; darum zeichnet das System sie hier zur Laufzeit,
//! nichts davon liegt im Repository oder im Bundle.

use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSImageSymbolConfiguration, NSWorkspace, NSWorkspaceIconCreationOptions};
use objc2_foundation::{NSData, NSDictionary, NSString};

/// PNG eines SF Symbols in Grösse `pt` (Punkte, in doppelter Auflösung gezeichnet), None bei unbekanntem Namen.
pub fn png(name: &str, pt: f64) -> Option<Vec<u8>> {
    let img = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None)?;
    let img = img.imageWithSymbolConfiguration(&NSImageSymbolConfiguration::configurationWithPointSize_weight(pt * 2.0, 0.0))?;
    let rep = NSBitmapImageRep::imageRepWithData(&*img.TIFFRepresentation()?)?;
    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some(data.to_vec())
}

/// Symbol der laufenden App (Sesam Remote.app) als eigenes Symbol einer Datei setzen, z. B. einer Verknüpfung
/// (.webloc zeigt sonst das allgemeine Internet-Symbol). Ausserhalb eines App-Bundles: nichts tun.
pub fn set_app_icon(file: &std::path::Path) -> bool {
    let Some(app) = std::env::current_exe().ok().and_then(|e| e.ancestors().find(|p| p.extension().is_some_and(|x| x == "app")).map(|p| p.to_path_buf())) else {
        return false;
    };
    let ws = NSWorkspace::sharedWorkspace();
    let icon = ws.iconForFile(&NSString::from_str(&app.to_string_lossy()));
    ws.setIcon_forFile_options(Some(&icon), &NSString::from_str(&file.to_string_lossy()), NSWorkspaceIconCreationOptions::empty())
}

/// JPEG mit dem Bilddecoder des Systems (ImageIO) dekodieren, für VNC Tight: Breite, Höhe, Bytes je Pixel,
/// Bytes je Zeile und die Pixel (R, G, B[, X]). None, wenn das Format unerwartet ist.
pub fn decode_jpeg(data: &[u8]) -> Option<(usize, usize, usize, usize, Vec<u8>)> {
    // Eigener Autorelease-Pool: läuft auf einem Tokio-Thread, sonst bleiben die dekodierten Bilder liegen.
    objc2::rc::autoreleasepool(|_| decode_jpeg_inner(data))
}

fn decode_jpeg_inner(data: &[u8]) -> Option<(usize, usize, usize, usize, Vec<u8>)> {
    let rep = NSBitmapImageRep::imageRepWithData(&NSData::with_bytes(data))?;
    let (w, h, bpp, stride) = (rep.pixelsWide() as usize, rep.pixelsHigh() as usize, rep.bitsPerPixel() as usize / 8, rep.bytesPerRow() as usize);
    if rep.isPlanar() || rep.bitsPerSample() != 8 || rep.samplesPerPixel() < 3 || bpp < 3 || rep.bitmapData().is_null() {
        return None;
    }
    let px = unsafe { std::slice::from_raw_parts(rep.bitmapData(), stride * h) }.to_vec();
    Some((w, h, bpp, stride, px))
}

#[cfg(test)]
mod tests {
    /// Ein vom System erzeugtes JPEG (sips) kommt mit plausiblen Farben zurück.
    #[test]
    fn jpeg_ueber_imageio() {
        let dir = std::env::temp_dir().join(format!("sesam-jpeg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        let mut e = png::Encoder::new(std::fs::File::create(&png).unwrap(), 16, 8);
        e.set_color(png::ColorType::Rgb);
        e.write_header().unwrap().write_image_data(&[200, 40, 40].repeat(16 * 8)).unwrap();
        let jpg = dir.join("a.jpg");
        assert!(std::process::Command::new("sips").args(["-s", "format", "jpeg"]).arg(&png).arg("--out").arg(&jpg).output().unwrap().status.success());
        let (w, h, bpp, stride, px) = super::decode_jpeg(&std::fs::read(&jpg).unwrap()).expect("jpeg");
        assert_eq!((w, h), (16, 8));
        let p = &px[stride * 4 + 8 * bpp..][..3];
        assert!(p[0] > 170 && p[1] < 80 && p[2] < 80, "Farbe {p:?}");
        std::fs::remove_dir_all(dir).unwrap();
        assert!(super::decode_jpeg(b"kein jpeg").is_none());
    }

    #[test]
    fn zeichnet_bekanntes_symbol_und_lehnt_unbekanntes_ab() {
        let p = super::png("gearshape", 16.0).expect("gearshape");
        assert_eq!(&p[1..4], b"PNG");
        assert!(super::png("gibt.es.nicht", 16.0).is_none());
    }
}
