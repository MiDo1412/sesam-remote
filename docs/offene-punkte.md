# Offene Punkte

Was bewusst noch fehlt, mit dem Grund und dem nächsten Schritt.

- **Windows Hello für «Passwort anzeigen» (Zugangsdaten):** Auf dem Mac zeigt Sesam ein gespeichertes Passwort erst nach Touch ID bzw. dem Mac-Passwort. Unter Windows sagt der Menüpunkt vorerst, dass es dort noch nicht geht. Nächster Schritt: `UserConsentVerifier` (windows-Crate, Feature `Security_Credentials_UI`) auf einem Windows-Rechner bauen und testen, dann freigeben.
- **Zugang testen:** geht vorerst nur über SSH. Für Verbindungen nur mit RDP oder VNC bräuchte es eine Anmeldung bis nach der Authentifizierung ohne Bild.
- **VNC Tight-JPEG unter Windows und Linux:** dekodiert wird JPEG auf dem Mac vom System (ImageIO). Unter Windows und Linux bittet Sesam den Server vorerst nicht um JPEG; Tight bleibt dort verlustfrei. Nächster Schritt: WIC (Windows) bzw. ein JPEG-Decoder.
- **Qualitätsstufe für VNC:** fest JPEG-Qualität 8 und Kompression 2; eine Einstellung pro Verbindung fehlt.
