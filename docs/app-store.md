# App Stores: was bereit ist, was die Einreichung blockiert

Stand 11.11.2026. Nichts davon ist eingereicht, hochgeladen oder bestellt. Was ein Konto, Geld oder eine
Entscheidung von Mike braucht, ist als **Blocker** markiert.

## Gemeinsam für alle Stores

- **Version:** eine Quelle, `VERSION`. `node scripts/release.mjs` setzt sie in `package.json`,
  `src-tauri/tauri.conf.json`, `Cargo.toml` und `Cargo.lock`; die Tauri-CLI übernimmt sie beim Bauen in
  `CFBundleShortVersionString` und `CFBundleVersion` (macOS, iPadOS). «Über Sesam» liest sie zur Laufzeit.
  Heute überall `0.1.0` (noch kein Release); das nächste hiesse `26.11.1`. Die Nummern `JJ.M.N` steigen
  immer, taugen also auch als Build-Nummer für App Store Connect und Partner Center.
- **Kategorie und Texte:** `bundle.category = Utility` (Mac: «Dienstprogramme»), Kurz- und Langbeschreibung
  in `tauri.conf.json`. Store-Texte, Screenshots, Datenschutz-URL und Support-URL fehlen noch.
- **Symbole:** alle Grössen in `src-tauri/icons` (macOS `.icns`, Windows `.ico` und Store-Logos, iOS-Satz,
  im Xcode-Projekt eingetragen).
- **Lizenz (Blocker, Entscheidung):** Sesam steht unter GPL-3.0. Die GPL verträgt sich nach verbreiteter
  Auffassung nicht mit den Nutzungsbedingungen des Apple App Store (zusätzliche Einschränkungen für Nutzer);
  beim Microsoft Store ist es unkritischer. Lösbar, weil
  - alle 187 Commits von Mike stammen (alleiniger Urheber, keine fremden Beiträge), er also selbst zusätzlich
    unter anderer Lizenz veröffentlichen kann (Doppellizenz oder Ausnahme für App-Store-Vertrieb), und
  - keine Abhängigkeit unter GPL steht: 760 Rust-Crates und alle npm-Pakete sind MIT/Apache/BSD/ISC,
    einige wenige MPL-2.0 (`cssparser`, `selectors`, `option-ext`, `dtoa-short`; Copyleft nur auf
    Dateiebene, Store-tauglich), `r-efi` wahlweise MIT/Apache. Fremde Farbschemata und Schriften:
    `docs/lizenzen/`.
  Nimmt Sesam später Beiträge von anderen an, braucht es dafür eine Zustimmung (CLA), sonst geht die
  Doppellizenz nicht mehr.
- **Exportkontrolle (Entscheidung):** Sesam verschlüsselt selbst (SSH, TLS über ring/rustls). Für Apple
  heisst das `ITSAppUsesNonExemptEncryption`; ob die Ausnahme für Standardverfahren greift und ob eine
  jährliche Selbstauskunft (US BIS) nötig ist, entscheidet Mike bzw. die Beratung. Darum ist der Schlüssel
  noch **nicht** gesetzt; App Store Connect fragt beim Hochladen danach.

## Mac App Store

Bereit:
- Bundle-ID `ch.sesam.remote`, Kategorie, Symbole, Datenschutz-Manifest
  (`src-tauri/store/PrivacyInfo.xcprivacy`: keine Verfolgung, keine gesammelten Daten, Gründe für
  Dateizeitstempel und Systemuhr).
- Eigene Sandbox-Variante als Overlay: `src-tauri/tauri.mas.conf.json` mit
  `src-tauri/store/mas.entitlements` (Netz als Client und Server für Tunnel, vom Nutzer gewählte Dateien,
  Downloads). Gebaut und geprüft: Manifest und Kategorie landen im Paket.

Blocker:
- **Apple Developer Program (bezahlt, 99 USD/Jahr).** Das heutige kostenlose «Personal Team» kann weder
  signieren mit «Apple Distribution» noch einreichen.
