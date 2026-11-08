# Versionierung und Versionshinweise

Nach dem alldesk-Standard, angepasst an Tauri.

## Eine Nummer

`VERSION` im Repo-Root ist die einzige Quelle: die Nummer des letzten Releases. Was sie spiegelt
(`package.json` mit Lockfile, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` mit
`Cargo.lock`), setzt nur das Release-Script. Der pre-commit-Hook lehnt einen Commit ab, wenn eine
Kopie abweicht.

## Zwei Branches

- **`main`**: hier landet die Arbeit.
- **`release`**: was veröffentlicht ist. Er bewegt sich nur mit `publish`, nur vorwärts, auf einen
  Release-Commit. Nie von Hand verschieben, nie erzwingen, nie löschen; Tags ebenso.

## Eine Version ist ein Release, benannt nach dem Kalender

- Die Nummer ist `JJ.M.N`: Jahr, Monat, laufende Nummer im Monat, ohne führende Nullen
  (`26.11.1`, `26.11.2`, `26.12.1`), Tag nach Zürcher Zeit. Drei Zahlen passen für
  `package.json`, Cargo und den MSI-Installer (Haupt- und Nebennummer höchstens 255). Der Stand vor
  dem ersten Release ist `0.1.0`.
- **Ein Commit ändert keine Nummer.**
- **Ein Release bündelt**, was seit dem letzten dazukam, und wird auf Wunsch geschnitten:
  1. `node scripts/release.mjs status` zeigt, was wartet, und die Nummer, die es bekäme.
  2. `node scripts/release.mjs` setzt `VERSION` und alle Kopien und macht aus «In Arbeit» in
     `docs/release-notes.md` den Eintrag `## <Nummer> — <Datum>`. Ohne wartende Zeile lehnt es ab.
     Die Dateien sind vorgemerkt, nicht committet.
  3. Allein als «Release <Nummer>» committen und auf `main` pushen.
  4. Installer bauen (`npx tauri build --bundles nsis`).
  5. `node scripts/release.mjs publish` schiebt `release` auf diesen Commit, legt den Tag
     `v<Nummer>` und das GitHub-Release mit den Zeilen des Eintrags an (`gh`) und hängt die
     Installer dieser Nummer an. Nochmals ausgeführt, gleicht es Text und Installer an.
- Die Nummer nie von Hand senken.

## Versionshinweise

`docs/release-notes.md` (deutsch) ist die Geschichte, die Menschen lesen; das Commit-Protokoll
nicht.

- **Jede Änderung schreibt ihre Zeilen unter `## In Arbeit`**, im selben Commit.
- **Ein Eintrag hat eine Form:** ein Satz direkt unter der Überschrift, was das Release bringt,
  dann die Gruppen `### Neu`, `### Verbessert`, `### Behoben`, `### Entfernt`, `### Betrieb`, nur
  die nötigen, in dieser Reihenfolge. Eine Zeile ist `- **Worum es geht:** ein, zwei einfache Sätze.`
- **Einfache Worte:** für Menschen, die die App benutzen. Was man jetzt tun kann oder was nicht
  mehr schiefgeht, wo man es findet, nichts darüber, wie es gebaut ist. Datei-, Funktions- und
  Bibliotheksnamen gehören unter `### Betrieb`.
- Veröffentlichte Einträge folgen, neueste zuerst. Der oberste trägt die Nummer aus `VERSION`.
  `node --test scripts/release.test.mjs` prüft Nummerierung, Schnitt, Veröffentlichen und diese Dateien.

## In der App

- **Version und Commit:** «Über Sesam Remote» und Einstellungen › Info zeigen die Nummer (aus
  `tauri.conf.json`) und den Commit, mit dem gebaut wurde (`git rev-parse`, ohne git aus `.git-commit`).
- **Hooks:** einmal pro Klon `git config core.hooksPath .githooks`. pre-commit schreibt
  `.git-commit` und prüft die Nummer, post-commit schreibt die neue Kennung.

## GitHub

Branch-Schutz (Pflicht-Pull-Request, kein Force-Push) gibt es bei privaten Repos nur mit einem
bezahlten Plan (GitHub Pro oder Team). Ohne ihn gilt die Regel oben als Abmachung.
