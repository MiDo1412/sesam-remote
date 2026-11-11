# Plattformen: welche Funktion wo

Grundsatz (Mike, 11.11.2026): **alle Funktionen überall, wo es geht**, mit Namen, wie sie die jeweilige
Plattform nennt. Wo etwas nicht geht, steht hier der Grund und der Ersatz. Neue Funktionen tragen sich hier
ein.

Legende: **✓** vorhanden · **↻** ersetzt durch … · **✗** nicht möglich, weil … · **…** noch nicht gebaut.
Windows und Linux: Stand aus dem Code; auf echten Geräten zuletzt nicht geprüft (WS-MD01 nicht erreichbar).

## Vertrieb

| | macOS | Windows | Linux | iPad |
|---|---|---|---|---|
| Wege | Download von der Website, signiert (Developer ID) und notarisiert; **kein** Mac App Store (Entscheid 11.11.2026) | Website und/oder Microsoft Store | Download (Pakete des Tauri-Bundlers: `.deb`, `.rpm`, AppImage) | App Store |
| Details | `docs/app-store.md` | `docs/app-store.md` | – | `docs/app-store.md` |

## Verbinden

| Funktion | macOS | Windows | Linux | iPad |
|---|---|---|---|---|
| Bibliothek, Gruppen, Suche, Karten/Liste | ✓ | ✓ | ✓ | ✓ |
| Kontextmenü (Neue Verbindung, Bearbeiten …) | ✓ Rechtsklick | ✓ Rechtsklick | ✓ Rechtsklick | ↻ lange drücken |
| Schnelle Verbindung | ✓ | ✓ | ✓ | ✓ |
| Remotedesktop RDP | ✓ | ✓ | ✓ | ✓ (auf dem Gerät noch ungeprüft) |
| VNC | ✓ | ✓ | ✓ | ✓ |
| SSH-Shell | ✓ | ✓ | ✓ | ✓ |
| Dateien SFTP/FTP | ✓ | ✓ | ✓ | ✓ |
| Lokale Seite im Dateifenster | ✓ ganzer Rechner | ✓ ganzer Rechner | ✓ ganzer Rechner | ↻ «Auf meinem iPad › Sesam Remote» in der App «Dateien»; Hochladen über die Dateiauswahl |
| Lokales Terminal | ✓ Terminal | ✓ Windows-Terminal-Profile | ✓ | ✗ iPadOS startet keine fremden Programme |
| Online-Status, Aufwecken (Wake-on-LAN) | ✓ | ✓ | ✓ | ✓ |
| Erste Hilfe | ✓ | ✓ | ✓ | ✓ |
| Desktop freischalten | ✓ | ✓ | ✓ | ✓ |
| Türöffner (lokale Dienste über SSH) | ✓ | ✓ | ✓ | ✓ |
| Gehaltene Shell (tmux, «Nahtlos») | ✓ | ✓ | ✓ | ✓ wichtig, weil iPadOS Verbindungen im Hintergrund trennt |
| Durchreiche (Dateien auf Shell/Remote-Bild ziehen) | ✓ | ✓ | ✓ | ✓ aus «Dateien» (Split View, Slide Over) ziehen; nur Dateien, keine Ordner |
| Mehrfachauswahl, Tastatur, Gummiband | ✓ wie Finder (Enter benennt um, ⌘↓ öffnet) | ✓ wie Explorer (Enter öffnet, F2) | ✓ wie Explorer | ✓ Tippen, «Alle auswählen» im Menü, mit Tastatur wie am Mac; … Auswahlmodus mit Häkchen noch nicht |
| Rückfrage vor dem Überschreiben, Ordner zusammenführen | ✓ | ✓ | ✓ | ✓ |
| Papierkorb auf dem Server, Rückgängig | ✓ ⌘Z | ✓ Strg+Z | ✓ Strg+Z | ✓ (⌘Z mit Tastatur) |
| Mit Administratorrechten (sudo auf dem Server) | ✓ | ✓ | ✓ | ✓ |

## Bedienung

