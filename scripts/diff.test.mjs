// Zeilenvergleich des eingebauten Editors (src/files/diff.ts), läuft direkt mit Node (Typen werden entfernt).
import test from "node:test";
import assert from "node:assert/strict";
import { hunks, lineDiff } from "../src/files/diff.ts";

const show = (d) => d.map((l) => (l ? l.op + l.text : "…")).join("\n");

test("Änderung, Einfügen und Löschen", () => {
  const a = "port 22\nuser root\n# alt\nlog info";
  const b = "port 2222\nuser root\nlog info\nneu";
  assert.equal(show(lineDiff(a, b)), "-port 22\n+port 2222\n user root\n-# alt\n log info\n+neu");
});

test("gleich: keine Änderung, Ausschnitte nur um Änderungen", () => {
  assert.ok(lineDiff("a\nb", "a\nb").every((l) => l.op === " "));
  const a = Array.from({ length: 20 }, (_, i) => `z${i}`).join("\n");
  const b = a.replace("z10", "Z10");
  assert.equal(show(hunks(lineDiff(a, b), 1)), "…\n z9\n-z10\n+Z10\n z11\n…");
});
