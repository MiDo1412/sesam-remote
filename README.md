# Sesam Remote

Quelloffene Fernzugriffs-App für macOS, Windows, Linux und iPad: Bildschirm per RDP und VNC, Shell per SSH,
Dateien per SFTP und FTP/FTPS – in einer Bibliothek. Lizenz: Mozilla Public License 2.0 (MPL-2.0, siehe
`LICENSE`).

## Was Sesam kann

- **Bibliothek** mit Gruppen, Suche, Karten und Liste, Online-Status und Aufwecken (Wake-on-LAN).
- **Remotedesktop** per RDP (IronRDP, mit RD-Gateway, H.264, Ton, Zwischenablage, Laufwerk) und VNC (nativ in
  Rust: VNC-Passwort, Apple-Anmeldung, VeNCrypt mit Zertifikat-Pinning, Tight/ZRLE).
- **SSH-Shell** (russh, xterm.js) mit Schlüsseln, Agent, Sprungserver, Portweiterleitungen und tmux-Sitzungen,
  die einen Abbruch überstehen.
- **Dateien** zweispaltig über SFTP, SCP oder FTP/FTPS, mit Papierkorb und «Rückgängig», Konfliktabfrage,
  Vorschau, Bearbeiten im Standardprogramm und Fortsetzen nach einem Abbruch.
- **Zugangsdaten** nur im Schlüsselbund des Systems (macOS- bzw. iOS-Schlüsselbund, Windows-
  Anmeldeinformationsverwaltung, Secret Service unter Linux). Die Bibliothek (`library.json`) enthält keine
  Geheimnisse; Sicherungen sind mit eigenem Passwort verschlüsselt (AES-256-GCM, Argon2id).

Was auf welchem System geht: [docs/plattformen.md](docs/plattformen.md).

## Installieren

- **macOS:** `.dmg` bzw. portabel als `.app` im ZIP.
- **Windows:** MSI (Verknüpfung auf dem Desktop und im Startmenü, still installierbar, auch über Intune:
  [docs/intune.md](docs/intune.md)) oder portabel als ZIP.
- **Linux:** AppImage (portabel) bzw. `.deb`.
- **iPad:** App Store (geplant, [docs/app-store.md](docs/app-store.md)).

Portabler Betrieb: Liegt neben der App eine Datei `portable` oder ein Ordner `Sesam-Daten`, legt Sesam
Bibliothek und Einstellungen dort ab ([docs/portabel.md](docs/portabel.md)).

## Entwickeln

Voraussetzungen: Rust (stable), Node 20+; unter Windows WebView2 und die MSVC Build Tools; für das iPad Xcode.

```bash
npm install
npm run tauri dev
```

Prüfen:

```bash
npm run build
cd src-tauri && cargo test
```

Release (Nummer JJ.M.N, Abschnitt «In Arbeit» der Release-Notes): [docs/versionierung.md](docs/versionierung.md).

## Dokumentation

| Datei | Inhalt |
|---|---|
| [docs/architektur.md](docs/architektur.md) | Aufbau und Module |
| [docs/plattformen.md](docs/plattformen.md) | Funktionen je System, Vertrieb |
| [docs/design-konzept.md](docs/design-konzept.md) | Gestaltung «Inhalt und Glas», Tokens, Knöpfe, Blätter |
| [docs/design-inventar.md](docs/design-inventar.md) | Prüfstand jedes Bildschirms, hell und dunkel |
| [docs/beschriftungen.md](docs/beschriftungen.md) | Begriffe in der Oberfläche |
| [docs/release-notes.md](docs/release-notes.md) | Änderungen je Version |
| [docs/versionierung.md](docs/versionierung.md) | Versionsnummern und Release |
| [docs/app-store.md](docs/app-store.md) | Stores und Signieren |
| [docs/intune.md](docs/intune.md) | Windows: MSI über Intune verteilen |
| [docs/portabel.md](docs/portabel.md) | Portable Fassungen |
| [docs/security-review-2026-11-11.md](docs/security-review-2026-11-11.md) | Sicherheitsprüfung |
| [docs/offene-punkte.md](docs/offene-punkte.md) | Bewusst Offenes |
| [docs/hilfe/](docs/hilfe/) | Anleitungen in der App |
