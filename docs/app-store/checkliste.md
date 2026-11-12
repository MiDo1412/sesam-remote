# Checkliste: was noch das bezahlte Entwicklerprogramm braucht

Stand 12.11.2026. Vorbereitet ohne Anmeldung bei Apple; nichts ist registriert, angelegt oder hochgeladen.

## Erledigt (im Repository)

- Texte: `metadata-de.md` (Hauptsprache), `metadata-en.md` (optional)
- App-Datenschutz: `app-privacy.md` (Ergebnis «Keine Daten erfasst», passt zu `PrivacyInfo.xcprivacy`)
- Altersfreigabe: `age-rating.md` (4+)
- Exportkontrolle: `export-compliance.md` (Standardverfahren; Frankreich zuerst abwählen; kein Jahresbericht)
- Hinweise für App Review: `review-notes.md` (Test-Server nötig, Entscheid offen)
- Version 26.11.3 in `Info.plist` (setzt die Tauri-CLI beim Bauen ohnehin aus `VERSION`)
- Screenshots iPad 13" quer (2752×2064), aus dem Simulator mit Demo-Daten: `test-build/appstore/screenshots/`
  (nicht im Git). Apple verlangt für iPad nur die 13"-Grösse; 11" und 12,9" leitet App Store Connect davon ab.

## Mit dem bezahlten Konto (Reihenfolge)

1. **Apple Developer Program** aktiv (Einzelperson, 99 USD/Jahr). Team-ID notieren (nicht ins Repository).
2. **App-ID registrieren** (developer.apple.com › Certificates, Identifiers & Profiles › Identifiers):
   Bundle-ID explizit `ch.sesam.remote`, Name «Sesam Remote».
   Capabilities:
   - **App Groups** mit `group.ch.sesam.remote` (nur für das Widget, `docs/plattformen.md`; kann warten)
   - sonst nichts: kein Push, kein iCloud, keine Associated Domains. Face ID, Schlüsselbund, lokales Netzwerk
     und Dateien brauchen keine Capability.
   Fürs Widget später zusätzlich `ch.sesam.remote.widget` mit derselben App Group.
3. **Zertifikat «Apple Distribution»** (Xcode › Settings › Accounts › Manage Certificates › «+», oder
   automatisch beim ersten Archivieren mit `-allowProvisioningUpdates`).
4. **Bereitstellungsprofil «App Store Connect»** für `ch.sesam.remote` (automatische Signierung erledigt das
   beim Export mit `method = app-store-connect`).
5. **App in App Store Connect anlegen**: Plattform iOS, Name «Sesam Remote», Hauptsprache Deutsch,
   Bundle-ID `ch.sesam.remote`, SKU z. B. `sesam-remote-ipad`, voller Zugriff.
   Dann Texte, URLs, Kategorie, Preis (gratis), Altersfreigabe, App-Datenschutz aus diesem Ordner eintragen,
   Screenshots hochladen, Verfügbarkeit (Frankreich vorerst ohne, siehe `export-compliance.md`).
6. **Build bauen und hochladen** (siehe unten), Exportkontrolle beantworten, dann Code in die Info.plist.
7. **TestFlight** intern auf beiden iPads testen (ersetzt die 7-Tage-Installationen des kostenlosen Teams),
   dabei RDP und Hochladen aus «Dateien» prüfen.
8. **Zur Prüfung einreichen** mit den Notizen aus `review-notes.md` (Test-Server vorher einschalten).
9. **Apple Business Manager / Intune:** Nach der Freigabe die App in ABM unter «Apps und Bücher» mit
   Lizenzen holen (gratis), mit dem Intune-Token synchronisieren, dann in Intune als iOS-Store-App
   (VPP, gerätebasierte Lizenz) den Gruppen der alldesk-iPads zuweisen.

## Bauen und hochladen (später)

```bash
# Team-ID nur als Umgebungsvariable, nie ins Repository
export APPLE_DEVELOPMENT_TEAM=<TEAM_ID>
npx tauri ios build --export-method app-store-connect
# Ergebnis: src-tauri/gen/apple/build/arm64/Sesam Remote.ipa
```

Hochladen, eine der Varianten:

```bash
# mit API-Schlüssel aus App Store Connect › Benutzer und Zugriff › Integrationen (Datei unter ~/.appstoreconnect/private_keys/)
xcrun altool --upload-app -f "src-tauri/gen/apple/build/arm64/Sesam Remote.ipa" -t ios --apiKey <KEY_ID> --apiIssuer <ISSUER_ID>
```

oder die App «Transporter» bzw. Xcode › Organizer › Distribute App. Vor jedem Upload `VERSION` erhöhen
(`node scripts/release.mjs`); App Store Connect lehnt eine Build-Nummer ab, die schon da war.

Hinweis aus dem Bauen am 12.11.2026: `tauri ios build` schreibt die Team-ID als `DEVELOPMENT_TEAM` in
`sesam-remote.xcodeproj/project.pbxproj`; diese Änderung nicht committen (`git checkout` der Datei).

## Gefunden beim Vorbereiten (nicht behoben)

- Die Fehlermeldung bei unbekanntem Host enthält englischen Systemtext («failed to lookup address
  information: nodename nor servname provided, or not known»).
- Im Dialog «Verbindung bearbeiten» heisst es auf dem iPad «Bei Doppelklick öffnen» (auf Touch:
  Doppeltippen).
- «Ordner als Laufwerk freigeben» (RDP) erscheint im Dialog auch auf dem iPad, obwohl
  `docs/plattformen.md` die RDP-Ordnerfreigabe dort als nicht möglich führt. Klären, sonst rügt App Review
  eine Funktion, die nicht geht.
- Ohne bestehende Verbindungen gibt es keinen Desktop-Screenshot; dafür braucht es einen Demo-Desktop
  (siehe `review-notes.md`, Test-Server).
