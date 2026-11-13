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

### Neu

- **Hilfe, die weiss, wo du bist:** «Hilfe zu dieser Ansicht» (Mac: Menü «Hilfe», Windows: ☰ › Hilfe, überall in der Befehlspalette) öffnet die Anleitung zur offenen Sitzung: RDP zu Windows, GNOME oder xrdp je nach System, VNC zum Mac, SSH oder FTP. Das Hilfe-Fenster und das Mac-Menü sind in «Zielrechner einrichten», «Verbinden» und «Sesam kennenlernen» gegliedert; die passende Anleitung trägt den Namen der Verbindung.
- **Links an die richtige Stelle:** Statt überall auf die Startseite zu zeigen, führen die Links jetzt dorthin, wo es weitergeht: «Was ist neu in dieser Version» zu den Versionshinweisen genau dieser Version, «Sesam auf deinen anderen Geräten» zum Download, «Fehler melden oder Idee teilen» zu GitHub. Im Hilfe-Fenster: «Häufige Fragen» und «Anleitung verbessern» mit dem Titel der offenen Anleitung.
- **Touch-Bedienung je Verbindung:** Im Editor unter «Desktop» wählst du für Geräte mit Touch, ob du den Zeiger wie auf einem Touchpad schiebst oder direkt tippst; die Sitzung startet so. Im Vollbild schaltest du über die Verbindungsleiste oben um (Handsymbol).

### Behoben

- **Windows-PC per RDP vom iPad:** Die Verbindung brach mit «the platform certificate store contains no usable roots» ab, bevor Sesam nach dem Zertifikat fragen konnte. Jetzt prüft Sesam das Zertifikat selbst: bekannt, vom System vertraut (Schlüsselbund, Zertifikatspeicher) oder es fragt beim ersten Mal nach, wie bei selbst signierten Windows-Zertifikaten üblich. Gilt auf allen Systemen.
- **«Anfragen» bei den Berechtigungen (Mac):** macOS zeigt seinen Dialog nur einmal; danach tat der Knopf nichts. Bleibt die Freigabe aus, öffnet Sesam jetzt gleich die richtige Stelle in den Systemeinstellungen, auch bei «Alle einrichten».

### Verbessert

- **Feinschliff:** Leere Bibliothek mit einer Erklärung und nur einem Hauptknopf statt zwei gleichen. Die «Zuletzt»-Einträge der schnellen Verbindung sind rund wie alle anderen. Die Fragezeichen bei den Diensten sehen aus wie die bei den Feldern; das beim Remotedesktop öffnet die Anleitung zum gewählten Betriebssystem.
- **Protokoll der letzten Läufe bleibt:** Beim Start legt Sesam `sesam.log` nicht mehr über das alte; die zwei vorigen Läufe bleiben als `sesam.1.log` und `sesam.2.log` (je höchstens 5 MB, bei mehr das Ende). So lässt sich z. B. nachlesen, was ein Update vor dem Neustart heruntergeladen und installiert hat.

### Betrieb

- **RDP-Zertifikat:** Handshake mit `DangerouslyAcceptInvalidCertificate`, danach Prüfung vor CredSSP: Pin, sonst `rustls-platform-verifier` (Security.framework, CryptoAPI), sonst `CERT_UNKNOWN`. Neue direkte Abhängigkeit `rustls-platform-verifier` (war über den Updater schon dabei). RD-Gateway und KDC-Proxy prüfen weiter mit `rustls-native-certs` und gehen auf dem iPad deshalb noch nicht.

## 26.11.3 — 2026-11-12

### Neu

- **Neue Shell-Designs:** Vier eigene Designs von Sesam, je hell und dunkel und als «automatisch» passend zum Erscheinungsbild: Tiefsee, Graphit, Wald und Abend. Der Cursor trägt die Sesam-Farbe; Text und alle Farben erfüllen den WCAG-Kontrast (Text mindestens 7:1, Farben 4,5:1). Dazu Rosé Pine und Kanagawa (je hell und dunkel). Auswahl mit Vorschau unter Einstellungen › Shell und je Profil unter Einstellungen › Terminal.
- **Website im Programm:** sesam-remote.app öffnet sich aus «Über Sesam», aus dem Hilfe-Fenster, aus der Befehlspalette und auf dem Mac aus dem Menü «Hilfe» › «Sesam Remote-Website».

