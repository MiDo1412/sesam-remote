# Offene Punkte

Was bewusst noch fehlt, mit dem Grund und dem nächsten Schritt.

- **Windows Hello für «Passwort anzeigen» (Zugangsdaten):** Auf dem Mac zeigt Sesam ein gespeichertes Passwort erst nach Touch ID bzw. dem Mac-Passwort. Unter Windows sagt der Menüpunkt vorerst, dass es dort noch nicht geht. Nächster Schritt: `UserConsentVerifier` (windows-Crate, Feature `Security_Credentials_UI`) auf einem Windows-Rechner bauen und testen, dann freigeben.
- **Zugang testen:** geht vorerst nur über SSH. Für Verbindungen nur mit RDP oder VNC bräuchte es eine Anmeldung bis nach der Authentifizierung ohne Bild.
- **VNC Tight-JPEG unter Windows und Linux:** dekodiert wird JPEG auf dem Mac vom System (ImageIO). Unter Windows und Linux bittet Sesam den Server vorerst nicht um JPEG; Tight bleibt dort verlustfrei. Nächster Schritt: WIC (Windows) bzw. ein JPEG-Decoder.
- **Qualitätsstufe für VNC:** fest JPEG-Qualität 8 und Kompression 2; eine Einstellung pro Verbindung fehlt.
- **VNC-Anmeldung RA2/RA2ne (RealVNC, ältere Raspberry Pi OS):** RealVNC Server meldet sich standardmässig mit RA2 bzw. RA2ne (Typen 5/6) an, einem nicht offengelegten Verfahren. Sesam kann Keine, VNC-Passwort, Apple (ARD) und VeNCrypt mit X509 (Plain, VNC, None; mit Zertifikat-Pinning). wayvnc auf aktuellem Raspberry Pi OS bietet VeNCrypt X509Plain und funktioniert. Für RealVNC: dort «VNC-Passwort» als Anmeldung einstellen oder auf wayvnc wechseln. RSA-AES (TigerVNC/wayvnc, Typ 129) wäre mit dem rsa-Crate aus dem Baum machbar, ist aber nicht gebaut.
- **Papierkorb:** Ein Grössenlimit fehlt noch (nur Altersgrenze). Beim Ersetzen mit Administratorrechten sichert Sesam die alte Fassung nicht im Papierkorb. Einen Texteditor mit Änderungsvorschau gibt es in Sesam nicht; der Punkt «Änderungen vor dem Speichern zeigen» entfällt, bis es einen Editor gibt.
