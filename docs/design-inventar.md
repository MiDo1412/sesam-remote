# Design-Inventar «Inhalt und Glas»

Jeder Bildschirm und jedes Bauteil, geprüft gegen `docs/design-konzept.md` in der echten App: Mac mit der
Entwicklungs-App, iPad im Simulator (iPad Pro 13", Hochformat, Debug-Build). ✓ = fotografiert, geprüft und
stimmig. ○ = nicht live geprüft, der Grund steht in der Bemerkung. Die Fotos liegen in `test-build/diag/design/`
(nicht im Git): Mac `<name>-light.png` bzw. `-dark.png`, iPad mit `ipad-` davor. Vorher-Fotos liegen in
`vorher/`, der erste Durchgang in `erster-durchgang/`. Windows und Linux sind nur im Code umgesetzt
(`body.custom-titlebar`: Fluent-Radien, 32 px); hier gibt es keine Geräte dafür, sie sind nicht getestet.

Stand 11.11.2026.

## Bauteile

| Bauteil | Mac hell | Mac dunkel | iPad hell | iPad dunkel | Fotos / Bemerkung |
|---|---|---|---|---|---|
| Knöpfe: Standard, Hauptaktion, Text, Link, Löschen, Löschen als Hauptaktion, Symbol, Symbol aktiv, Symbol mit Text, inaktiv; je Grösse S/M/L | ✓ | ✓ | ✓ | ✓ | `knoepfe` (eigens eingeblendetes Musterblatt mit den echten Klassen) |
| Fokusring nur nach Tastatur | ✓ | ✓ | ✓ | ✓ | `einst-shell-ueberblatt`, `blatt-rueckfrage`: kein Ring nach dem Öffnen oder Klicken |
| Felder, Suche, Auswahlmenü-Knopf | ✓ | ✓ | ✓ | ✓ | `neu-allgemein`, `neu-reiter1`–`4`, `schnell` |
| Segmente, Reiter im Blatt | ✓ | ✓ | ✓ | ✓ | `neu-reiter*`, `ersthilfe`, `einst-general` |
| Blatt: Titel, Inhalt, Knopfleiste ohne Nähte | ✓ | ✓ | ✓ | ✓ | `neu-reiter2`/`4` (lang, scrollt), `schnell`, `vorschau` |
| Gruppe im Blatt, Optionszeilen ohne Kasten im Kasten | ✓ | ✓ | ✓ | ✓ | `neu-reiter2`, `neu-reiter4` |
| Rückfrage, auch zerstörend (rot) | ✓ | ✓ | ✓ | ✓ | `blatt-rueckfrage`, `hostkey` |
| Menü, Kontextmenü (am Zeiger) | ✓ | ✓ | ✓ | ✓ | `kontextmenue`, `datei-kontext`, `blatt-menue` |
| Gruppierte Liste | ✓ | ✓ | ✓ | ✓ | `ersthilfe`, `einst-terminal`, `einst-creds` |
| Popover Feldhilfe, Verbindungsqualität | ○ | ○ | ○ | ○ | nicht eigens fotografiert; gleiche Glas-Regel wie Menüs (`.popover`, `.quality-pop`) |

## Gestapelte Zustände

| Zustand | Mac hell | Mac dunkel | iPad hell | iPad dunkel | Fotos / Bemerkung |
|---|---|---|---|---|---|
| Menü über Blatt | ✓ | ✓ | ✓ | ✓ | `blatt-menue` |
| Rückfrage über Blatt | ✓ | ✓ | ✓ | ✓ | `blatt-rueckfrage`: Blatt darunter abgedunkelt |
| Einstellungen über offenem Blatt | ✓ | ✓ | ✓ | ✓ | Mac: eigenes Fenster `einst-shell-ueberblatt` (nichts schimmert durch), Hauptfenster `blatt-hinter-einst`; iPad: `ipad-einst-ueber-blatt` (Blatt verdeckt) |
| Bereich in den Einstellungen gewechselt: eine Markierung | ✓ | ✓ | ✓ | ✓ | `einst-general-klick` |
| Konflikt über Dateifenster | ✓ | ✓ | – | – | `konflikt`; iPad nicht ausgelöst (gleiches Blatt wie Mac) |

## Bildschirme

| Bildschirm | Mac hell | Mac dunkel | iPad hell | iPad dunkel | Fotos / Bemerkung |
|---|---|---|---|---|---|
| Bibliothek: Kacheln, Seitenleiste, Titelzeile | ✓ | ✓ | ✓ | ✓ | `bibliothek` |
| Bibliothek: Liste | ✓ | ✓ | ✓ | ✓ | `liste` |
| Bibliothek: Suche ohne Treffer | ✓ | ✓ | ✓ | ✓ | `suche-leer` (neu: Hinweis «Keine Verbindungen gefunden») |
| Bibliothek: leer (keine Verbindung) | ○ | ○ | ○ | ○ | nicht ausgelöst, dafür müsste die Test-Bibliothek gelöscht werden; Bauteile wie oben |
| Bibliothek: Details (Spalte bzw. Blatt) | ✓ | ✓ | ✓ | ✓ | `details` |
| Neue/Bearbeiten Verbindung: alle Reiter | ✓ | ✓ | ✓ | ✓ | `neu-allgemein`, `neu-reiter1`–`4` |
| Schnelle Verbindung | ✓ | ✓ | ✓ | ✓ | `schnell`, `schnell-dateien` |
| Sitzung: Tab-Leiste, Werkzeuge, «…» | ✓ | ✓ | ✓ | ✓ | `terminal`, `sitzung-mehr`, `vnc-mehr` |
| Shell (lokal und SSH) | ✓ | ✓ | ✓ | ✓ | `terminal`, `ssh-shell`, `ipad-shell` |
| Remote-Bild VNC | ✓ | ✓ | ✓ | ✓ | `vnc` (Testserver auf 127.0.0.1) |
| Remote-Bild RDP | ○ | ○ | ○ | ○ | kein RDP-Testserver; gleiche Sitzungsansicht wie VNC |
| Vollbild-Leiste, Griff (iPad) | ○ | ○ | ✓ | ✓ | `ipad-vollbild`, `ipad-vollbild-leiste`; Mac-Vollbild nicht ausgelöst (eigener Space) |
| Dateifenster: Listen, Kopf, Übertragungen | ✓ | ✓ | ✓ | ✓ | `dateien` |
| Konflikt | ✓ | ✓ | – | – | `konflikt` |
| Vorschau | ✓ | ✓ | ✓ | ✓ | `vorschau` (jetzt einheitliches Blatt) |
| Rechte | ✓ | ✓ | ✓ | ✓ | `rechte` |
| Unbekannter Server (Host-Schlüssel) | ✓ | ✓ | ✓ | ✓ | `hostkey` |
| Einstellungen: Allgemein, Shell mit Design-Galerie, Sicherheit, Zugangsdaten, Terminal, Gruppen, Über | ✓ | ✓ | ✓ | ✓ | `einst-*` (Terminal nur Mac, auf dem iPad gibt es kein lokales Terminal) |
| Befehlspalette | ✓ | ✓ | ✓ | ✓ | `palette` |
| Erste Hilfe | ✓ | ✓ | ✓ | ✓ | `ersthilfe` |
| Desktop freischalten | ✓ | ✓ | ○ | ○ | `freischalten`; iPad nicht ausgelöst, gleiches Blatt |
| Berechtigungen einrichten (nur Mac) | ○ | ○ | – | – | nicht geöffnet: die Prüfung «Lokales Netzwerk» kann eine Systemabfrage auslösen; gleiche Liste wie Erste Hilfe (`.perm-list`) |
| Hilfe | ✓ | ✓ | ✓ | ✓ | `hilfe` (iPad: ohne Bildschirmtastatur beim Öffnen) |
| Über Sesam | ✓ | ✓ | ✓ | ✓ | `einst-about` |
| Statusmeldungen, Neu-verbinden-Leiste | ✓ | ✓ | ✓ | ✓ | `neuverbinden`, `vnc-neuverbinden` |
| Abgekoppeltes Fenster (Mac, Windows) | ○ | ○ | – | – | nicht ausgelöst |
