#!/usr/bin/env node
/** Wörterbuch der Oberfläche (src/i18n/en.json) prüfen.
 *
 *   node scripts/i18n.mjs check     deutsche Texte im Code ohne englische Übersetzung (Exit 1, wenn welche fehlen)
 *   node scripts/i18n.mjs missing   dasselbe als JSON-Liste, zum Übersetzen
 *
 * Gesammelt wird wie folgt: */
// Texte für die Übersetzung: TS-Strings und Vorlagen (über den TS-Parser), Text in index.html und in HTML-Strings,
// deutsche Meldungen aus Rust (format!-Vorlagen mit {} als Platzhalter). Ausgabe: { exact: [...], tpl: [...] }
import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL("..", import.meta.url));
const ts = createRequire(root + "/package.json")("typescript");
const exact = new Set(), tpl = new Set();
const DE = /[äöüÄÖÜß«»]|\b(der|die|das|und|nicht|ist|mit|für|oder|ein|eine|einen|auf|zu|von|im|den|dem|des|du|dein|deine|wird|werden|kann|noch|kein|keine|alle|neu|neue|schliessen|öffnen|abbrechen|sichern|löschen|zurück|weiter|fertig|fehler|passwort|benutzer|gruppe|ordner|kopieren|einfügen|suchen|teilen|trennen|verbinden|verbindung|sitzung|datei|dateien|einstellungen|anmeldung|schlüssel|hier|jetzt|immer|nie|aus|ein|an|bei|nach|vor|über|unter|ohne|bis|wie|wenn|dann|auch|nur|schon|mehr|bitte)\b/i;
// Auch einzelne Wörter und kurze Beschriftungen («Adresse», «Bearbeiten»): gross geschrieben, nur Buchstaben und Leerzeichen
const LABEL = /^[A-ZÄÖÜ][a-zäöüß]+(?:[ -][A-Za-zÄÖÜäöüß]+){0,4}( …)?$/;
const isText = (s) => { const t = s.trim(); return t.length > 1 && /[A-Za-zÄÖÜäöü]{2}/.test(t) && (DE.test(t) || LABEL.test(t) || LABEL.test(t.replace(/[^A-Za-zÄÖÜäöüß ]+/g, " ").replace(/\s+/g, " ").trim())) && !/^(\.|#|\[|\/|[a-z-]+=|data-|aria-|https?:|var\(|calc\()/.test(t) && !/^[a-z][\w-]*$/.test(t) && !/[{};]\s*$/.test(t) && !/^\s*[\w.-]+\s*\(/.test(t); };
const SUB = "\u0000";
// HTML-Fragment in Textstücke zerlegen (Text zwischen Tags, dazu title/aria-label/placeholder-Werte)
function fragments(s) {
  if (!/<[a-z!/]/i.test(s)) return [s];
  const out = [];
  for (const m of s.matchAll(/(?:title|aria-label|placeholder|alt|data-tip)="([^"]*)"/g)) out.push(m[1]);
  for (const part of s.replace(/<[^>]*>/g, "\u0001").split("\u0001")) out.push(part);
  return out;
}
function add(s) { for (const f of fragments(s)) { const t = f.replace(/\s+/g, " ").trim(); if (!isText(t.replaceAll(SUB, "x"))) continue; (t.includes(SUB) ? tpl : exact).add(t.replaceAll(SUB, "{}")); } }
for (const f of readdirSync(root + "/src").filter((f) => f.endsWith(".ts"))) {
  const sf = ts.createSourceFile(f, readFileSync(`${root}/src/${f}`, "utf8"), ts.ScriptTarget.Latest, true);
  const visit = (n) => {
    if (ts.isImportDeclaration(n) || ts.isExportDeclaration(n)) return;
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) add(n.text);
    else if (ts.isTemplateExpression(n)) { add(n.head.text + n.templateSpans.map((sp) => SUB + sp.literal.text).join("")); n.templateSpans.forEach((sp) => visit(sp.expression)); return; }
    ts.forEachChild(n, visit);
  };
  visit(sf);
}
add(readFileSync(root + "/index.html", "utf8").replace(/<script[\s\S]*?<\/script>|<style[\s\S]*?<\/style>/g, ""));
for (const f of readdirSync(root + "/src-tauri/src", { recursive: true }).filter((f) => String(f).endsWith(".rs"))) {
  const src = readFileSync(`${root}/src-tauri/src/${f}`, "utf8").replace(/(^|\s)\/\/.*$/gm, "$1").replace(/#\[cfg\(test\)\][\s\S]*$/m, "");
  // Menüleiste (macOS, Windows, Linux): Beschriftungen in SubmenuBuilder::new / .text / with_id
  for (const m of src.matchAll(/(?:SubmenuBuilder::new\(\s*app,\s*|\.text\(\s*"[^"]*",\s*|with_id\(\s*"[^"]*",\s*)"([^"]+)"/g)) if (LABEL.test(m[1].trim()) || isText(m[1])) exact.add(m[1].trim());
  for (const m of src.matchAll(/"((?:[^"\\]|\\.)*)"/g)) { const s = m[1].replace(/\\"/g, '"').replace(/\\n/g, " "); if (/^[a-z][\w-]*:\S/.test(s)) continue; if (!/[äöüÄÖÜß«]/.test(s) && !(DE.test(s) && /\s/.test(s))) continue; add(s.replace(/\{[^{}]*\}/g, SUB)); }
}
const en = JSON.parse(readFileSync(root + "/src/i18n/en.json", "utf8"));
const missing = [...exact, ...tpl].filter((s) => !(s in en)).sort();
if (process.argv[2] === "missing") console.log(JSON.stringify(missing, null, 1));
else {
  console.log(`${exact.size + tpl.size} Texte, ${missing.length} ohne Übersetzung`);
  for (const s of missing) console.log("  " + s);
  process.exitCode = missing.length ? 1 : 0;
}
