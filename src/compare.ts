/** Ordnervergleich lokal ↔ Server: reine Logik, ohne Oberfläche. */

type Item = { name: string; dir: boolean; size: number; modified: number | null };

/** Aus Sicht einer Seite: nur hier, neuer hier, neuer dort, gleich, verschieden (ohne klare Richtung). */
export type Mark = "only" | "newer" | "older" | "same" | "differ";

export const MARK_LABEL: Record<Mark, string> = { only: "nur hier", newer: "neuer hier", older: "neuer dort", same: "gleich", differ: "verschieden" };

/**
 * Markierung je Name aus Sicht von `here`. Gleich = gleiche Grösse und Änderungszeit (± `tol` Sekunden).
 * Ordner auf beiden Seiten bleiben ohne Markierung (der Vergleich geht nicht in die Tiefe).
 * `content(name)`: Ergebnis von «Inhalt prüfen» (true = gleicher Inhalt), sonst undefined.
 */
export function compareDirs(here: Item[], there: Item[], tol = 2, content: (name: string) => boolean | undefined = () => undefined) {
  // shortcut: Namen exakt vergleichen, auch gegen ein Windows-Laufwerk ohne Gross/Klein-Unterschied
  const other = new Map(there.map((e) => [e.name, e]));
  const out = new Map<string, Mark>();
  for (const a of here) {
    const b = other.get(a.name);
    if (!b) out.set(a.name, "only");
    else if (a.dir !== b.dir) out.set(a.name, "differ");
    else if (!a.dir) out.set(a.name, fileMark(a, b, tol, content(a.name)));
  }
  return out;
}

function fileMark(a: Item, b: Item, tol: number, same: boolean | undefined): Mark {
  if (same === true) return "same";
  const dt = a.modified != null && b.modified != null ? a.modified - b.modified : 0;
  if (dt > tol) return "newer";
  if (dt < -tol) return "older";
  return a.size === b.size && same !== false ? "same" : "differ";
}

// Selbstprüfung im Entwicklungsbuild
if (import.meta.env?.DEV) {
  const f = (name: string, size: number, modified: number | null, dir = false) => ({ name, dir, size, modified });
  const local = [f("a", 1, 100), f("b", 1, 100), f("c", 1, 110), f("d", 1, 100), f("e", 2, 101), f("o", 1, 1), f("x", 0, null, true), f("y", 0, null, true), f("z", 1, 1)];
  const remote = [f("a", 1, 102), f("b", 1, 103), f("c", 1, 100), f("d", 1, 120), f("e", 1, 100), f("x", 0, null, true), f("z", 0, null, true), f("nur", 1, 1)];
  const m = compareDirs(local, remote);
  const want: Record<string, Mark | undefined> = { a: "same", b: "older", c: "newer", d: "older", e: "differ", o: "only", x: undefined, y: "only", z: "differ" };
  for (const [k, v] of Object.entries(want)) console.assert(m.get(k) === v, `compareDirs ${k}: ${m.get(k)} statt ${v}`);
  const r = compareDirs(remote, local);
  console.assert(r.get("b") === "newer" && r.get("nur") === "only" && r.get("a") === "same", "compareDirs Gegenseite");
  const h = compareDirs(local, remote, 2, (n) => (n === "c" ? true : n === "a" ? false : undefined));
  console.assert(h.get("c") === "same" && h.get("a") === "differ", "compareDirs Inhalt");
}
