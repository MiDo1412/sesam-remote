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
| Durchreiche (Dateien auf Shell/Remote-Bild ziehen) | ✓ | ✓ | ✓ | … Ziehen aus «Dateien» in Split View noch nicht angebunden |
| Papierkorb auf dem Server, Rückgängig | ✓ ⌘Z | ✓ Strg+Z | ✓ Strg+Z | ✓ (⌘Z mit Tastatur) |
| Mit Administratorrechten (sudo auf dem Server) | ✓ | ✓ | ✓ | ✓ |

## Bedienung

| Funktion | macOS | Windows | Linux | iPad |
|---|---|---|---|---|
| Tastenkürzel an den Remotecomputer | ✓ «Mac-Tastenkürzel» (Bedienungshilfen) | ✓ «Windows-Tastenkombinationen» | … noch nicht | ✗ ⌘Tab, ⌘Leertaste, Globus, ⌘H bleiben bei iPadOS; ↻ Zusatzleiste und «Tasten senden» |
| Bildschirmtastatur mit Zusatzleiste | – (nicht nötig) | ✓ Touch-Geräte | ✓ Touch-Geräte | ✓ |
| Touch im Remote-Bild (Touchpad/Direkt, Zoom) | – | ✓ Touch-Geräte | ✓ Touch-Geräte | ✓ |
| Grössere Touch-Ziele | – | – | – | ✓ (44 pt) |
| Eigene Fenster (Abkoppeln) | ✓ | ✓ | ✓ | ✗ eine App, ein Fenster; ↻ Tabs |
| Einstellungen | ✓ eigenes Fenster (⌘,) | ✓ Dialog | ✓ Dialog | ✓ Blatt |
| Menüleiste | ✓ native | ↻ Menü «☰» in der Titelleiste | ↻ Menü «☰» | ✗ gibt es nicht; ↻ Knöpfe und Kontextmenüs |
| Befehlspalette | ✓ ⌘⇧P | ✓ Strg+Umschalt+P | ✓ | ✓ mit Tastatur |
| Designs (Terminal) | ✓ | ✓ | ✓ | ✓ |

## System

| Funktion | macOS | Windows | Linux | iPad |
|---|---|---|---|---|
| Zugangsdaten | ✓ Schlüsselbund | ✓ Anmeldeinformationsverwaltung | ✓ Secret Service | ✓ Schlüsselbund |
| Passwort anzeigen mit Bestätigung | ✓ Touch ID / Mac-Passwort | … Windows Hello noch nicht | … | … Face ID/Touch ID noch nicht angebunden |
| Verschlüsselte Sicherung (Export/Import) | ✓ | ✓ | ✓ | ✓ (Datei über «Dateien») |
| Gemeinsame Zwischenablage Text | ✓ | ✓ | ✓ | ✓ nur im Vordergrund; Einfügen mit Bestätigung von iOS |
| Zwischenablage Dateien und Bilder | ✓ | ✓ | ✓ | … noch nicht |
| Verknüpfung zu einer Verbindung | ✓ «Verknüpfung auf dem Schreibtisch» | ✓ «Verknüpfung auf dem Desktop» | ✓ | ↻ Kurzbefehle und Widget (in Arbeit, siehe unten) |
| Zuletzt verwendete Verbindungen am App-Symbol | … Dock-Menü | … Sprungliste (Jump List) | – | ↻ Widget (in Arbeit) |
| `sesam://`-Links | ✓ | ✓ | ✓ | ✓ |
| Freigabe «Lokales Netzwerk» | ✓ Systemeinstellungen | – (nicht nötig) | – | ✓ Abfrage beim ersten Verbinden |
| Berechtigungen einrichten (Dialog) | ✓ | – (nicht nötig) | – | – (iPadOS fragt selbst) |
| H.264 im Remote-Bild | ✓ VideoToolbox | ✓ Media Foundation | … | … VideoToolbox gibt es, noch nicht freigeschaltet |
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
- **Kein lokales Terminal, keine Verknüpfungen auf dem Schreibtisch**; ersetzt durch Kurzbefehle und Widget.

## Nächste Schritte

- iPad: Widget mit zuletzt verwendeten Verbindungen. Es braucht eine App Group, die das kostenlose Team nicht
  ausstellen kann; erst mit dem bezahlten Programm sinnvoll.
- macOS: dieselben Kurzbefehle (App Intents ab macOS 13) und ein Dock-Menü mit zuletzt verwendeten
  Verbindungen.
- Windows: Sprungliste am Taskleisten-Symbol, Windows Hello vor dem Anzeigen von Passwörtern.
- Linux: Tastenkürzel an den Remotecomputer.
