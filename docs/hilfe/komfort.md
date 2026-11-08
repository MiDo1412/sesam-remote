# Komfortfunktionen

## Online-Status und Aufwecken

In der Bibliothek zeigt ein grüner Punkt vor der Adresse, dass der Rechner antwortet. Fährst du mit der Maus darüber, steht die Latenz in Millisekunden. Ein grauer Punkt heisst «nicht erreichbar». Sesam prüft alle 30 Sekunden, solange die Bibliothek offen ist.

War ein Rechner einmal erreichbar, merkt sich Sesam seine MAC-Adresse (nur im selben Netz möglich). Ist er später aus, erscheint im Detailbereich **Aufwecken und verbinden**: Sesam schickt ein Wake-on-LAN-Signal und verbindet, sobald er antwortet. Die MAC-Adresse lässt sich auch unter *Bearbeiten › Erweitert* eintragen. Wake-on-LAN muss im BIOS/UEFI und bei der Netzwerkkarte eingeschaltet sein.

## Sicher über SSH verbinden

Unter *Bearbeiten › Desktop* schickt **Sicher über SSH verbinden** RDP oder VNC durch einen SSH-Tunnel. Der Desktop-Dienst muss dann nicht offen im Netz stehen. Klassisches VNC, das selbst nicht verschlüsselt, ist so geschützt. Die Verbindung braucht dafür SSH (*Dienste › Shell*); den Schlüssel des Servers einmal in der Shell bestätigen.

## Automatisch wiederverbinden

Bricht eine RDP-Verbindung ab, etwa durch WLAN-Wechsel oder Ruhezustand, verbindet Sesam selbst neu: bis zu fünf Versuche nach 2, 4, 8, 16 und 30 Sekunden. Windows setzt dabei die laufende Sitzung fort. Trennt der Server absichtlich (Abmelden, anderer Benutzer), verbindet Sesam nicht neu.

## Befehlspalette

**Strg+Umschalt+P** (macOS ⌘⇧P) öffnet die Befehlspalette: Namen einer Verbindung oder Aktion tippen, mit den Pfeiltasten wählen, Enter. Sie kennt alle Verbindungen (Desktop, Shell, Dateien), neue und schnelle Verbindungen, neue Terminals, die Einstellungen, die Hilfe und den Zwischenablage-Verlauf.

## Textbausteine und «An alle Shells senden»

In jeder Shell unter **…**:

- **Textbausteine:** häufige Befehle je Verbindung speichern und mit einem Klick einfügen. Endet ein Baustein mit `\n`, wird er gleich ausgeführt. Keine Passwörter speichern, die Bausteine liegen unverschlüsselt.
- **An alle Shells senden:** Was du tippst, geht zusätzlich an alle anderen offenen Shells, etwa um dasselbe Update auf mehreren Servern zu starten. Nochmals wählen schaltet es aus.

## Verbindungsqualität

Im Desktop-Tab steht oben links, wie viele Bildaktualisierungen pro Sekunde ankommen, wie viele Bilddaten das sind (entpackt) und die Latenz zum Rechner. Fährst du mit der Maus darüber, erscheinen die Werte ausführlich.

## Zwischenablage-Verlauf

Sesam merkt sich die letzten 25 Texte der Zwischenablage, auch Kopien vom entfernten Rechner, solange Sesam im Vordergrund ist. Sie stehen in der Befehlspalette unter «Zwischenablage: …»; Enter legt den Text wieder in die Zwischenablage. Der Verlauf liegt nur im Arbeitsspeicher und ist nach dem Beenden weg.