### Behoben

- **PowerShell und weitere Shells werden erkannt:** Auf dem Mac fehlte PowerShell bei den Terminal-Typen, weil eine aus dem Finder gestartete App die Programmordner der Login-Shell (Homebrew, `/usr/local/microsoft/powershell/7`) nicht kennt. Sesam sucht jetzt zusätzlich an den üblichen Installationsorten und in `/etc/paths.d`, unter Linux auch Snap, Linuxbrew, Nix und `~/.local/bin`, unter Windows WinGet und Scoop. Neu erkannt werden auch Nushell, xonsh und Elvish.

### Verbessert

- **Updates und Downloads direkt von GitHub:** Die App holt Updates zuerst aus dem GitHub-Release, die Website ist nur noch Rückfall. Die Download-Knöpfe auf sesam-remote.app laden die Datei direkt (fester Name, immer die neueste Version), mit portabler Fassung daneben; die Website wählt das System des Besuchers vor. Die Website sagt jetzt richtig, dass die Mac-App noch nicht notarisiert ist (Rechtsklick › Öffnen), und bietet Windows (MSI, portabel) und Linux (AppImage, .deb, portabel) mit festen Namen an.

## 26.11.2 — 2026-11-11

### Neu

- **Automatische Updates:** Sesam sucht im Hintergrund nach neuen Versionen, lädt sie still herunter, prüft die Signatur und installiert beim Beenden; der nächste Start ist neu. Unter Einstellungen › Über Sesam stehen der Stand, «Automatisch aktualisieren» (an) und «Jetzt suchen». Windows mit MSI im Programme-Ordner: nur Hinweis mit «Jetzt installieren»; Firmen schalten es mit `AUTOUPDATE=0` bzw. einer Richtlinie aus (`docs/intune.md`). Portable Fassungen aktualisieren sich an Ort und Stelle. Auf dem iPad aktualisiert der App Store.

### Betrieb

- **Neue Git-Historie:** `main` und `website` wurden einmal umgeschrieben (nur Autor-Adressen, Inhalte gleich). Bestehende Klone einmal mit `git fetch origin && git reset --hard origin/main` neu aufsetzen (`docs/build-windows.md`).
- **Update-Schlüssel:** Einmal `scripts/updater-key.sh` ausführen und den Schlüssel sichern; Bauen, Signieren und Hochladen der Updates nach `https://sesam-remote.app/updates/` stehen in `docs/release.md`.

## 26.11.1 — 2026-11-11

Die erste Version: Bildschirm, Shell und Dateien entfernter Rechner in einer App.

### Neu

