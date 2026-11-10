# Optik-Durchgang: grösste Schwächen und Vorschläge

Grundlage: Bildschirmfotos der echten App auf macOS 27 (hell und dunkel, kleinstes und normales Fenster) und die
Human Interface Guidelines von Apple. Unter Windows gelten dieselben Punkte sinngemäss (Fluent statt SF). Umsetzen
nach Freigabe, jeweils mit Vorher/Nachher-Foto.

## Bibliothek

1. **Drei gleich gewichtete Knöpfe oben rechts** («Terminal ⌄», «Schnelle Verbindung», «+ Neue Verbindung»). HIG: eine
   klare Hauptaktion. Vorschlag: «+ Neue Verbindung» bleibt betont; «Schnelle Verbindung» und «Terminal» werden
   Symbolknöpfe (Blitz, Terminal) mit Tooltip und Kürzel.
2. **Ansichtsumschalter Raster/Liste** steht zwischen Titel und Aktionen und wirkt wie eine Aktion. Vorschlag: an den
   rechten Rand der Titelzeile, kleiner, wie in Finder.
3. **Online-Punkt ohne Erklärung** (grün/grau). Vorschlag: Tooltip «Erreichbar · 2 ms» bzw. «Nicht erreichbar», bei
   Hover auf der Karte; grau mit Rand statt gefüllt, damit es nicht wie «aus» aussieht.
4. **Karten-Unterzeile nur IP.** Vorschlag: Benutzer@Adresse und die Dienste als kleine Symbole (Desktop, Shell, Dateien),
   damit man sieht, was ein Doppelklick öffnet.
5. **Seitenleiste:** Zähler und Gruppenkopf «Gruppen» sitzen eng; die Auswahl «Alle» hat einen eigenen Balken links.
   Vorschlag: Quellenliste wie in Finder (Abschnittstitel klein und grau, Auswahl als abgerundete Fläche ohne Balken).

## Tab-Leiste und Sitzungskopf

6. **Zwei Leisten übereinander** (Tabs, darunter Titel mit Werkzeugen) kosten rund 96 px Höhe. HIG «unified toolbar»:
   Vorschlag: Titel und Unterzeile in die Tab-Leiste (aktiver Tab zeigt sie als Tooltip), Werkzeuge in eine einzige
   schmale Leiste; im Vollbild ohnehin ausgeblendet.
7. **«‹ Bibliothek» als Textknopf** neben den Tabs. Vorschlag: Symbol der Seitenleiste (sidebar.left) wie in Safari,
   Kürzel ⌘⇧L; die Bibliothek als erster fester Tab ohne Schliessen-Knopf wäre die Alternative.
8. **Inaktive Tabs** haben wenig Kontrast zum Hintergrund; Trennung zwischen Tabs fehlt. Vorschlag: dünne Trennlinie
   zwischen inaktiven Tabs, aktiver Tab mit Fläche wie heute.
9. **Verbindungsqualität («55 fps · 30 MB/s») als Text in der Werkzeugleiste** wirkt technisch und verschiebt die
   Knöpfe. Vorschlag: kleines Symbol (Signalbalken) mit Popover für Bilder/s, Daten und Latenz.

## Remote-Bild und Shell

10. **Statusmeldungen als Pille unten mittig** überdecken das Bild und bleiben stehen. Vorschlag: nach 5 s ausblenden,
    Fehler in der Leiste «Verbindung getrennt» oben bündeln (ein Ort für Zustände).
11. **Terminal im Dunkelmodus:** die Bildlaufleiste zeigt eine hellere Spur rechts. Vorschlag: Spur transparent, Leiste
    nur beim Scrollen (macOS-Verhalten «Beim Scrollen einblenden»).

## Dateien und Übertragungsliste

12. **Übertragungsliste** in einem Raster mit fester Spalte 170 px; lange Fehler brechen jetzt um, schieben aber die
    Zeile auf. Vorschlag: Zeile mit Name und Fortschritt, darunter Status in kleiner Schrift; Fehler rot mit Symbol und
    Knopf «Erneut versuchen» bzw. «Mit Administratorrechten».
13. **Kontextmenüs** mischen Einträge mit und ohne Symbol. HIG: entweder alle mit Symbol oder keine in einem Abschnitt.

## Einstellungen

14. **Einstellungen als Sheet über der Bibliothek.** HIG: eigenes Fenster «Einstellungen» (⌘,) mit Symbolleiste
    oben (Allgemein, Shell, Sicherheit, Zugangsdaten …). Vorschlag: auf dem Mac eigenes Fenster, unter Windows wie heute.
15. **Fokusrahmen bleibt auf «Allgemein»**, wenn ein anderer Bereich gewählt ist. Vorschlag: Fokus auf den gewählten
    Eintrag setzen (Tastatur) und den Rahmen nur bei Tastaturbedienung zeigen (`:focus-visible`).
16. **Zugangsdaten-Zeilen:** Textlinks «Bearbeiten» und «Weitere …» rechts. Vorschlag: ganze Zeile anklickbar (öffnet
    Bearbeiten), «…»-Symbolknopf für das Menü.

## Querschnitt

17. **Dialoge** nutzen ein zweispaltiges Formularraster; neue Dialoge müssen `grid-column: 1 / -1` setzen, sonst wird es
    eng. Vorschlag: einspaltiges `.sheet.single` als Standard für Dialoge ohne Formularfelder.
18. **Schriftgrössen:** 11, 12 und 13 px gemischt in Unterzeilen. Vorschlag: zwei Stufen (13 Inhalt, 11 Neben­text) wie
    die macOS-Textstile «Body» und «Caption».
