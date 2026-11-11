# Design-Konzept: Inhalt und Glas

Stand 11.11.2026, zweiter Durchgang mit Inventar `docs/design-inventar.md` (Auftrag Mike: «moderner Anstrich, der auch mit Liquid Glass gut aussieht, konsequent, auch
für Windows»). Grundlage: Apples Human Interface Guidelines (Materials, Liquid Glass, Toolbars, Sidebars,
Accessibility) und Fluent 2 für Windows. Gilt für alle Fenster und alle Systeme; Abweichungen je System stehen
unten. Umgesetzt in `src/styles.css` (Abschnitt «Design-System»), Fenster-Material in `src-tauri/src/lib.rs`
(`ui_material`).

## Grundsatz

Sesam zeigt fremde Bildschirme, Shells und Dateien. **Der Inhalt hat Vorrang**, die Bedienung tritt zurück und
schwebt als eigene Ebene darüber.

```text
┌──────────────────────────────────────────────┐
│ Bedienebene (Glas)                            │  Seitenleiste, Titel-/Werkzeugleisten, Tabs,
│                                               │  Sitzungsleiste, Menüs, Blätter, Popover
├──────────────────────────────────────────────┤
│ Inhaltsebene (deckend)                        │  Karten, Listen, Terminal, Remote-Bild,
│                                               │  Dateilisten, Formulare
└──────────────────────────────────────────────┘
```

1. **Zwei Ebenen, nie Glas auf Glas.** Glas nur in der Bedienebene. Karten, Listen, Terminal und Remote-Bild
   bleiben deckend (Terminal und Remote-Bild immer, auch für die Lesbarkeit).
2. **Keine Kästen in Kästen.** Gruppen entstehen durch Abstand und Schrift, nicht durch Rahmen. Linien nur, wo
   sie etwas trennen, das sonst verschwimmt (Tabellenköpfe, Teilfenster im Dateifenster).
3. **Konzentrische Ecken.** Innerer Radius = äusserer Radius − Abstand. Fenster → Inhaltsfläche → Karte →
   Vorschaubild folgen derselben Kurve.
4. **Kapseln für schwebende Bedienelemente.** Knöpfe in Leisten, Segmente und Tabs sind Kapseln bzw. stark
   gerundet; Formularfelder und Listenzeilen bleiben ruhig rechteckig mit kleinem Radius.
5. **Farbe sparsam.** Grün (`#157A5A` hell, `#4CD08F` dunkel) nur für Hauptaktion, Auswahl und Status. Leisten
   und Symbole bleiben einfarbig.
6. **Hierarchie durch Schrift und Material**, nicht durch Farbe: Titel 20/600, Abschnitt 13/600, Text 13/400,
   Nebeninfo 12/400 in `--muted`, Kleinstes 11.

## Tokens

Alle Farben, Radien, Abstände, Höhen, Schatten und Bewegungen kommen aus Variablen am Anfang von
`src/styles.css`; ausserhalb davon gibt es keine festen Werte. Ausnahmen sind nur Inhaltsfarben: Gruppenfarben,
Terminal-Designs, Vorschaubilder, Weiss auf farbigen Flächen und das Rot des Windows-Schliessen-Knopfs.

| Gruppe | Token | Mac / iPad | Windows (Fluent 2, `body.custom-titlebar`) |
|---|---|---|---|
| Radius Bedienelement | `--r-ctl` | 999 px (Kapsel) | 4 px |
| Radius Feld, Listenzeile | `--r-field`, `--r-row` | 8 px | 4 px |
| Radius Karte, Menü | `--r-card`, `--r-menu` | 16 / 12 px | 8 px |
| Radius Panel, Blatt | `--r-panel`, `--r-sheet` | 20 px | 8 px |
| Abstand | `--s-1` … `--s-6` | 4 / 8 / 12 / 16 / 24 / 32 px | gleich |
| Höhe Knopf | `--h-sm` / `--h-md` / `--h-lg` | Mac 24 / 30 / 36 px, iPad 30 / 34 / 40 px (mal 1,3 = mind. 44 pt) | 24 / 32 / 36 px |
| Höhe Zeile, Leiste | `--h-row`, `--h-bar` | 30 (iPad 36) / 52 px | gleich |
| Glas | `--glass-bg`, `--glass-strong`, `--glass-edge`, `--glass-blur` | Weiss 62 % bzw. Dunkelgrau 58 %, Unschärfe 30 px, Sättigung 1,6 | gleich (Acrylic-Optik) |
| Höhenstufe | `--elev-1` / `--elev-2` / `--elev-3` | Inhaltsfläche, Karte / Menü, Seitenleiste / Blatt | gleich |
| Bewegung | `--ease`, `--dur-s` / `--dur-m` | `cubic-bezier(.2,.8,.2,1)`, 120 / 200 ms | gleich |

Farben sind semantisch (`--bg`, `--surface`, `--card`, `--text`, `--muted`, `--line`, `--line-strong`, `--fill`,
`--fill-strong`, `--accent`, `--on-accent`, `--danger`, `--select`), je hell und dunkel.

## Knöpfe: ein System

| Variante | Klasse | Wofür |
|---|---|---|
| Standard (getönt) | `button` | jede normale Aktion |
| Hauptaktion | `.primary` | höchstens eine je Ansicht, rechts aussen |
| Text | `.plain` | leise Aktion ohne Fläche (z. B. «‹ Bibliothek») |
| Link | `.link` | im Fliesstext |
| Zerstörend | `.danger`, `.primary.danger` | Löschen, Ersetzen; in Rückfragen automatisch rot |
| Symbol | `.tool`, `.tool.on`, `.tool.with-text` | Leisten, Werkzeuge |

