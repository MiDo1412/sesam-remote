# Vertrieb: was bereit ist, was noch blockiert

Entscheid (11.11.2026): **Mac** nur als Download von der Website (signiert und notarisiert), **iPad** über den
App Store, **Windows** über den Microsoft Store und/oder die Website, **Linux** vorerst nur als Download.

Stand 11.11.2026. Nichts davon ist eingereicht, hochgeladen oder bestellt. Was ein Konto, Geld oder eine
Entscheidung von Mike braucht, ist als **Blocker** markiert.

## Gemeinsam

- **Version:** eine Quelle, `VERSION`. `node scripts/release.mjs` setzt sie in `package.json`,
  `src-tauri/tauri.conf.json`, `Cargo.toml` und `Cargo.lock`; die Tauri-CLI übernimmt sie beim Bauen in
  `CFBundleShortVersionString` und `CFBundleVersion` (macOS, iPadOS). «Über Sesam» liest sie zur Laufzeit.
  Heute `0.1.0` (noch kein Release); Builds dazwischen zeigen «26.11.1 Entwicklungsstand · Commit · Datum»
  (`vite.config.ts`). Die Nummern `JJ.M.N` steigen
  immer, taugen also auch als Build-Nummer für App Store Connect und Partner Center.
- **Kategorie und Texte:** `bundle.category = Utility` (Mac: «Dienstprogramme»), Kurz- und Langbeschreibung
  in `tauri.conf.json`. iPad: Store-Texte, App-Datenschutz, Altersfreigabe, Exportkontrolle, Prüfhinweise und
  Checkliste in `docs/app-store/` (Screenshots in `test-build/appstore/screenshots`, nicht im Git). Windows: fehlt noch.
- **Symbole:** alle Grössen in `src-tauri/icons` (macOS `.icns`, Windows `.ico` und Store-Logos, iOS-Satz,
  im Xcode-Projekt eingetragen).
- **Lizenz:** Sesam steht seit dem 11.11.2026 unter der Mozilla Public License 2.0 (vorher GPL-3.0; Mike ist
  alleiniger Urheber, alle Commits von ihm). Die MPL verlangt nur, dass Sesams eigene Dateien offen bleiben,
  und verträgt sich mit den Bedingungen von App Store und Microsoft Store. Keine Abhängigkeit steht unter
  GPL: 760 Rust-Crates und alle npm-Pakete sind MIT/Apache/BSD/ISC, einige MPL-2.0 (`cssparser`,
  `selectors`, `option-ext`, `dtoa-short`), `r-efi` wahlweise MIT/Apache. Fremde Farbschemata und
  Schriften: `docs/lizenzen/`. Nimmt Sesam Beiträge von anderen an, gelten sie unter derselben Lizenz.
- **Exportkontrolle (Entscheidung):** Sesam verschlüsselt selbst (SSH, TLS über ring/rustls). Für Apple
  heisst das `ITSAppUsesNonExemptEncryption`; ob die Ausnahme für Standardverfahren greift und ob eine
  jährliche Selbstauskunft (US BIS) nötig ist, entscheidet Mike bzw. die Beratung. Darum ist der Schlüssel
  noch **nicht** gesetzt; App Store Connect fragt beim Hochladen danach. Einordnung und Empfehlung:
  `docs/app-store/export-compliance.md`.

## Mac: Download von der Website

Bereit:
- Release-Build als `.app` (`npx tauri build --bundles app`), Bundle-ID `ch.sesam.remote`, Kategorie, Symbole,
  Info.plist mit Text für «Lokales Netzwerk». Heute nur ad-hoc signiert: Gatekeeper warnt beim ersten Öffnen.
- Kein Sandbox-Zwang: lokales Terminal, Tastenkürzel an den Remotecomputer (Bedienungshilfen),
  Verknüpfungen und RDP-Ordnerfreigabe bleiben möglich.

Was es für einen Download ohne Warnung braucht:
- **Apple Developer Program (bezahlt, 99 USD/Jahr).** Das heutige kostenlose «Personal Team» kann kein
  «Developer ID Application»-Zertifikat ausstellen.
- **Zertifikat «Developer ID Application»** (in Xcode › Settings › Apple Accounts › Manage Certificates
  oder auf developer.apple.com) im Schlüsselbund; in `tauri.conf.json` `bundle.macOS.signingIdentity` darauf
  setzen bzw. beim Bauen `APPLE_SIGNING_IDENTITY`.
- **Notarisierung:** Tauri reicht beim Bauen selbst ein, wenn `APPLE_API_KEY`, `APPLE_API_ISSUER` und
  `APPLE_API_KEY_PATH` (App-Store-Connect-API-Schlüssel) oder `APPLE_ID`, `APPLE_PASSWORD`
  (app-spezifisches Passwort) und `APPLE_TEAM_ID` gesetzt sind. Nie ins Repository.
- **Hardened Runtime** (setzt Tauri beim Signieren) und für die Verteilung ein `.dmg`
  (`--bundles dmg`) statt der nackten `.app`.
- **Website:** Download-Seite, Prüfsumme, Hinweis auf die Freigaben (Lokales Netzwerk, Bedienungshilfen).

## iPad (App Store)

Bereit:
- Xcode-Projekt `src-tauri/gen/apple` (nur iPad, ab iPadOS 16), Bundle-ID, Symbole, Datenschutz-Manifest
  (`sesam-remote_iOS/PrivacyInfo.xcprivacy`: keine Verfolgung, keine gesammelten Daten, Gründe für
  Dateizeitstempel und Systemuhr),
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

## Windows: Microsoft Store und/oder Website

Bereit:
- Windows-Installer (MSI/NSIS) aus dem Tauri-Bundler, Store-Logos (`Square*Logo.png`, `StoreLogo.png`).
  Der Microsoft Store nimmt heute auch MSI/EXE-Apps an (Win32-Einreichung); ein MSIX ist dafür nicht nötig.

Blocker:
- **Partner-Center-Konto** für den Store (Kosten je nach Kontotyp, vor dem Anlegen prüfen).
- **Code-Signatur** für den Installer (Zertifikat oder Azure Trusted Signing), für den Store und für den
  Download von der Website, sonst warnt SmartScreen.
- **Hilfsprozess für Tastenkürzel:** Sesam startet sich selbst mit `--kbhook-helper` für den
  Tastatur-Hook. Als MSI/EXE-Einreichung erlaubt; ein MSIX-Paket bräuchte `runFullTrust` und eine eigene
  Prüfung. Nicht getestet: Ein Windows-Rechner war von hier aus nicht erreichbar.

## Linux

- Flathub (Flatpak-Manifest) bzw. Snap wären möglich, sind aber nicht vorbereitet: Tauri braucht dort
  WebKitGTK aus der Laufzeit, das Manifest muss alle Rust- und npm-Quellen offline auflisten. Aufwand
  mittel; erst sinnvoll, wenn Linux-Nutzer danach fragen.
