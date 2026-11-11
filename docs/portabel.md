# Portable Fassungen

Sesam läuft portabel, wenn neben der App eine Datei `portable` oder ein Ordner `Sesam-Daten` liegt
(`src-tauri/src/portable.rs`). Dann landen die Daten in `Sesam-Daten` statt im Benutzerprofil.

| System | Lieferung | «Neben der App» heisst |
|---|---|---|
| Windows | ZIP mit `sesam-remote.exe`, Lizenzen und der Datei `portable` | Ordner mit `sesam-remote.exe` |
| macOS | ZIP mit `Sesam Remote.app` und der Datei `portable` | Ordner mit `Sesam Remote.app` (nicht im Bündel) |
| Linux | AppImage, daneben `portable` oder `Sesam-Daten` | Ordner mit der `.AppImage`-Datei |

## Was wohin geht

| Daten | portabel | sonst |
|---|---|---|
| Bibliothek, Gruppen, gemerkte Zertifikate, FTP-Sicherungen, Protokolle | `Sesam-Daten/` | Datenordner der App im Benutzerprofil |
| Einstellungen der Oberfläche (Erscheinungsbild, Terminal-Designs …) | Windows und Linux: `Sesam-Daten/` (Webview-Speicher); macOS: im Benutzerprofil, weil sich der Speicher von WKWebView nicht verlegen lässt | Benutzerprofil |
| Zugangsdaten (Passwörter, Schlüssel) | **Schlüsselbund des Systems** | Schlüsselbund des Systems |
| SSH-Host-Schlüssel | `~/.ssh/known_hosts` wie OpenSSH | gleich |
| Temporäres (Vorschau, Ablage) | Temp-Ordner des Systems, wird aufgeräumt | gleich |

Zugangsdaten reisen bewusst **nicht** mit: Einen eigenen verschlüsselten Tresor für unterwegs gibt es nicht.
Wer Bibliothek und Zugangsdaten auf einen anderen Rechner bringen will, nimmt Einstellungen › Zugangsdaten ›
«Sichern …» (verschlüsselte `.sesam`-Datei mit eigenem Passwort) und dort «Einlesen …».

## Hinweise

- Updates: Portable Fassungen aktualisieren sich still an Ort und Stelle (beim Beenden), wenn der Ordner
  beschreibbar ist (`docs/release.md`).
- macOS: Die App ist nicht notarisiert; beim ersten Start Rechtsklick › Öffnen. Von einem schreibgeschützten
  Ort (z. B. direkt aus dem DMG) läuft sie, kann dort aber keinen Ordner `Sesam-Daten` anlegen.
- Windows: WebView2 muss vorhanden sein (Windows 10/11 haben sie).
