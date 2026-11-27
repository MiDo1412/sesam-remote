// Zeilenweiser Vergleich für den eingebauten Editor: gleicher Anfang und gleiches Ende fallen weg, der Rest über die
// längste gemeinsame Teilfolge. Reicht für Konfigurationsdateien; sehr grosse Änderungen zeigt er als Block.

export type DiffLine = { op: " " | "+" | "-"; text: string };

/** Mittelteil grösser als das (Zeilen × Zeilen): ganz alt raus, ganz neu rein. */
const MAX_CELLS = 4_000_000;

export function lineDiff(before: string, after: string): DiffLine[] {
  const a = before.split("\n");
  const b = after.split("\n");
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) (endA--, endB--);
  const head = a.slice(0, start).map((text) => ({ op: " " as const, text }));
  const tail = a.slice(endA).map((text) => ({ op: " " as const, text }));
  const x = a.slice(start, endA);
  const y = b.slice(start, endB);
  if (x.length * y.length > MAX_CELLS)
    return [...head, ...x.map((text) => ({ op: "-" as const, text })), ...y.map((text) => ({ op: "+" as const, text })), ...tail];
  // lcs[i][j] = Länge der gemeinsamen Teilfolge von x[i..] und y[j..]
  const w = y.length + 1;
  const lcs = new Uint32Array((x.length + 1) * w);
  for (let i = x.length - 1; i >= 0; i--)
    for (let j = y.length - 1; j >= 0; j--) lcs[i * w + j] = x[i] === y[j] ? lcs[(i + 1) * w + j + 1] + 1 : Math.max(lcs[(i + 1) * w + j], lcs[i * w + j + 1]);
  const mid: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < x.length && j < y.length) {
    if (x[i] === y[j]) (mid.push({ op: " ", text: x[i] }), i++, j++);
    else if (lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]) mid.push({ op: "-", text: x[i++] });
    else mid.push({ op: "+", text: y[j++] });
  }
  while (i < x.length) mid.push({ op: "-", text: x[i++] });
  while (j < y.length) mid.push({ op: "+", text: y[j++] });
  return [...head, ...mid, ...tail];
}

/** Nur die Änderungen mit `context` Zeilen drumherum; Lücken als `null`. */
export function hunks(d: DiffLine[], context = 3): (DiffLine | null)[] {
  const keep = d.map(() => false);
  d.forEach((l, k) => {
    if (l.op !== " ") for (let m = Math.max(0, k - context); m <= Math.min(d.length - 1, k + context); m++) keep[m] = true;
  });
  const out: (DiffLine | null)[] = [];
  d.forEach((l, k) => {
    if (keep[k]) out.push(l);
    else if (out[out.length - 1] !== null) out.push(null);
  });
  return out;
}