- **Sandbox-Konflikte**, die eine MAS-Variante mit weniger Funktionen nötig machen:
  - lokales Terminal (startet Shells und Programme; in der Sandbox nicht erlaubt),
  - Tastenkürzel an den Remotecomputer (CGEventTap braucht Bedienungshilfen, in der Sandbox verboten),
  - «Mit Administratorrechten» betrifft nur entfernte Rechner (sudo per SSH) und bleibt möglich,
  - Dateifenster ausserhalb von Downloads und gewählten Ordnern (lokale Seite müsste auf
    Ordnerauswahl mit Security-Scoped Bookmarks umgestellt werden),
  - Verknüpfungen auf dem Schreibtisch und Ordnerfreigabe für RDP (Schreiben ausserhalb des Containers),
  - `sesam://`-Links gehen, Hilfsprozess für Tasten nicht.
- **Empfehlung:** zwei Fassungen. Voller Funktionsumfang als notarisierter Direkt-Download (braucht
  ebenfalls das bezahlte Programm, «Developer ID»), dazu eine MAS-Fassung ohne lokales Terminal und
  Tastenkürzel. Die MAS-Fassung braucht im Code noch Schalter, die diese Teile ausblenden (ähnlich wie
  heute für das iPad, `isMobile` bzw. `cfg(desktop)`).

## iPad (App Store)

Bereit:
- Xcode-Projekt `src-tauri/gen/apple` (nur iPad, ab iPadOS 16), Bundle-ID, Symbole, Datenschutz-Manifest,
  Text für «Lokales Netzwerk», Dateifreigabe («Auf meinem iPad › Sesam Remote»). Auf dem echten iPad
  (iPadOS 27.2.1) installiert und gestartet.

Blocker:
- **Apple Developer Program (bezahlt).** Mit dem kostenlosen Team läuft die App nur 7 Tage und lässt sich
  nicht einreichen.
- **Hintergrund:** iPadOS hält SSH-, RDP- und VNC-Verbindungen im Hintergrund nicht offen (kein passender
  Hintergrundmodus); wer die App wechselt, verbindet danach neu. Gehaltene Shells auf dem Server (tmux)
  mildern das. Für die Prüfung ehrlich beschreiben.
- **Lokales Netzwerk:** iPadOS fragt beim ersten Verbinden; ohne Erlaubnis gehen nur Verbindungen ins
  Internet.
- **Exportkontrolle und Lizenz:** siehe oben.
- Ungeprüft auf dem Gerät: RDP, Dateiauswahl beim Hochladen.

## Microsoft Store

Bereit:
- Windows-Installer (MSI/NSIS) aus dem Tauri-Bundler, Store-Logos (`Square*Logo.png`, `StoreLogo.png`).
  Der Microsoft Store nimmt heute auch MSI/EXE-Apps an (Win32-Einreichung); ein MSIX ist dafür nicht nötig.

Blocker:
- **Partner-Center-Konto** (Entwicklerkonto; Kosten je nach Kontotyp, vor dem Anlegen prüfen) und eine **Code-Signatur** für den Installer
  (Zertifikat oder Azure Trusted Signing).
- **Hilfsprozess für Tastenkürzel:** Sesam startet sich selbst mit `--kbhook-helper` für den
  Tastatur-Hook. Als MSI/EXE-Einreichung erlaubt; ein MSIX-Paket bräuchte `runFullTrust` und eine eigene
  Prüfung. Nicht getestet: Der Windows-Rechner (WS-MD01) war von hier aus nicht erreichbar.

## Linux

- Flathub (Flatpak-Manifest) bzw. Snap wären möglich, sind aber nicht vorbereitet: Tauri braucht dort
  WebKitGTK aus der Laufzeit, das Manifest muss alle Rust- und npm-Quellen offline auflisten. Aufwand
  mittel; erst sinnvoll, wenn Linux-Nutzer danach fragen.
