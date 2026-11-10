# Versionshinweise

Was sich in Sesam Remote geändert hat, die neueste Version zuerst. Eine Version ist ein Release:
Sie fasst alles zusammen, was seit der letzten dazukam, und heisst nach dem Kalender
`Jahr.Monat.Laufnummer` (26.11.1, 26.11.2, im Dezember 26.12.1). `0.1.0` ist der Stand vor dem
ersten Release.

So ist ein Eintrag gebaut (`docs/versionierung.md`):

- `## <Version> — <Datum>`, darunter **ein Satz**, was die Version bringt.
- Dann die Gruppen `### Neu`, `### Verbessert`, `### Behoben`, `### Entfernt` und `### Betrieb`,
  nur die, die es braucht. Je Zeile `- **Worum es geht:** ein, zwei einfache Sätze.`
- `### Betrieb` nennt die Handgriffe für den Betreiber.

Jede Änderung schreibt ihre Zeilen unter «In Arbeit». `node scripts/release.mjs` macht daraus den
Eintrag der nächsten Nummer; der oberste Eintrag mit Nummer trägt die aus `VERSION`. Veröffentlicht
ist, was auf dem Branch `release` liegt; dorthin bringt es `node scripts/release.mjs publish`.

## In Arbeit

Die erste Version: Bildschirm, Shell und Dateien entfernter Rechner in einer App.

### Neu

- **Alles in einer App:** Bildschirm per RDP und VNC, Shell per SSH, Dateien per SFTP und FTP/FTPS.
- **Bibliothek:** Verbindungen in Gruppen, mit Symbol des Betriebssystems und einem Punkt, der zeigt, ob der Rechner erreichbar ist und wie schnell er antwortet.
- **Eine Anmeldung je Verbindung:** gilt für alle Dienste; eigene Zugangsdaten für RDP oder VNC nur, wo sie nötig sind. Passwörter liegen im Schlüsselbund des Systems.
- **Ubuntu-PC hinzufügen:** Sesam liest die eingerichteten Zugänge und legt die Einträge selbst an; ein Einrichtungs-Script für GNOME Remote Desktop liegt bei.
- **Aufwecken und verbinden:** schaltet einen schlafenden Rechner per Wake-on-LAN ein und verbindet, sobald er da ist.
- **Sicher über SSH verbinden:** RDP und VNC laufen auf Wunsch durch eine SSH-Verbindung.
- **Portweiterleitungen:** je Verbindung, lokal, entfernt oder als SOCKS-Proxy.
- **Ordner vergleichen:** zwei Ordner nebeneinander, mit Status je Datei und Kopieren in Pfeilrichtung.
- **Befehlspalette:** Strg+Umschalt+P findet Verbindungen, Aktionen, Hilfe und zuletzt Kopiertes.
- **Textbausteine:** je Verbindung, auf Wunsch an alle offenen Shells gleichzeitig.
- **Bildschirmtastatur:** mit Esc, Tab, Strg, Alt, Win, Pfeilen und F-Tasten für Touch-Geräte.
- **Hilfe:** Anleitungen je Servertyp, mit Suche.
- **Neu verbinden im Tab:** Nach einem Unterbruch bleibt der Tab mit seinem Inhalt stehen, eine Leiste bietet «Neu verbinden» an (Enter, ⌘R bzw. Strg+R, Tab-Menü). Shell und VNC versuchen es nach einem Abbruch zuerst selbst (nach 2, 5 und 15 s), RDP wie bisher im Hintergrund.
- **Derselbe Rechner mit einem Klick:** Im Sitzungskopf und in der Befehlspalette geht es von der Shell zum Desktop und zu den Dateien desselben Rechners und umgekehrt; fehlt der Dienst in der Verbindung, nimmt Sesam einen Eintrag der Bibliothek mit derselben Adresse.
- **Dateien mit Administratorrechten:** Fehlt das Schreibrecht (z. B. Hochladen nach /opt), bietet die Übertragungsliste «Mit Administratorrechten wiederholen» an: Sesam lädt in einen temporären Ordner und legt die Datei mit sudo an ihren Platz; eine bestehende ersetzt es erst nach Rückfrage und behält dabei deren Besitzer und Rechte. Dasselbe gilt fürs Herunterladen, Löschen, Umbenennen und Anlegen. Fehlermeldungen sind in der Liste ganz lesbar.
- **Berechtigungen-Dialog:** Lesen/Schreiben/Ausführen für Besitzer, Gruppe und Andere, als Zahl, Besitzer:Gruppe und auf Wunsch rekursiv; wo nötig nach Rückfrage mit sudo. Das sudo-Passwort fragt Sesam höchstens einmal ab, gibt es nur über die Standardeingabe weiter und speichert es nicht.
- **Berechtigungen einrichten …** (App-Menü und Befehlspalette): zeigt auf dem Mac Lokales Netzwerk, Gerätesteuerung und Datenzugriff sowie Eingabeüberwachung mit Stand; je Zeile anfragen oder die richtige Stelle in den Systemeinstellungen öffnen, oder alles auf einmal. Windows und Linux brauchen keine Freigaben.
- **Zugangsdaten verwalten:** eigener Eintrag «Zugangsdaten …» im App-Menü und in der Befehlspalette; je Eintrag sichtbar, welche Verbindungen ihn nutzen. Neu: SSH-Schlüssel (Ed25519) erzeugen, öffentlichen Schlüssel kopieren und wie ssh-copy-id auf einem Server eintragen, duplizieren, und auf dem Mac das gespeicherte Passwort nach Touch ID anzeigen und kopieren. Geheimnisse bleiben im Schlüsselbund.

