# Offene Punkte

Was bewusst noch fehlt, mit dem Grund und dem nächsten Schritt.

- **Windows Hello für «Passwort anzeigen» (Zugangsdaten):** Auf dem Mac zeigt Sesam ein gespeichertes Passwort erst nach Touch ID bzw. dem Mac-Passwort. Unter Windows sagt der Menüpunkt vorerst, dass es dort noch nicht geht. Nächster Schritt: `UserConsentVerifier` (windows-Crate, Feature `Security_Credentials_UI`) auf einem Windows-Rechner bauen und testen, dann freigeben.
- **Zugang testen:** geht vorerst nur über SSH. Für Verbindungen nur mit RDP oder VNC bräuchte es eine Anmeldung bis nach der Authentifizierung ohne Bild.
- **VNC Tight-JPEG unter Windows und Linux:** dekodiert wird JPEG auf dem Mac vom System (ImageIO). Unter Windows und Linux bittet Sesam den Server vorerst nicht um JPEG; Tight bleibt dort verlustfrei. Nächster Schritt: WIC (Windows) bzw. ein JPEG-Decoder.
- **Qualitätsstufe für VNC:** fest JPEG-Qualität 8 und Kompression 2; eine Einstellung pro Verbindung fehlt.
- **VNC-Anmeldung RA2/RA2ne (RealVNC, ältere Raspberry Pi OS):** RealVNC Server meldet sich standardmässig mit RA2 bzw. RA2ne (Typen 5/6) an, einem nicht offengelegten Verfahren. Sesam kann Keine, VNC-Passwort, Apple (ARD) und VeNCrypt mit X509 (Plain, VNC, None; mit Zertifikat-Pinning). wayvnc auf aktuellem Raspberry Pi OS bietet VeNCrypt X509Plain und funktioniert. Für RealVNC: dort «VNC-Passwort» als Anmeldung einstellen oder auf wayvnc wechseln. RSA-AES (TigerVNC/wayvnc, Typ 129) wäre mit dem rsa-Crate aus dem Baum machbar, ist aber nicht gebaut.
- **Papierkorb:** Einen Texteditor mit Änderungsvorschau gibt es in Sesam nicht; der Punkt «Änderungen vor dem Speichern zeigen» entfällt, bis es einen Editor gibt.

## iPad (iPadOS)

- Auf dem echten iPad und im Simulator geprüft: Bibliothek, Einstellungen, SSH mit Schlüssel aus dem
  iOS-Schlüsselbund, Dateien (auch Ordner aus «Dateien»), VNC mit Touch und Vollbild, Erste Hilfe, Face ID.
- Ungeprüft: RDP (kein RDP-Server zum Testen).
- Zwischenablage: Dateien und Bilder auf dem iPad noch nicht; kein Verlauf der Zwischenablage (iOS fragt bei
  jedem Lesen).
- App Store: braucht das bezahlte Entwicklerkonto (`docs/app-store.md`); bis dahin nur auf eigenen Geräten.

## Vertrieb

- **macOS ohne Developer ID:** Die Builds sind nur ad hoc signiert und nicht notarisiert. Gatekeeper warnt
  beim ersten Start («nicht verifizierter Entwickler»); öffnen mit Rechtsklick › Öffnen. Nächster Schritt:
  Zertifikat «Developer ID Application» und Notarisierung (bezahltes Konto).
- **Windows ohne Code-Signatur:** MSI und EXE sind unsigniert; SmartScreen warnt beim Download, Intune
  installiert trotzdem. Nächster Schritt: Code-Signing-Zertifikat.
- **Windows und Linux bauen:** geht nur auf dem jeweiligen System (WiX bzw. AppImage-Werkzeuge). Auf dem Mac
  vorbereitet, nicht gebaut; es gibt keinen CI-Lauf.

## Sicherheit (aus `docs/security-review-2026-11-11.md`)

- **iPad: Stammzertifikate für RDP/RD-Gateway:** `webpki-roots` oder `rustls-platform-verifier` einbauen (neue
  Abhängigkeit, braucht OK), dann RDP auf dem iPad prüfen.
- **Sicherung unter Windows/Linux ohne Bestätigung:** mit Windows Hello zusammen lösen.
- **SSH:** Host-Key-Algorithmen auf die bekannten Typen einschränken (statt nur zu warnen).
- **«Bearbeiten»:** Quarantäne (macOS) bzw. Mark-of-the-Web (Windows) auf heruntergeladene Dateien setzen.
- **Einlesen einer Sicherung:** Portweiterleitungen und Ordnerfreigabe in der Vorschau zeigen.
- **Grenzen:** rekursives Herunterladen (Tiefe, Menge), Zwischenablage-Grenze auch während des Herunterladens,
  Tight-JPEG-Grösse vor dem Dekodieren.
- **Signatur:** Developer ID mit Notarisierung, Authenticode.