- **Alles in einer App:** Bildschirm per RDP und VNC, Shell per SSH, Dateien per SFTP und FTP/FTPS.
- **Bibliothek:** Verbindungen in Gruppen, mit Symbol des Betriebssystems und einem Punkt, der zeigt, ob der Rechner erreichbar ist und wie schnell er antwortet.
- **Eine Anmeldung je Verbindung:** gilt für alle Dienste; eigene Zugangsdaten für RDP oder VNC nur, wo sie nötig sind. Passwörter liegen im Schlüsselbund des Systems.
- **Ubuntu mit GNOME:** «Desktop freischalten …» richtet GNOME Remote Desktop über das beiliegende Einrichtungs-Script ein; feste Ports pro Benutzer liest der Verbindungsdialog per SSH aus.
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
- **Sicherung:** Bibliothek und Zugangsdaten samt Passwörtern in eine mit eigenem Passwort verschlüsselte Datei (.sesam, AES-256-GCM, Argon2id); beim Einlesen zeigt Sesam zuerst, was dazukommt, und führt zusammen, ohne Bestehendes zu ändern.
- **Zugangsdaten:** Suche, «zuletzt verwendet», Hinweis bei unbenutzten Einträgen und «Zugang testen» (Anmeldung über SSH an einer Verbindung, die sie nutzt).
- **Raspberry Pi erkennen:** Nach dem Verbinden der Shell liest Sesam Modell und System aus und setzt das Symbol in der Bibliothek (Raspberry Pi bzw. Ubuntu), wenn noch keins gewählt ist. Auf Raspberry Pi OS zeigt der Sitzungskopf Modell, Temperatur und eine Warnung bei Unterspannung, jede Minute aktualisiert.
- **Ein Gerät, drei Türen:** Karten zeigen Desktop, Shell und Dateien als Knöpfe und Benutzer@Adresse; bei einer neuen Verbindung erkennt Sesam die erreichbaren Dienste selbst; in der Sitzung lässt sich ein fehlender Dienst einrichten; zwei Einträge desselben Geräts lassen sich zusammenführen (mit Sicherung der Bibliothek).
- **Mehr und schönere Terminal-Designs:** Raster mit echter Vorschau (Prompt, farbige Ausgabe, Fehler, Auswahl, blinkender Cursor, alle 16 Farben), Suche und Filter hell/dunkel. Neu: One Light, Gruvbox Hell, Tokyo Night (Day), Catppuccin Latte/Mocha, GitHub hell/dunkel und «automatische» Paare, die dem Erscheinungsbild folgen. Eigene Designs: duplizieren und mit Farbwählern anpassen (wirkt sofort auf offene Shells), .itermcolors und Windows-Terminal-Schemata importieren, als Windows-Terminal-Schema exportieren.
- **Nahtlose Shell:** Auf Wunsch je Verbindung läuft die Shell in einer tmux-Sitzung auf dem Server und übersteht Unterbruch, Ruhezustand und Netzwechsel; Sesam hängt sie beim Neuverbinden wieder an. tmux ohne Statuszeile und eigene Kürzel; gehaltene und verwaiste Sitzungen lassen sich auflisten und beenden. Nach dem ersten Abbruch fragt Sesam einmal nach.
- **Durchreiche:** Dateien auf eine SSH-Shell ziehen lädt sie in deren Ordner und setzt die Pfade in die Zeile; auf das Remote-Bild gezogen landen sie über SSH desselben Geräts in dessen Downloads; das Dateifenster kann dem Ordner der Shell folgen.
- **Türöffner:** Zeigt die Shell eine Adresse auf dem Server selbst (z. B. `http://localhost:3000` oder `0.0.0.0:8080`), öffnet ⌘-Klick bzw. Strg+Klick sie über die SSH-Verbindung im Browser dieses Computers. Offene Türen stehen unter dem Weltkugel-Knopf im Sitzungskopf und lassen sich dort schliessen.
- **Erste Hilfe:** Klappt eine Verbindung nicht, prüft Sesam der Reihe nach Name, Erreichbarkeit, Port, Antwort des Dienstes, Anmeldung und Zertifikat und zeigt, wo es hängt. Passend dazu bietet es Aufwecken, die Freigabe «Lokales Netzwerk», den Weg über einen SSH-Tunnel oder die Zugangsdaten an. Aufrufen lässt es sich im Kontextmenü der Karte und in der Leiste «Verbindung getrennt».
- **Desktop freischalten:** Hat ein Gerät nur SSH, bietet die Karte «Desktop freischalten …» an. Sesam erkennt per SSH das System (Ubuntu mit GNOME, Raspberry Pi OS, macOS), zeigt genau die Befehle und führt sie erst nach Klick mit sudo aus; unter Ubuntu startet das Setup-Script in einer Shell. Danach steht der Desktop in der Verbindung.
- **Rückgängig auf dem Server:** Gelöschtes und beim Hochladen Ersetztes kommt zuerst in einen Papierkorb im Home auf dem Server (7 Tage, einstellbar, räumt selbst auf); «Rückgängig» in der Übertragungsliste und ⌘Z bzw. Strg+Z holen es zurück. Geht der Papierkorb nicht, fragt Sesam, statt still endgültig zu löschen. Auch Dateien mit Administratorrechten (z. B. in /etc) werden vor dem Ersetzen gesichert und mit dem alten Besitzer und den alten Rechten zurückgeholt; Grosses (über 500 MB, einstellbar) sichert Sesam nicht still, sondern fragt.
- **Portabel:** Liegt neben der App eine Datei `portable` oder ein Ordner `Sesam-Daten`, legt Sesam Bibliothek, Gruppen, gemerkte Zertifikate und Protokolle dort ab (unter Windows und Linux auch die Einstellungen der Oberfläche). Zugangsdaten bleiben im Schlüsselbund des Systems. Fertige portable Fassungen: ZIP für Windows und macOS, AppImage für Linux.
- **Windows-Installer für Firmen:** Das MSI installiert für alle Benutzer, legt eine Verknüpfung auf dem Desktop und im Startmenü an und lässt sich still installieren und entfernen (`msiexec /i … /qn`, `/x … /qn`), auch über Intune; eine neue Version ersetzt die alte. Anleitung: `docs/intune.md`.

