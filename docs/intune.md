# Windows: MSI über Intune verteilen

Das MSI baut der Tauri-Bundler mit WiX (`npx tauri build --bundles msi`, nur unter Windows). Einstellungen in
`src-tauri/tauri.conf.json` unter `bundle.windows`.

## Eigenschaften

| | |
|---|---|
| Datei | `Sesam Remote_<Version>_x64_de-DE.msi` |
| Installation | pro Computer (`InstallScope="perMachine"`), nach `C:\Program Files\Sesam Remote\` |
| Programm | `C:\Program Files\Sesam Remote\sesam-remote.exe` |
| Verknüpfungen | Desktop (öffentlicher Desktop) und Startmenü, ohne Bedingung (Tauri-Standardvorlage) |
| UpgradeCode | `26F306D2-BA30-5F9C-B735-610DBA9E864E`, bleibt für alle Versionen gleich: eine neue Version ersetzt die alte (Major Upgrade), nichts doppelt |
| ProductCode | neu je Build; steht nach der Installation unter `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{…}` (DisplayName «Sesam Remote») |
| Neustart | keiner; kein Dialog bei `/qn` |
| WebView2 | auf Windows 10/11 vorhanden; fehlt sie, lädt das MSI den Bootstrapper still nach (braucht Internet) |
| Signatur | noch keine (SmartScreen warnt beim Download, Intune installiert trotzdem) |

## Befehle

Installieren, still:

```bat
msiexec /i "Sesam Remote_26.11.1_x64_de-DE.msi" /qn /norestart /l*v "%TEMP%\sesam-install.log"
```

Deinstallieren, still (Produktcode aus dem Uninstall-Schlüssel oder aus der Intune-Erkennung):

```bat
msiexec /x {PRODUCT-CODE} /qn /norestart
```

oder mit dem MSI selbst:

```bat
msiexec /x "Sesam Remote_26.11.1_x64_de-DE.msi" /qn /norestart
```

## In Intune

**Variante A – Branchen-App (LOB, MSI):** MSI hochladen; Intune liest Produktcode und Version selbst und
installiert im Gerätekontext mit `/qn`. Einfach, aber jede Version ist eine neue App.

**Variante B – Win32-App (.intunewin, empfohlen):**

- Paket mit dem *Microsoft Win32 Content Prep Tool* aus dem Ordner mit dem MSI erstellen.
- Installationsbefehl: `msiexec /i "Sesam Remote_26.11.1_x64_de-DE.msi" /qn /norestart`
- Deinstallationsbefehl: `msiexec /x {PRODUCT-CODE} /qn /norestart`
- Installationsverhalten: **System**; Neustart: **Keine bestimmte Aktion**.
- Erkennungsregel (eine davon):
  - **MSI:** Produktcode des MSI (Intune füllt ihn beim Hochladen aus), optional Versionsprüfung.
  - **Datei:** `C:\Program Files\Sesam Remote\sesam-remote.exe`, Dateiversion grösser oder gleich `26.11.1`.
    Diese Regel bleibt über Updates gleich.
- Rückgabecodes: Standard (0, 1707, 3010, 1641, 1618).

Updates: neue Version als neue Win32-App mit «Ersetzung» der alten oder einfach neues MSI in derselben App; dank
gleicher UpgradeCode entfernt das MSI die alte Version selbst.

## Was das Benutzerprofil bekommt

Bibliothek und Einstellungen liegen pro Benutzer unter `%APPDATA%\ch.sesam.remote\`, Zugangsdaten in der
Windows-Anmeldeinformationsverwaltung. Die Deinstallation lässt beides stehen.
