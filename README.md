# Sesam Remote

Quelloffene Fernzugriffs-App für Windows, macOS und iPad: VNC-Desktop, SSH-Shell und
SFTP-Dateitransfer in einer Bibliothek. Lizenz: GPL-3.0-or-later (siehe `LICENSE`).

## Zielbild

- **Bibliothek** mit Verbindungen (Name, Gruppe, Adresse, VNC-Port, SSH-Port, Benutzer,
  SSH-Schlüssel, Standardansicht). Doppelklick öffnet die Standardansicht.
- **Tresor:** Passwörter und Schlüssel liegen im Schlüsselbund des Betriebssystems
  (Windows-Anmeldeinformationsverwaltung, macOS-Schlüsselbund). Die Verbindungsliste im
  App-Datenordner (`library.json`) enthält keine Geheimnisse.
- **VNC-Viewer** (RFB, nativ in Rust) mit «An Fenster anpassen» (ExtendedDesktopSize /
  SetDesktopSize), «Skalieren», «Originalgrösse (1:1)», Quer-/Hochformat und Vollbild (F11).
  Ziel-Hosts: wayvnc, GNOME Remote Desktop, macOS-Bildschirmfreigabe (Apple-Auth Typ 30).
- **SSH-Shell** (`russh`) mit Terminal (xterm.js).
- **Dateitransfer** über SFTP (`russh-sftp`), zweispaltig, mit Ziehen und Ablegen.
- **Gemeinsame Zwischenablage:** Text über VNC (Extended Clipboard für UTF-8), Dateien und
  Bilder über SFTP, wenn SSH vorhanden ist.

## Stand

| Bereich | Stand |
|---|---|
| Bibliothek + Tresor | läuft, mit Tests |
| Verbindung anlegen/bearbeiten (Sheet) | läuft |
| SSH-Shell | läuft (Passwort oder ed25519-Schlüssel aus dem Tresor, Fingerabdruck bestätigen) |
| VNC | Anzeige im Fenster, VNC-/Apple-Anmeldung, VeNCrypt (TLS) mit Zertifikat-Pinning, lokaler Mauszeiger; gegen echte Hosts noch ungetestet |
| Dateien (SFTP) | zweispaltig, Ziehen und Ablegen, Übertragungsliste, Umbenennen/Löschen; gegen einen Test-Server geprüft |
| Gemeinsame Zwischenablage | Text über VNC; Dateien und Bilder über SFTP nach `~/Sesam-Ablage`, `file://`-Listen vom Server als lokale Dateien |

## Entwickeln

Voraussetzungen: Rust (stable, MSVC), Node 20+, unter Windows WebView2 und die
MSVC Build Tools.

```bash
npm install
npm run tauri dev
```

Prüfen:

```bash
npm run build
cd src-tauri && cargo test
```

Aufbau und Modulplan: [docs/architektur.md](docs/architektur.md).
