# Windows bauen

Das MSI und die portable Fassung entstehen nur unter Windows (WiX). Alles Nötige liegt im Repository; auf dem
Windows-Rechner reicht holen und bauen.

## Einmal einrichten

- Rust (stable, MSVC-Toolchain `x86_64-pc-windows-msvc`), Visual Studio Build Tools mit «Desktopentwicklung mit
  C++», Node 20+, Git.
- WiX lädt der Tauri-Bundler beim ersten MSI-Build selbst herunter (WiX 3, in den Tauri-Cache).
- Pro Klon: `git config core.hooksPath .githooks`

## Bauen

```powershell
git pull
npm ci
npx tauri build --bundles msi
node scripts/portable.mjs
```

Ergebnis in `test-build\release\<Version>\` (nicht im Git):

- `Sesam Remote_<Version>_x64_de-DE.msi`: Installer pro Computer, Verknüpfung auf dem Desktop und im
  Startmenü, still installierbar (`docs/intune.md`)
- `Sesam-Remote_<Version>_Windows_x64_portabel.zip`: `sesam-remote.exe`, Datei `portable`, Lizenzen
- `SHA256SUMS.txt`: Prüfsummen, ergänzt die Zeilen der anderen Systeme

## Prüfen

```powershell
msiexec /i "test-build\release\<Version>\Sesam Remote_<Version>_x64_de-DE.msi" /qn /norestart /l*v "$env:TEMP\sesam.log"
# Verknüpfungen: C:\Users\Public\Desktop\Sesam Remote.lnk und Startmenü › Sesam Remote
# Programm: C:\Program Files\Sesam Remote\sesam-remote.exe
msiexec /x "test-build\release\<Version>\Sesam Remote_<Version>_x64_de-DE.msi" /qn /norestart
```

Portabel: ZIP entpacken, `sesam-remote.exe` starten; der Ordner `Sesam-Daten` entsteht daneben.

## Linux (WSL oder Linux-Rechner)

```bash
sudo apt install libwebkit2gtk-4.1-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
npm ci
npx tauri build --bundles appimage,deb
node scripts/portable.mjs
```

Konfiguration: `src-tauri/tauri.conf.json` › `bundle.windows` (UpgradeCode, Sprache, WebView2). Das MSI nutzt
die Standardvorlage von Tauri: pro Computer, Desktop- und Startmenü-Verknüpfung ohne Bedingung, kein Neustart.
