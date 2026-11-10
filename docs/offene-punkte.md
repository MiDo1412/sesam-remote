# Offene Punkte

Was bewusst noch fehlt, mit dem Grund und dem nächsten Schritt.

- **Windows Hello für «Passwort anzeigen» (Zugangsdaten):** Auf dem Mac zeigt Sesam ein gespeichertes Passwort erst nach Touch ID bzw. dem Mac-Passwort. Unter Windows sagt der Menüpunkt vorerst, dass es dort noch nicht geht. Nächster Schritt: `UserConsentVerifier` (windows-Crate, Feature `Security_Credentials_UI`) auf einem Windows-Rechner bauen und testen, dann freigeben.
- **Zugang testen:** geht vorerst nur über SSH. Für Verbindungen nur mit RDP oder VNC bräuchte es eine Anmeldung bis nach der Authentifizierung ohne Bild.
- **VNC Tight-JPEG unter Windows und Linux:** dekodiert wird JPEG auf dem Mac vom System (ImageIO). Unter Windows und Linux bittet Sesam den Server vorerst nicht um JPEG; Tight bleibt dort verlustfrei. Nächster Schritt: WIC (Windows) bzw. ein JPEG-Decoder.
- **Qualitätsstufe für VNC:** fest JPEG-Qualität 8 und Kompression 2; eine Einstellung pro Verbindung fehlt.
- **VNC-Anmeldung RA2/RA2ne (RealVNC, ältere Raspberry Pi OS):** RealVNC Server meldet sich standardmässig mit RA2 bzw. RA2ne (Typen 5/6) an, einem nicht offengelegten Verfahren. Sesam kann Keine, VNC-Passwort, Apple (ARD) und VeNCrypt mit X509 (Plain, VNC, None; mit Zertifikat-Pinning). wayvnc auf aktuellem Raspberry Pi OS bietet VeNCrypt X509Plain und funktioniert. Für RealVNC: dort «VNC-Passwort» als Anmeldung einstellen oder auf wayvnc wechseln. RSA-AES (TigerVNC/wayvnc, Typ 129) wäre mit dem rsa-Crate aus dem Baum machbar, ist aber nicht gebaut.
- **Papierkorb:** Einen Texteditor mit Änderungsvorschau gibt es in Sesam nicht; der Punkt «Änderungen vor dem Speichern zeigen» entfällt, bis es einen Editor gibt.

## iPad (iPadOS)

- Im Simulator geprüft (iPad Pro 13"): Bibliothek, Einstellungen, Kontextmenüs per langem Drücken, SSH mit Schlüssel aus dem iOS-Schlüsselbund, Zusatzleiste, Erste Hilfe, VNC mit Touch, Zwischenablage (Kopieren direkt, Einfügen mit Bestätigung von iOS), Dateien («Auf meinem iPad», Herunterladen; Ordner erscheint in der Dateien-App). Start im Release-Build rund 2 s (Debug rund 10 s).
- Bauen: `npx tauri ios build --target aarch64-sim` (Simulator) bzw. für das Gerät mit Team-ID (`APPLE_DEVELOPMENT_TEAM`, nicht im Repository).
- Ungeprüft: RDP (kein RDP-Server zum Testen), Auswahl in der Dateiauswahl beim Hochladen (im Simulator nicht bedienbar), echtes Gerät.
- Zwischenablage: Dateien und Bilder auf dem iPad noch nicht; kein Verlauf der Zwischenablage (iOS fragt bei jedem Lesen).
- H.264 (VideoToolbox) gibt es auch auf iOS; `h264.rs` ist bisher nur für macOS freigeschaltet.
- Der «protected»-Speicher braucht je nach Signatur eine Keychain-Access-Group in den Entitlements (im Simulator nicht nötig).
