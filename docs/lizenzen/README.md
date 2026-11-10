# Lizenzen mitgelieferter Schriften

Sesam liefert zwei Schriften mit (nur die Zeichensätze Latin und Latin-Ext, als WOFF2 in `src/fonts/`).
Es gibt keine Verbindung zu Google Fonts oder einem anderen Dienst.

| Schrift | Version | Quelle | Lizenz | Verwendung |
|---|---|---|---|---|
| Inter (variabel, optische Grössen) | 5.3.0 | npm `@fontsource-variable/inter` | SIL Open Font License 1.1 (`Inter-OFL.txt`) | Oberfläche unter Windows/Linux |
| JetBrains Mono (variabel) | 5.3.0 | npm `@fontsource-variable/jetbrains-mono` | SIL Open Font License 1.1 (`JetBrainsMono-OFL.txt`) | Shell, Adressen, Pfade |

Unter macOS/iOS nutzt Sesam die Systemschrift SF Pro (Apple HIG); sie wird nicht mitgeliefert.
Die OFL erlaubt das Bündeln mit Software, auch unter GPL-3.0; die Schriften selbst bleiben unter der OFL.

# Symbole

| Symbole | Version | Quelle | Lizenz | Verwendung |
|---|---|---|---|---|
| Fluent System Icons (Microsoft) | 1.1.345 | npm `@fluentui/svg-icons` | MIT (`FluentSystemIcons-MIT.txt`) | alle Werkzeug-, Menü- und Fenstersymbole |
| Simple Icons | 16.35.0 | npm `simple-icons` | CC0 1.0 (gemeinfrei) | Logos von Linux (Tux), Ubuntu, Apple und Raspberry Pi |

Ins Programm kommen nur die tatsächlich benutzten SVG-Dateien (als Inline-SVG). Das Windows-Zeichen (vier Felder)
ist selbst gezeichnet, weil Simple Icons es nicht mehr führt. Die Logos stehen nur zur Kennzeichnung des
Betriebssystems eines Zielrechners; die Marken gehören ihren Inhabern. Auf macOS zeigt Sesam
für Werkzeug- und Menüsymbole statt Fluent die SF Symbols von Apple; das System zeichnet sie zur Laufzeit (`src-tauri/src/sfsymbol.rs`), sie werden nicht mitgeliefert und dürfen nur auf Apple-Geräten verwendet werden.

## Farbschemata der Shell

Nur die Farbwerte (16 Farben, Hinter-/Vordergrund, Cursor, Auswahl) sind übernommen, keine Dateien. Die
Systemdesigns (macOS Terminal, Windows Terminal Campbell, PowerShell, Ubuntu, GNOME, Konsole) geben die
Standardfarben dieser Programme wieder, damit sich die Shell vertraut anfühlt.

| Schema | Quelle | Lizenz |
|---|---|---|
| Solarized (hell/dunkel) | Ethan Schoonover, github.com/altercation/solarized | MIT |
| One Dark / One Light | Atom, github.com/atom/atom (one-dark-syntax, one-light-syntax) | MIT |
| Gruvbox (hell/dunkel) | Pavel Pertsev, github.com/morhetz/gruvbox | MIT |
| Nord | Arctic Ice Studio, github.com/nordtheme/nord | MIT |
| Dracula | Dracula Theme, github.com/dracula/dracula-theme | MIT |
| Tokyo Night / Day | Folke Lemaitre, github.com/folke/tokyonight.nvim | Apache-2.0 |
| Catppuccin Latte / Mocha | Catppuccin, github.com/catppuccin/catppuccin | MIT |
| GitHub (hell/dunkel) | GitHub Primer, github.com/primer/github-vscode-theme | MIT |
| Tango | Tango Desktop Project | gemeinfrei |