| Funktion | macOS | Windows | Linux | iPad |
|---|---|---|---|---|
| Tastenkürzel an den Remotecomputer | ✓ «Mac-Tastenkürzel» (Bedienungshilfen) | ✓ «Windows-Tastenkombinationen» | … noch nicht | ✗ ⌘Tab, ⌘Leertaste, Globus, ⌘H bleiben bei iPadOS; ↻ Zusatzleiste und «Tasten senden» |
| Bildschirmtastatur mit Zusatzleiste | – (nicht nötig) | ✓ Touch-Geräte | ✓ Touch-Geräte | ✓ |
| Touch im Remote-Bild (Touchpad/Direkt, Zoom) | – | ✓ Touch-Geräte | ✓ Touch-Geräte | ✓ |
| Grössere Touch-Ziele | – | – | – | ✓ (44 pt) |
| Vollbild | ✓ F11 | ✓ F11 | ✓ F11 | ✓ ohne Leisten, Statusleiste und Home-Leiste; Griff oben in der Mitte holt die Verbindungsleiste |
| Seitenleiste und Details ein-/ausblenden | ✓ | ✓ | ✓ | ✓ Details als Blatt, Seitenleiste im Hochformat eingeklappt |
| Eigene Fenster (Abkoppeln) | ✓ | ✓ | ✓ | ✗ eine App, ein Fenster; ↻ Tabs |
| Einstellungen | ✓ eigenes Fenster (⌘,) | ✓ Dialog | ✓ Dialog | ✓ Blatt |
| Menüleiste | ✓ native | ↻ Menü «☰» in der Titelleiste | ↻ Menü «☰» | ✗ gibt es nicht; ↻ Knöpfe und Kontextmenüs |
| Befehlspalette | ✓ ⌘⇧P | ✓ Strg+Umschalt+P | ✓ | ✓ mit Tastatur |
| Designs (Terminal) | ✓ | ✓ | ✓ | ✓ |

## System

| Funktion | macOS | Windows | Linux | iPad |
|---|---|---|---|---|
| Zugangsdaten | ✓ Schlüsselbund | ✓ Anmeldeinformationsverwaltung | ✓ Secret Service | ✓ Schlüsselbund |
| Passwort anzeigen mit Bestätigung | ✓ Touch ID / Mac-Passwort | … Windows Hello noch nicht | … | ✓ Face ID / Touch ID / Gerätecode |
| Verschlüsselte Sicherung (Export/Import) | ✓ | ✓ | ✓ | ✓ (Datei über «Dateien») |
| Gemeinsame Zwischenablage Text | ✓ | ✓ | ✓ | ✓ nur im Vordergrund; Einfügen mit Bestätigung von iOS |
| Zwischenablage Dateien und Bilder | ✓ | ✓ | ✓ | … noch nicht |
| Kurzbefehl «Verbindung öffnen» | ↻ Kurzbefehl mit der Aktion «URL öffnen» und `sesam://open?id=…` (eigene Aktion siehe unten) | – | – | ✓ |
| Verknüpfung zu einer Verbindung | ✓ «Verknüpfung auf dem Schreibtisch» | ✓ «Verknüpfung auf dem Desktop» | ✓ | ↻ Kurzbefehl «Verbindung öffnen» (App «Kurzbefehle», Spotlight, Siri, Home-Bildschirm) |
| Zuletzt verwendete Verbindungen am App-Symbol | ✓ Dock-Menü (Rechtsklick aufs Symbol im Dock) | ✓ Sprungliste (Rechtsklick aufs Symbol in der Taskleiste; auf Windows noch ungeprüft) | – (je nach Desktop; nicht gebaut) | … Widget «Zuletzt verwendet»: Code bereit, braucht die App Group (siehe unten) |
| `sesam://`-Links | ✓ | ✓ | ✓ | ✓ |
| Freigabe «Lokales Netzwerk» | ✓ Systemeinstellungen | – (nicht nötig) | – | ✓ Abfrage beim ersten Verbinden |
| Berechtigungen einrichten (Dialog) | ✓ | – (nicht nötig) | – | – (iPadOS fragt selbst) |
| H.264 im Remote-Bild | ✓ VideoToolbox | ✓ Media Foundation | … | ✓ VideoToolbox (wie am Mac; mit echtem RDP-Server auf dem iPad noch ungeprüft) |
| RDP-Ordnerfreigabe | ✓ | ✓ | ✓ | ✗ kein freier Dateizugriff |
| RDP-Ton | ✓ | ✓ | ✓ | ✓ |

