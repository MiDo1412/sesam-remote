// Kontrast der mitgelieferten Shell-Designs nach WCAG 2.2: node --test scripts/termthemes.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

globalThis.addEventListener ??= () => {};
const { TERM_THEMES, AUTO_PAIRS } = await import("../src/termthemes.ts");

const lum = (h) => {
  const n = parseInt(h.slice(1, 7), 16);
  const l = (c) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * l(n >> 16) + 0.7152 * l((n >> 8) & 255) + 0.0722 * l(n & 255);
};
const ratio = (a, b) => (Math.max(lum(a), lum(b)) + 0.05) / (Math.min(lum(a), lum(b)) + 0.05);
const COLORS = ["red", "green", "yellow", "blue", "magenta", "cyan", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan"];

// Solarized Hell liegt im Original bei 4,1:1; die Farbwerte bekannter Schemata bleiben unverändert.
const ORIGINAL_LOW = new Set(["solarized-light"]);

test("Text aller Designs mindestens 4,5:1 (WCAG AA)", () => {
  for (const [id, { theme: t }] of Object.entries(TERM_THEMES)) if (!ORIGINAL_LOW.has(id)) assert.ok(ratio(t.foreground, t.background) >= 4.5, `${id}: ${ratio(t.foreground, t.background).toFixed(2)}`);
});

test("Sesam-Designs: Text 7:1, Farben 4,5:1, Grau 3:1, Text auf Auswahl 4,5:1", () => {
  const own = Object.entries(TERM_THEMES).filter(([id]) => /^sesam-.+-(dark|light)$/.test(id));
  assert.ok(own.length >= 8);
  for (const [id, { theme: t }] of own) {
    assert.ok(ratio(t.foreground, t.background) >= 7, id);
    assert.ok(ratio(t.foreground, t.selectionBackground) >= 4.5, `${id} Auswahl`);
    assert.ok(ratio(t.brightBlack, t.background) >= 3, `${id} brightBlack`);
    for (const c of COLORS) assert.ok(ratio(t[c], t.background) >= 4.5, `${id} ${c}: ${ratio(t[c], t.background).toFixed(2)}`);
  }
});

test("Automatische Paare zeigen auf vorhandene Designs", () => {
  for (const [id, p] of Object.entries(AUTO_PAIRS)) assert.ok(TERM_THEMES[p.light] && TERM_THEMES[p.dark], id);
});