### Verbessert

- **Neuer Anstrich «Inhalt und Glas»:** Inhalte (Karten, Listen, Terminal, Remote-Bild) bleiben ruhig und deckend; Seitenleiste, Leisten, Tabs, Menüs und Dialoge liegen als Glas darüber. Auf dem Mac scheint das Fenstermaterial des Systems durch (passt zu Liquid Glass), unter Windows 11 Mica, sonst deckend. Gerundete, aufeinander abgestimmte Ecken, Knöpfe in Leisten als Kapseln, weniger Rahmen. «Transparenz reduzieren», «Kontrast erhöhen» und «Bewegung reduzieren» des Systems werden beachtet. Konzept: `docs/design-konzept.md`.
- **Ein Knopfsystem, ein Blatt:** Alle Knöpfe folgen denselben Regeln: Standard, Hauptaktion, Text, Löschen (rot), Symbol. Es gibt drei feste Höhen, auf dem Mac und iPad als Kapseln, unter Windows mit Fluent-Ecken. Alle Dialoge sind dasselbe Blatt mit dem Titel im Blatt und der Knopfleiste unten, die Hauptaktion steht rechts. Ohne Nähte zwischen Kopf, Inhalt und Fuss. Zerstörende Rückfragen («Löschen», «Ersetzen») zeigen den Knopf rot. Der Fokusring erscheint nur noch nach Bedienung mit der Tastatur, nicht mehr nach einem Klick oder beim Öffnen eines Blatts. Kontextmenüs öffnen am Mauszeiger. Gruppen in Formularen und Listen in den Einstellungen sind eine Fläche mit Trennlinien statt Kästen in Kästen. Die Dateivorschau ist ein normales Blatt. Auf dem iPad schwebt die Inhaltsfläche auch bei eingeklappter Seitenleiste abgerundet über dem Hintergrund. Die Bibliothek zeigt «Keine Verbindungen gefunden», wenn die Suche nichts findet. Der aktive Tab bleibt auch bei vielen Tabs ganz sichtbar.
- **FTP: sicher ersetzen:** «Ersetzen» sichert die alte Fassung zuerst auf diesem Computer (nicht auf dem Server, wo sie bei vielen Webhostern über das Web abrufbar wäre), lädt die neue unter einem Zwischennamen hoch, prüft die Grösse und setzt sie erst dann an ihren Platz. Scheitert etwas, bleibt die alte Datei stehen; gelöscht wird sie nie vorher. «Rückgängig» lädt die gesicherte Fassung wieder hoch. Sicherungen werden wie der Papierkorb nach den eingestellten Tagen aufgeräumt; grössere Dateien als die Grenze ersetzt Sesam nur nach Rückfrage ohne Sicherung. Herunterladen über FTP kann jetzt ebenfalls ersetzen.
- **iPad: eigene Ordner im Dateifenster:** «Ordner öffnen …» (Haus-Knopf links oder in der leeren Liste) wählt einen Ordner aus der App «Dateien», etwa in iCloud Drive, «Auf meinem iPad», auf einem USB-Stick oder einer Freigabe. Sesam merkt ihn sich als Ort und zeigt ihn links mit Unterordnern; Hoch- und Herunterladen gehen direkt dorthin. «Dateien wählen …» holt einzelne Dateien in den offenen Ordner. Orte lassen sich wieder entfernen.
- **Nach einem Verbindungsabbruch weitermachen:** Bricht die Verbindung mitten in einer Datei ab (SFTP), bleibt der fertige Teil liegen; «Wiederholen» macht dort weiter, sofern sich die Quelle nicht geändert hat (Grösse und Änderungszeit), sonst überträgt Sesam die Datei von vorn.
- **Übertragungen pausieren:** «Pause» hält eine Übertragung an ihrer Stelle an, «Weiter» setzt sie dort fort (SFTP, SCP, FTP; bei FTP kann ein Server eine sehr lange Pause beenden).
- **Klarere Fehler bei der Anmeldung:** Sesam sagt, welche Verfahren versucht wurden (Agent, Schlüssel, Passwort), und meldet, wenn ein gespeichertes Passwort oder ein gespeicherter Schlüssel im Schlüsselbund dieses Geräts fehlt.
- **Dateifenster, mehr wie Finder und Explorer:** Leertaste zeigt eine Vorschau (Text, Bilder, PDF). «Bearbeiten (Standardprogramm)» öffnet eine Datei vom Server im passenden Programm; jedes Sichern lädt sie wieder hoch, mit Rückfrage, falls sie sich auf dem Server inzwischen geändert hat. «Unterordner» beim Filter sucht auch in Unterordnern. Spaltenbreite lässt sich ziehen, «Grösse berechnen» zeigt die Grösse eines Ordners. Fehlgeschlagene Übertragungen lassen sich wiederholen; ist alles fertig, fasst eine Zeile zusammen («4 von 4 fertig»), Details auf Klick. iPad: «Auswählen» schaltet Häkchen zum Antippen ein.
- **Dateifenster wie Finder und Explorer:** Mehrere Dateien auswählen mit Umschalt-Klick (Bereich), ⌘-/Strg-Klick (einzeln dazu), ⌘A/Strg+A (alle), Umschalt+Pfeiltasten und Ziehen auf freier Fläche (Gummiband); Pfeiltasten, Pos1/Ende und Tippen der ersten Buchstaben springen zur Datei. Am Mac benennt Enter um und ⌘↓ öffnet (wie im Finder), unter Windows und Linux öffnet Enter und F2 benennt um. Alle Aktionen gelten für die ganze Auswahl; auf dem iPad gibt es «Alle auswählen» im Menü (lange drücken).
- **Nie mehr still überschreiben:** Gibt es eine Datei am Ziel schon, fragt Sesam mit beiden Fassungen nebeneinander (Grösse, Datum, welche neuer ist): «Ersetzen», «Überspringen», «Beide behalten», «Nur wenn neuer», auf Wunsch «Für alle übernehmen». Bei Ordnern: «Zusammenführen» (fragt dann je Datei) oder «Beide behalten». Gilt in beide Richtungen und auch beim Ziehen aus Finder/Explorer und «Dateien».
- **Pfad im Dateifenster:** Titel und Pfad überdecken sich nicht mehr; lange Pfade zeigen Anfang und aktuellen Ordner, die Mitte steckt in «…».
- **iPad wie am Mac:** Gespeicherte Passwörter zeigt Sesam auf dem iPad jetzt nach Face ID, Touch ID oder dem Gerätecode. Dateien lassen sich aus der App «Dateien» (Split View oder Slide Over) auf ein Remote-Bild, eine Shell oder die Server-Seite im Dateifenster ziehen und werden hochgeladen. RDP nutzt auch auf dem iPad H.264 über den Decoder des Systems.
- **Zuletzt verwendet am App-Symbol:** Rechtsklick aufs Sesam-Symbol im Dock (Mac) bzw. in der Taskleiste (Windows) zeigt die zuletzt verbundenen Verbindungen sowie «Neue Verbindung …» und «Schnelle Verbindung …». Ein Klick öffnet sie direkt.
- **Vollbild auf dem iPad:** «Vollbild» blendet Leisten, Statusleiste und Home-Leiste aus; das Remote-Bild nimmt den ganzen Bildschirm. Die Verbindungsleiste kommt mit einem Tipp auf den Griff oben in der Mitte oder mit einem Wisch vom oberen Rand nach unten; dort beendet man das Vollbild auch wieder.
- **Kompakte Sitzungsleiste:** Auf dem iPad und in schmalen Fenstern zeigt die Leiste nur noch Tasten, Bildschirmtastatur, Vollbild und Trennen; Ansicht (Anpassen, Skalieren, 1:1), Touch-Bedienung, Lupe und Seltenes liegen im Menü «…». Tabs zeigen den ganzen Namen.
- **Seitenleiste und Details ein- und ausblenden:** Zwei Knöpfe in der Bibliothek blenden die Seitenleiste und die Details aus, Sesam merkt sich die Wahl. Auf dem iPad sind die Details anfangs aus und erscheinen als Blatt über den Karten; im Hochformat ist die Seitenleiste anfangs eingeklappt. Lange Knopftexte in den Details brechen sauber um.
- **Lizenz:** Sesam steht jetzt unter der Mozilla Public License 2.0 (vorher GPL-3.0). Die Lizenztexte von Sesam, den Schriften und Symbolen liegen in der App.
- **iPad bedienbar mit dem Finger:** Die Oberfläche ist auf dem iPad grösser (Text rund 17 pt, Knöpfe, Menüzeilen und Tabs mindestens 44 pt hoch, nach den Richtlinien von Apple), hoch und quer; Seitenleiste und Inhaltsfeld laufen bis an den unteren Rand.
- **Versionsangabe:** Builds zwischen zwei Releases zeigen den Entwicklungsstand («26.11.1 Entwicklungsstand · Commit · Datum») statt einer nackten Nummer; ein Release zeigt nur die Nummer. «Über Sesam Remote» im Mac-Menü öffnet diese Angabe.
- **Kurzbefehl «Verbindung öffnen» (iPad):** In der App «Kurzbefehle», in Spotlight und bei Siri lässt sich eine Verbindung aus der Bibliothek wählen und direkt öffnen, auf Wunsch als Desktop, Shell oder Dateien; als Symbol auf dem Home-Bildschirm ersetzt das die Verknüpfungen auf dem Schreibtisch.
- **Neue Verbindung per Kontextmenü:** Rechtsklick (iPad: lange drücken) auf freie Fläche, Gruppe oder Karte bietet «Neue Verbindung …»; auf einer Karte auch «Neue Verbindung zum gleichen Gerät …» mit Adresse, Gruppe und Benutzer vorausgefüllt.
- **iPad:** Sesam läuft als iPad-App (iPadOS 16 oder neuer). Im Simulator getestet: Bibliothek, Einstellungen, SSH-Shell mit Zusatztasten über der Bildschirmtastatur, Erste Hilfe und VNC mit Touch (Pinch-Zoom, Touchpad/Direkt). Ausgeblendet ist, was es nur auf Computern gibt (lokales Terminal, Verknüpfungen, eigene Fenster, Mac-Tastenkürzel).
- **Zusatztasten für Touch:** Die Leiste über der Bildschirmtastatur hat jetzt auch Pos1, Ende, Bild auf/ab, die Zeichen | ~ / - und ausklappbare F1–F12.
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
- **VNC mit RealVNC:** Bietet ein Server nur dessen eigenes Anmeldeverfahren (RA2) an, erklärt Sesam die zwei Auswege (VNC-Passwort in RealVNC oder wayvnc auf aktuellem Raspberry Pi OS) statt einer Fehlernummer.
- **Ruhigere, plattformtypische Optik:** Seitenleiste wie eine Quellenliste (getönte Auswahl, kompakte Zeilen), klarere Titelzeile mit einer Hauptaktion, Karten mit grösserer Vorschau und dezentem Anheben, Sitzungskopf niedriger mit leiser Qualitätsanzeige, Einstellungen mit Fokus auf dem gewählten Bereich.
- **Statusmeldungen:** Hinweise blenden sich nach 6 s aus, Fehler bleiben stehen, bis man sie anklickt.
- **Verbindungsqualität als Signalbalken:** Ein Klick zeigt Bilder pro Sekunde, Datenmenge und Latenz in einem kleinen Fenster.
- **Einstellungen auf dem Mac in einem eigenen Fenster** (⌘,), wie bei anderen Mac-Apps; unter Windows bleibt das Sheet.
- **Eine Leiste für Tabs und Werkzeuge:** Ab 1000 px Fensterbreite stehen die Werkzeuge der Sitzung rechts in der Tab-Leiste; das Remote-Bild bekommt rund 44 px mehr Höhe. Titel und Adresse zeigt der Tooltip des Tabs; in schmalen Fenstern bleibt es bei zwei Leisten.

