//! SF Symbols für die Oberfläche auf macOS: `sfsymbol://localhost/<name>` liefert das Symbol als PNG.
//! Die Weboberfläche nutzt es als CSS-Maske, die Farbe kommt also weiter aus dem Thema (src/icons.ts).
//! SF Symbols dürfen nur auf Apple-Geräten gezeigt werden; darum zeichnet das System sie hier zur Laufzeit,
//! nichts davon liegt im Repository oder im Bundle.

use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSImageSymbolConfiguration, NSWorkspace, NSWorkspaceIconCreationOptions};
use objc2_foundation::{NSDictionary, NSString};

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

#[cfg(test)]
mod tests {
    #[test]
    fn zeichnet_bekanntes_symbol_und_lehnt_unbekanntes_ab() {
        let p = super::png("gearshape", 16.0).expect("gearshape");
        assert_eq!(&p[1..4], b"PNG");
        assert!(super::png("gibt.es.nicht", 16.0).is_none());
    }
}