## iPad: Grenzen von iPadOS

- **Hintergrund:** Wechselt man die App, trennt iPadOS nach etwa 30 Sekunden alle Verbindungen. Beim
  Zurückkommen verbindet Sesam neu; eine gehaltene Shell (tmux, «Nahtlos») läuft auf dem Server weiter.
- **Systemkürzel:** ⌘Tab, ⌘Leertaste, Globus-Taste und ⌘H bleiben bei iPadOS und kommen nicht beim
  Remotecomputer an; dafür gibt es die Zusatzleiste und «Tasten senden».
- **Dateien:** nur über die App «Dateien» bzw. die Dateiauswahl; Heruntergeladenes liegt dort unter
  «Auf meinem iPad › Sesam Remote».
- **Zwischenablage:** nur solange Sesam vorne ist; iOS kann beim Einfügen um Bestätigung bitten.
- **Kein lokales Terminal, keine Verknüpfungen auf dem Schreibtisch**; ersetzt durch den Kurzbefehl «Verbindung öffnen».

## Widget einschalten (iPad, bezahltes Entwicklerprogramm)

Der Code liegt bereit: `src-tauri/gen/apple/SesamWidget/SesamWidget.swift` (Widget «Zuletzt verwendet», klein,
mittel, gross; ein Tipp öffnet `sesam://open?id=…`). Die App schreibt die Liste selbst nach `widget.json` im
Ordner der App Group (`src-tauri/src/jumps.rs`); ohne App Group tut sie nichts. Das kostenlose Team kann keine
App Group ausstellen, darum ist das Widget noch in keinem Ziel und die App baut wie bisher.

Mit dem bezahlten Team:
1. Auf developer.apple.com die App Group `group.ch.sesam.remote` anlegen und der App-ID `ch.sesam.remote` geben.
2. In `sesam-remote_iOS/sesam-remote_iOS.entitlements` den Schlüssel `com.apple.security.application-groups`
   mit `group.ch.sesam.remote` eintragen.
3. In `project.yml` ein Ziel `SesamWidget` (Typ `app-extension`, Plattform iOS, ab iOS 16, Quelle
   `SesamWidget`, Bundle-ID `ch.sesam.remote.widget`, `NSExtensionPointIdentifier` =
   `com.apple.widgetkit-extension`, eigene Entitlements mit derselben App Group) anlegen, im App-Ziel unter
   `dependencies` mit `embed: true` eintragen, dann `xcodegen generate` (ohne Team-ID im Repository).

## Nächste Schritte

- iPad: Widget einschalten, sobald das bezahlte Programm da ist (siehe oben).
- macOS: eigene Kurzbefehl-Aktion «Verbindung öffnen». App Intents verlangen Swift-Code und Metadaten, die
  Xcode beim Bauen erzeugt; Tauri baut die Mac-App ohne Xcode-Projekt. Machbar über ein kleines Swift-Paket
  plus den Schritt `appintentsmetadataprocessor` im Build. Bis dahin: Kurzbefehl mit «URL öffnen».
- Windows: Sprungliste auf einem echten Windows-Rechner prüfen, Windows Hello vor dem Anzeigen von Passwörtern.
- Linux: Tastenkürzel an den Remotecomputer.
- Dateifenster: Vorschau mit Leertaste, Datei auf dem Server im lokalen Editor bearbeiten und beim Sichern
  hochladen, zuletzt besuchte Ordner je Verbindung, Suche in Unterordnern, Spaltenbreite ändern, Ordnergrösse
  auf Wunsch, Übertragungsliste mit Pause und Zusammenfassung, Auswahlmodus mit Häkchen auf dem iPad.
