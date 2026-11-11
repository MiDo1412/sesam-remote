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

macOS (Apple Silicon):

```bash
export TAURI_SIGNING_PRIVATE_KEY="$HOME/.sesam/updater.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s 'Sesam Remote Updater' -a updater -w)"
npx tauri build --bundles app,dmg --config '{"bundle":{"createUpdaterArtifacts":true}}'
node scripts/portable.mjs
node scripts/updates.mjs
unset TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

Windows (`docs/build-windows.md`): ohne Schlüssel bauen, dann die Dateien zum Mac bringen und dort
signieren:

```bash
npx tauri signer sign -f ~/.sesam/updater.key "Sesam Remote_<Version>_x64_de-DE.msi"
npx tauri signer sign -f ~/.sesam/updater.key sesam-remote.exe
node scripts/updates.mjs windows-x86_64 "Sesam Remote_<Version>_x64_de-DE.msi"
node scripts/updates.mjs windows-x86_64-portable sesam-remote.exe
```

Linux entsprechend mit dem AppImage (`linux-x86_64`).

## Hochladen

`test-build/release/<Version>/updates/` auf die Website nach `https://sesam-remote.app/updates/` kopieren:
`latest.json` und den Ordner `<Version>/` mit den Dateien und `.sig`. Erst die Dateien, zuletzt
`latest.json` (sonst zeigt sie kurz auf Fehlendes). Nur über HTTPS.

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
