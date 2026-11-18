# Release bauen, signieren, verteilen

Ergänzt `docs/versionierung.md` (Nummer, Release-Notes) um Bauen und automatische Updates.

## Signaturschlüssel für Updates (einmalig)

Jedes Update ist mit ed25519 (minisign) signiert; die App installiert nur, was mit dem passenden Schlüssel
signiert ist und dessen Signatur die angekündigte Version enthält (`requireSignedVersion`).

```bash
scripts/updater-key.sh
```

- Privater Schlüssel: `~/.sesam/updater.key` (mit Passwort), **nie** ins Repository.
- Passwort: Schlüsselbund, Eintrag «Sesam Remote Updater».
- Öffentlicher Schlüssel: `src-tauri/tauri.conf.json` › `plugins.updater.pubkey` (committen).

**Sichern:** `~/.sesam/updater.key` und das Passwort gehören in einen Passwort-Manager oder Tresor (zwei
getrennte Orte). Geht der Schlüssel verloren, kann keine installierte Sesam-Version mehr automatisch
aktualisiert werden; dann hilft nur eine neue Version von Hand mit neuem öffentlichem Schlüssel.

## Bauen und signieren

macOS (Apple Silicon), signiert mit «Developer ID Application» und notarisiert:

```bash
scripts/mac-release.sh
```

Das Skript nimmt die Identität «Developer ID Application: …» aus dem Schlüsselbund, notarisiert mit dem
App-Store-Connect-Schlüssel (wie `scripts/asc-upload.sh`), prüft Signatur, Ticket und Gatekeeper und erstellt
danach portables ZIP und `latest.json`. Ohne Developer ID wäre die App nur ad hoc signiert: Dann bindet macOS
die Schlüsselbund-Freigabe an genau eine Programmdatei und fragt nach jedem Update erneut. Mit Developer ID
fragt macOS nach dem Wechsel noch einmal, danach auch über Updates hinweg nicht mehr. Entwicklungsbuilds
bleiben ad hoc signiert (`signingIdentity: "-"` in `tauri.conf.json`).

Windows (`docs/build-windows.md`): ohne Schlüssel bauen, dann die Dateien zum Mac bringen und dort
signieren:

```bash
npx tauri signer sign -f ~/.sesam/updater.key --app-version <Version> "Sesam Remote_<Version>_x64_de-DE.msi"
npx tauri signer sign -f ~/.sesam/updater.key --app-version <Version> sesam-remote.exe
node scripts/updates.mjs windows-x86_64 "Sesam Remote_<Version>_x64_de-DE.msi"
node scripts/updates.mjs windows-x86_64-portable sesam-remote.exe
```

Linux entsprechend mit dem AppImage (`linux-x86_64`).

## Veröffentlichen

`node scripts/release.mjs publish` hängt alles aus `test-build/release/<Version>/github/` ans GitHub-Release:
Installer mit Version im Namen und mit festem Namen (`Sesam-Remote_macOS.dmg`, `Sesam-Remote_Windows.msi`,
`…_portabel.zip`), `latest.json`, Update-Dateien samt `.sig`, `SHA256SUMS.txt`. Die App holt Updates zuerst
von `https://github.com/MiDo1412/sesam-remote/releases/latest/download/latest.json`; die Website verlinkt die
festen Namen unter `releases/latest/download/…` und lädt so immer direkt die neueste Datei.

Freiwillig (Rückfall, falls GitHub einmal wegfällt): `test-build/release/<Version>/updates/` nach
`https://sesam-remote.app/updates/` kopieren, zuletzt `latest.json`.

## Wie die App aktualisiert

- Prüft 45 s nach dem Start und dann alle 6 Stunden, lädt still herunter, prüft die Signatur und installiert
  beim Beenden; der nächste Start ist neu. Keine Fenster, keine Unterbrechung einer Sitzung.
- Einstellungen › Über Sesam: «Sesam ist aktuell (…)» bzw. «Update … wird beim nächsten Start installiert»,
  Schalter «Automatisch aktualisieren» (Vorgabe an), «Jetzt suchen».
- Windows-MSI im Programme-Ordner: braucht Administratorrechte, darum nur Hinweis und «Jetzt installieren»;
  mit `AUTOUPDATE=0` bzw. der Richtlinie ganz aus (`docs/intune.md`).
- Portabel (Windows, macOS, AppImage): ersetzt sich an Ort und Stelle.
- iPad: App Store.
- Nur neuere Versionen; die Signatur bindet die Version (kein Zurückstufen über eine gefälschte latest.json).

Geprüft (11.11.2026, Mac, lokaler Testserver, Wegwerf-Schlüssel): stilles Herunterladen, Installation beim
Beenden, nächster Start meldet die neue Version; eine latest.json mit falscher Version wird abgelehnt, eine ältere
Version nicht installiert.
