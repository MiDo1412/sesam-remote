# Beschriftungen

Begriffe wie in macOS, Windows und gängigen Remote-Desktop-Apps. Keine eigenen Wortschöpfungen.

## Verbindungs-Sheet

| Vorher | Jetzt | Grund |
|---|---|---|
| Adresse | Hostname oder IP-Adresse | gebräuchlich (Windows RDP, macOS) |
| Benutzer | Benutzername | gebräuchlich |
| Passwort des Benutzers | Passwort | ein Passwort pro Verbindung |
| Abweichende Zugangsdaten … | Abweichende Passwörter … | enthält nur Passwörter |
| Desktop (RDP) | Remotedesktop (RDP) | Windows-Begriff |
| Desktop (VNC) | Bildschirmfreigabe (VNC) | macOS-Begriff |
| Aktiv | Ein | Schalter-Beschriftung wie in Systemeinstellungen |
| Ausrichtung des Monitors | Ausrichtung | kürzer |
| Shell und Dateien (SSH mit SFTP) | Shell und Dateien (SSH, SFTP) | kürzer |

Neu gegliedert in **Allgemein**, **Anmeldung**, **Dienste**, **Anzeige**, **Erweitert**.
Felder eines Dienstes erscheinen nur, wenn der Dienst eingeschaltet ist. Jedes Feld hat ein «?»
mit kurzer Hilfe (was eintragen, Beispiel, Pflicht oder optional).

## App

| Ort | Beschriftung |
|---|---|
| Menü | Ablage › Neue Verbindung …, Schnelle Verbindung …, Einstellungen … |
| Einstellungen | Allgemein (Erscheinungsbild), Shell, Sicherheit, Über Sesam |
| Sitzung | ‹ Bibliothek (Sitzung läuft weiter), Trennen (beendet) |

## Begriffe je System

Sesam nutzt die Begriffe des jeweiligen Systems. Windows: Menü im Sesam-Logo der Titelleiste.
macOS: native Menüleiste oben am Bildschirm.

| Zweck | Windows | macOS |
|---|---|---|
| Menüs | Datei · Bearbeiten · Ansicht · Sitzung · Hilfe | Sesam Remote · Ablage · Bearbeiten · Darstellung · Sitzung · Fenster · Hilfe |
| App-Menü | – (im Menü «Datei») | Über Sesam Remote · Einstellungen … · Dienste · Ausblenden · Sesam Remote beenden |
| Einstellungen | Datei › Einstellungen (Strg+,) | Sesam Remote › Einstellungen … (⌘,) |
| Beenden | Datei › Beenden (Alt+F4) | Sesam Remote › Sesam Remote beenden (⌘Q) |
| Über | Hilfe › Über Sesam Remote | Sesam Remote › Über Sesam Remote |
| Einfügen | Einfügen (Strg+V) | Einsetzen (⌘V) |
| Fenster verkleinern | Minimieren | Im Dock ablegen |
| Fenster vergrössern | Maximieren / Wiederherstellen | Zoomen |
| Arbeitsfläche | Desktop | Schreibtisch |
| Dateimanager | Explorer | Finder |
| Kürzel | Strg · Umschalt · Alt | ⌘ · ⇧ · ⌥ |

Die Kürzel in Tooltips wandelt `keyLabel()` (src/titlebar.ts) je System um.
