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

### Betrieb

- **Versionen:** die Nummer steht nur in `VERSION`; ein Release setzt sie in App, Installer und Paketdateien. Pro Klon einmal `git config core.hooksPath .githooks` ausführen.
- **Git-Hooks auf Mac und Linux:** Die Hooks in `.githooks` sind jetzt ausführbar; vorher übersprang Git sie dort stillschweigend.

## 0.1.0 — 2026-11-06

Projekt angelegt.