### Behoben

- **Sicherheit, Verbindungen:** RD-Gateway und KDC-Proxy prüfen das Zertifikat jetzt gegen die Stammzertifikate des Systems (vorher nahmen sie jedes an). Ein VNC-Server, der schon einmal verschlüsselt war, bekommt das Passwort nie mehr unverschlüsselt; ein SSH-Schlüssel anderen Typs wird deutlich als Warnung gemeldet. Bericht: `docs/security-review-2026-11-11.md`.
- **Sicherheit, Dateien:** Ein Dateiname vom Server kann beim Hochladen mit Administratorrechten keine Befehle mehr ausführen. «Bearbeiten» öffnet keine Dateien mehr, die das System ausführen würde. Die gemeinsame Zwischenablage holt keine Ordner und nicht mehr als 512 MB ungefragt.
- **Sicherheit, Zugangsdaten:** Eine Sicherung mit Passwörtern verlangt auf Mac und iPad Touch ID bzw. Face ID. Eine eingelesene fremde Sicherung kann vorhandene Zugangsdaten weder benutzen noch überschreiben. Über das Terminal darf ein Server die Zwischenablage nur mit «Gemeinsame Zwischenablage» setzen. Unplausible Bildgrössen eines VNC-Servers lehnt Sesam ab.
- **SSH und Dateien auf dem iPad:** Die erste Verbindung zu einem Server scheiterte auf dem echten iPad, weil Sesam den Server-Schlüssel in einem Ordner speichern wollte, den iPadOS nicht beschreiben lässt. Er liegt jetzt bei den Daten der App.
- **Mehrere Dateien gleichzeitig ersetzen:** Ersetzte man mehrere Dateien auf einmal, konnte das Sichern der alten Fassungen in den Papierkorb scheitern.
- **«Anpassen» bei Geräten mit festem Monitor:** Hat ein VNC-Server einen fest angeschlossenen Bildschirm (z. B. ein Raspberry Pi am HDMI-Monitor), vergrösserte «Anpassen» nur die Fläche und das Bild füllte bloss einen Teil davon. Sesam erkennt das jetzt, stellt die alte Grösse wieder her und skaliert das Bild auf die ganze Fläche.
- **Dateifenster nach einem Server-Neustart:** Wurde der Server neu gestartet (oder brach die Verbindung ab), während das Dateifenster offen war, scheiterten Vorschau, Bearbeiten und andere Aktionen mit «session closed», obwohl die Liste noch da war. Sesam merkt jetzt, dass die SSH-Verbindung weg ist, meldet sich einmal still neu an und führt die Aktion aus.
- **Einstellungen auf dem Mac:** Das geschlossene Blatt «Verbindung bearbeiten» schimmerte durch die Seitenleiste des Einstellungsfensters, und «Allgemein» trug einen Fokusring, obwohl ein anderer Bereich gewählt war. Beides ist behoben; die Einstellungen zeigen genau eine Markierung.
- **Ganzes Fenster:** Auf dem iPad keine weissen Balken mehr oben und unten; auf allen Systemen ist auch die Fläche hinter der App in der App-Farbe (hell und dunkel).
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
