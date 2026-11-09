//! SF Symbols für die Oberfläche auf macOS: `sfsymbol://localhost/<name>` liefert das Symbol als PNG.
//! Die Weboberfläche nutzt es als CSS-Maske, die Farbe kommt also weiter aus dem Thema (src/icons.ts).
//! SF Symbols dürfen nur auf Apple-Geräten gezeigt werden; darum zeichnet das System sie hier zur Laufzeit,
//! nichts davon liegt im Repository oder im Bundle.

use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSImageSymbolConfiguration};
use objc2_foundation::{NSDictionary, NSString};

/// PNG eines SF Symbols in Grösse `pt` (Punkte, in doppelter Auflösung gezeichnet), None bei unbekanntem Namen.
pub fn png(name: &str, pt: f64) -> Option<Vec<u8>> {
    let img = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None)?;
    let img = img.imageWithSymbolConfiguration(&NSImageSymbolConfiguration::configurationWithPointSize_weight(pt * 2.0, 0.0))?;
    let rep = NSBitmapImageRep::imageRepWithData(&*img.TIFFRepresentation()?)?;
    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some(data.to_vec())
}

#[cfg(test)]
mod tests {
    #[test]
    fn zeichnet_bekanntes_symbol_und_lehnt_unbekanntes_ab() {
        let p = super::png("gearshape", 16.0).expect("gearshape");
        assert_eq!(&p[1..4], b"PNG");
        assert!(super::png("gibt.es.nicht", 16.0).is_none());
    }
}