### Verbessert

- **Tastentest:** Befehlspalette › «Diagnose: Tastentest» drückt Win, Win+R und Alt+Tab und zeigt, ob sie drüben ankamen und lokal nichts aufging.
- **Windows-Tasten am richtigen Ort:** Win, Alt+Tab, Alt+F4 und Druck gehen an den Remotecomputer, solange das Remote-Bild aktiv ist, im Fenster wie im Vollbild. Strg+Alt+Ende sendet Strg+Alt+Entf.
- **Scharfes Bild:** auch auf hochauflösenden Bildschirmen; das Remote-Bild passt sich der Fenstergrösse an.
- **Bleibt verbunden:** RDP verbindet nach einem kurzen Netzabbruch selbst wieder.
- **Klare Hinweise:** lehnt ein Rechner die Verbindung ab, sagt Sesam warum und was zu tun ist, passend zum Servertyp.
- **Ruhige Oberfläche:** eigene Titelleiste unter Windows, kurzes App-Menü (Symbol mit drei Linien oder Alt), Symbole im Stil des Systems.
- **Symbole wie in macOS:** Auf dem Mac zeigt Sesam die Symbole von Apple (SF Symbols), gezeichnet vom System selbst; unter Windows und Linux bleiben die Fluent-Symbole.
- **Befehlspalette im Mac-Menü:** Darstellung › «Befehlspalette …»; das Kürzel ⌘⇧P bleibt beim Remote-Bild, solange es die Tastatur hat.
- **Terminal im Dunkelmodus auf dem Mac:** Das Design «macOS Terminal Standard» folgt jetzt wie in Terminal.app dem Erscheinungsbild; vorher blieb die Fläche im Dunkelmodus weiss.
- **⌘W auf dem Mac:** schliesst nur die aktive Sitzung (in der Bibliothek das Fenster) statt des ganzen Fensters mit allen Sitzungen; hat das Remote-Bild die Tastatur, geht ⌘W an den Remotecomputer.
- **Mac-Tastenkürzel am Remotecomputer:** ⌘Tab, ⌘`, ⌘Leertaste, ⌃Leertaste, ⌃Pfeiltasten und ⌘⇧3/4/5 gehen an den Remotecomputer, solange das Remote-Bild aktiv ist, statt den App-Umschalter, Spotlight oder Spaces zu öffnen. Beim ersten Mal fragt macOS nach der Freigabe; einstellbar unter Einstellungen › Allgemein.
- **RDP-Zwischenablage auf dem Mac:** Kopierter Text geht in beide Richtungen zwischen Mac und Remotecomputer (Option «Gemeinsame Zwischenablage» der Verbindung); Bilder und Dateien vorerst nur unter Windows.
- **Flüssiges RDP-Bild auf dem Mac:** H.264 (AVC420/AVC444) dekodiert jetzt auch auf dem Mac, mit VideoToolbox von macOS; der Server schickt damit wie unter Windows Video statt Einzelbilder.
- **Fenster überall ziehen:** an allen freien Stellen der Titel-, Tab- und Sitzungsleiste; Doppelklick zoomt bzw. maximiert.
- **Tabs:** Minimieren und Schliessen sitzen am rechten Rand des Tabs, auch bei kurzen Namen.
- **Mac: Schliessen wie bei Terminal:** Das rote Schliessen (und ⌘W in der Bibliothek) blendet nur das Fenster aus, Sesam bleibt im Dock; ein Klick aufs Dock-Symbol holt es zurück. Offene Sitzungen werden erst nach Rückfrage getrennt, ⌘Q beendet.
- **VNC mit Tight:** Sesam bevorzugt die Kodierung Tight (Füllfarbe, Palette, Gradient, zlib) und auf dem Mac auch Tight-JPEG (Qualität 8, dekodiert vom System): weniger Daten übers Netz, bei Fotos und Videos deutlich weniger. Messung mit einem Testserver: ZRLE 52 Bilder/s bei 0,9 MB/s, Tight 52 Bilder/s bei 0,4 MB/s.

### Behoben

- **Mac: Verbindungen ins eigene Netz:** «No route to host» zu Rechnern im lokalen Netz lag an der macOS-Freigabe «Lokales Netzwerk». Sesam Remote fragt jetzt mit eigenem Text danach, behält die Freigabe über Updates hinweg und erklärt bei einer Sperre, wo man sie einschaltet.
- **Flüssiges Bild bei VNC und RDP:** Bildschirm-, Shell- und Dateidaten gingen wegen einer fehlenden Freigabe in der Sicherheitsrichtlinie über einen langsamen Ersatzweg, der jedes Byte als Text verpackte; dazu kam jedes geänderte Rechteck als eigene Nachricht. Jetzt geht alles binär, ein Bild pro Nachricht, und Sesam schickt erst weiter, wenn die Anzeige mitkommt: kein wachsender Rückstand mehr, die Anzeige zählt echte Bilder pro Sekunde.
- **Auflösung bei VNC:** Lehnt der Server eine andere Grösse ab, sagt Sesam das einmal und skaliert, statt bei jeder Fenstergrösse «unbekannter Fehler» zu melden; «weitergeleitet» gilt nicht mehr als Fehler.
- **Verknüpfungen auf dem Mac-Schreibtisch mit Sesam-Symbol:** statt des allgemeinen Internet-Symbols. Bestehende Verknüpfungen einmal neu anlegen.
- **Mac: Absturz beim Abgleich der Zwischenablage:** gleichzeitige Zugriffe auf die Zwischenablage (VNC-Abgleich, RDP) laufen jetzt nacheinander.

### Betrieb

- **Versionen:** die Nummer steht nur in `VERSION`; ein Release setzt sie in App, Installer und Paketdateien. Pro Klon einmal `git config core.hooksPath .githooks` ausführen.
- **Git-Hooks auf Mac und Linux:** Die Hooks in `.githooks` sind jetzt ausführbar; vorher übersprang Git sie dort stillschweigend.

## 0.1.0 — 2026-11-06

Projekt angelegt.
