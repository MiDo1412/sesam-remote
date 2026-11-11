# Design-Konzept: Inhalt und Glas

Stand 11.11.2026 (Auftrag Mike: «moderner Anstrich, der auch mit Liquid Glass gut aussieht, konsequent, auch
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

| Gruppe | Token | Wert (Mac/iPad) | Windows (Fluent 2) |
|---|---|---|---|
| Radius | `--r-xs` / `--r-s` / `--r-m` / `--r-l` / `--r-xl` | 6 / 8 / 12 / 16 / 22 px | 4 / 4 / 8 / 8 / 8 px |
| Kapsel | `--r-pill` | 999 px | 999 px (nur Segmente und Leistenknöpfe: 4 px) |
| Abstand | `--s-1` … `--s-6` | 4 / 8 / 12 / 16 / 24 / 32 px | gleich |
| Höhe | `--h-control` / `--h-bar` | 28 / 52 px (iPad 44 / 56) | 32 / 48 px |
| Glas | `--glass-bg`, `--glass-edge`, `--glass-shadow`, `--glass-blur` | Weiss bzw. Schwarz 62–72 %, Kante 1 px hell, Unschärfe 28 px, Sättigung 1,5 | Acrylic: 80 % Fläche, Unschärfe 30 px |
| Höhenstufe | `--elev-1` / `--elev-2` / `--elev-3` | Karte / Menü / Blatt | Fluent Shadow 4 / 16 / 28 |
| Bewegung | `--ease`, `--dur-s` / `--dur-m` | `cubic-bezier(.2,.8,.2,1)`, 120 / 220 ms | gleich |

Farben bleiben semantisch (`--bg`, `--surface`, `--card`, `--text`, `--muted`, `--line`, `--accent`), je
hell und dunkel; Text auf Glas mindestens 4,5:1, weil das Glas mit 62–72 % Deckung getönt ist.

## Je System

- **macOS:** Echtes Fenstermaterial hinter der Webansicht (Tauri `windowEffects`, NSVisualEffectView
  «sidebar», folgt dem aktiven Fenster). Die Seitenleiste und der Fensterhintergrund sind durchsichtig und zeigen
  das Material; Titelleiste ohne eigenen Hintergrund (Ampel schwebt darüber). Ab macOS 26 sieht das Material wie
  Liquid Glass des Systems aus; ältere macOS bekommen Vibrancy. Schwebende Leisten in der Webansicht (Sitzung,
  Menüs, Blätter) nutzen CSS-Glas mit derselben Tönung.
- **iPadOS:** gleiche Ebenen mit CSS-Glas (WKWebView ist nicht durchsichtig zum System); Touch-Grössen bleiben
  (44 pt).
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