Grösse kommt nur über `--h-ctl` des Behälters (`--h-sm`, `--h-md`, `--h-lg`), nie über eigene Höhen. Gleiche
Schrift (500, Hauptaktion 600), Abstände und Symbolabstand (6 px) für alle; Zustände nur Hover, Drücken
(97 %), Deaktiviert (40 %) und Fokus. Auswahlmenü-Knöpfe in Formularen (`.menu-button`) sehen aus wie Felder,
weil sie einen Wert zeigen; Befehle wie «Ändern» sind normale Knöpfe.

**Fokusring nur nach Tastatur:** `:focus-visible` plus `body.kbd` (gesetzt bei Tab oder Pfeiltaste, entfernt bei
Zeiger oder Finger). Textfelder zeigen ihren Ring immer, wenn sie den Fokus haben.

## Blatt (Dialog): eine Komponente

Alle Dialoge (Verbindung, Schnelle Verbindung, Rückfrage, Erste Hilfe, Desktop freischalten, Konflikt, Vorschau,
Berechtigungen, Hilfe, Einstellungen, Befehlspalette) sind `dialog` mit `.sheet`: deckende Fläche `--card`,
Radius `--r-sheet`, Höhenstufe 3, Titel zentriert in der Fläche (keine eigene Titelleiste), Höhe nach Inhalt
(bis zur Fensterhöhe, dann scrollt nur der Inhalt), Knopfleiste `.actions` unten in derselben Fläche,
Hauptaktion rechts, zerstörende Aktion links. Es ist immer nur ein Blatt sichtbar; ein zweites (Rückfrage über
dem Blatt) dunkelt das erste ab. Auf dem Mac sind die Einstellungen ein eigenes Fenster, in dem nur die
Einstellungen liegen.

Gruppen in Blättern (`fieldset`) sind eine Fläche; Zeilen darin (`label.option`) haben keine eigene Karte,
sondern eine Trennlinie. Listen (Erste Hilfe, Berechtigungen, Zertifikate, Zugangsdaten, Terminal-Profile,
Gruppen) sind gruppierte Listen: eine Fläche, Zeilen mit Trennlinie.

Kontextmenüs öffnen am Zeiger bzw. Finger; Auswahlmenüs unter ihrem Knopf.

## Je System

- **macOS:** Echtes Fenstermaterial hinter der Webansicht (Tauri `windowEffects`, NSVisualEffectView
  «sidebar», folgt dem aktiven Fenster). Die Seitenleiste und der Fensterhintergrund sind durchsichtig und zeigen
  das Material; Titelleiste ohne eigenen Hintergrund (Ampel schwebt darüber). Ab macOS 26 sieht das Material wie
  Liquid Glass des Systems aus; ältere macOS bekommen Vibrancy. Schwebende Leisten in der Webansicht (Sitzung,
  Menüs, Blätter) nutzen CSS-Glas mit derselben Tönung.
- **iPadOS:** gleiche Ebenen mit CSS-Glas (WKWebView ist nicht durchsichtig zum System); Seitenleiste und
  Inhaltsfläche schweben auch im Hochformat (Seitenleiste eingeklappt) als abgerundete Flächen über dem
  Hintergrund, Details als Glas-Blatt. Touch-Grössen bleiben (44 pt).
- **Windows 11:** Mica als Fensterhintergrund (Seitenleiste und Titelleiste), Acrylic-Optik für Menüs und
  Flyouts, Fluent-Radien 8/4 px, Segoe UI Variable, eigene Titelleiste mit Tabs wie heute. **Windows 10:** ohne
  Mica, deckend.
- **Linux:** deckend mit denselben Tokens.

## Barrierefreiheit

- **Transparenz reduzieren** (macOS: Bedienungshilfen › Anzeige; Windows: Transparenzeffekte aus): kein
  Fenstermaterial, kein CSS-Glas, deckende Flächen. macOS meldet das nativ (`ui_material`), Windows über
  `prefers-reduced-transparency`.
- **Kontrast erhöhen** (`prefers-contrast: more` bzw. nativ): deckend, kräftigere Linien (1 px `--text` mit
  40 %), Fokusring 3 px.
- **Bewegung reduzieren:** keine Übergänge ausser Deckkraft.
- Terminal und Remote-Bild sind nie durchsichtig.

## Bildschirme

| Bildschirm | Bedienebene (Glas) | Inhalt (deckend) |
|---|---|---|
| Bibliothek | Seitenleiste, Titelleiste mit Suche, Details-Blatt (iPad) | Inhaltsfläche mit Karten/Liste, Details-Spalte (Mac/Windows) |
| Sitzung | Tab-Leiste, Werkzeugleiste, Menü «…», Vollbild-Leiste | Remote-Bild, Terminal, Dateilisten |
| Dateifenster | Kopf mit Pfad, Übertragungen als schwebende Fläche unten | beide Listen |
| Einstellungen | Navigation links (Material) | Inhalt rechts |
| Dialoge, Blätter, Menüs, Befehlspalette, Popover | Glas, Höhenstufe 2–3 | Formularfelder darin deckend |

## Nicht gemacht (bewusst)

- Kein Glas auf Karten, Listenzeilen oder Feldern (HIG: nur Bedienebene).
- Keine eigene Darstellungsoption «Glas aus» in Sesam: das System entscheidet (Transparenz reduzieren).
