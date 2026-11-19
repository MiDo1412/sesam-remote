/** Sprache der Oberfläche: Deutsch (Quelle) oder Englisch. Der Code schreibt deutsche Texte; auf Englisch übersetzt
 *  diese Schicht, was im DOM erscheint (Text, title, aria-label, placeholder), über das Wörterbuch src/i18n/en.json:
 *  feste Texte und Vorlagen mit {} (bzw. {0}, {1} bei anderer Reihenfolge) für eingesetzte Werte. Fehlt ein Text,
 *  bleibt er deutsch. Nutzerdaten (Namen, Pfade, Terminal) stehen unter [translate=no] und bleiben unberührt.
 *  Neue Texte: node scripts/i18n.mjs check zeigt, was im Wörterbuch fehlt. */
import EN from "./i18n/en.json";

export type Lang = "de" | "en";
export type LangPref = "auto" | Lang;
const KEY = "sesam.lang";

export function langPref(): LangPref {
  try {
    const v = localStorage.getItem(KEY);
    return v === "de" || v === "en" ? v : "auto";
  } catch {
    return "auto";
  }
}
export function setLangPref(v: LangPref) {
  try {
    localStorage.setItem(KEY, v);
  } catch {}
  location.reload();
}
/** «Automatisch»: Deutsch, wenn das System Deutsch spricht, sonst Englisch. */
export const lang: Lang = langPref() === "auto" ? (/^de\b/i.test(navigator.language) ? "de" : "en") : (langPref() as Lang);

const norm = (s: string) => s.replace(/\s+/g, " ").trim();
const exact = new Map<string, string>();
const patterns: [RegExp, string[]][] = [];
for (const [de, en] of Object.entries(EN as Record<string, string>)) {
  if (!de.includes("{}")) exact.set(norm(de), en);
  else {
    const parts = norm(de).split("{}");
    const re = new RegExp(`^${parts.map((p) => p.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("([\\s\\S]+?)")}$`);
    // Englische Vorlage in Stücke: Text, dann Index des eingesetzten Werts ({} der Reihe nach oder {n})
    let i = 0;
    const out = en.split(/(\{\d*\})/).map((p) => (/^\{\d*\}$/.test(p) ? String(p === "{}" ? i++ : Number(p.slice(1, -1))) + "\u0000" : p));
    patterns.push([re, out]);
  }
}
patterns.sort((a, b) => b[0].source.length - a[0].source.length); // genauere Vorlagen zuerst

/** Ursprünglicher (deutscher) Text eines Elements, auch wenn es übersetzt angezeigt wird. */
export const deText = (el: HTMLElement) => el.dataset.de ?? el.textContent!.trim();

/** Einen Text übersetzen (auf Deutsch unverändert). Für Stellen ausserhalb des DOM, z. B. Dateidialoge des Systems. */
export function tr(s: string): string {
  if (lang === "de" || !s) return s;
  const key = norm(s);
  if (!key || !/[A-Za-zÄÖÜäöü]/.test(key)) return s;
  const hit = exact.get(key) ?? fromPattern(key);
  if (hit !== undefined) return s.match(/^\s*/)![0] + hit + s.match(/\s*$/)![0];
  // Zusammengesetzte Meldungen («Keine Verbindung: …», mehrere Zeilen): Stück für Stück
  for (const sep of ["\n", ": ", " – ", " · "]) {
    const i = s.indexOf(sep);
    if (i > 0 && i < s.length - sep.length) {
      const a = tr(s.slice(0, i)), b = tr(s.slice(i + sep.length));
      if (a !== s.slice(0, i) || b !== s.slice(i + sep.length)) return a + sep + b;
    }
  }
  return s;
}
function fromPattern(key: string): string | undefined {
  for (const [re, out] of patterns) {
    const m = re.exec(key);
    if (m) return out.map((p) => (p.endsWith("\u0000") ? tr(m[Number(p.slice(0, -1)) + 1] ?? "") : p)).join("");
  }
}

const ATTRS = ["title", "aria-label", "placeholder", "alt"];
// Nutzerdaten: Terminal, Namen und Adressen von Verbindungen, Datei- und Ordnernamen, Pfade
const SKIP = "[translate=no], .xterm, .terminal, script, style, code, pre, textarea, [contenteditable], .card-name, .card-sub, td.name, .mono, .detail-title, .crumb, .tab-label";
function translateNode(n: Node) {
  if (n.nodeType === Node.TEXT_NODE) {
    const p = n.parentElement;
    if (!p || p.closest(SKIP)) return;
    const t = tr(n.nodeValue ?? "");
    if (t === n.nodeValue) return;
    // Code, der Beschriftungen als Schlüssel liest, findet das Original in data-de (deText)
    if (p.childNodes.length === 1 && p.dataset.de === undefined) p.dataset.de = n.nodeValue!.trim();
    n.nodeValue = t;
  } else if (n instanceof Element) {
    if (n.closest(SKIP) && !n.matches("textarea, input")) return;
    for (const a of ATTRS) {
      const v = n.getAttribute(a);
      if (v) {
        const t = tr(v);
        if (t !== v) n.setAttribute(a, t);
      }
    }
    if (n.matches("textarea, input")) return;
    for (const c of n.childNodes) translateNode(c);
  }
}

if (lang === "en") {
  document.documentElement.lang = "en";
  // Menüleiste der Desktop-Apps (in Rust gebaut): dieselben Texte, ohne Vorlagen
  import("@tauri-apps/api/core").then(({ invoke }) => invoke("relabel_menu", { map: Object.fromEntries(exact) })).catch(() => {});
  const run = () => {
    translateNode(document.body);
    new MutationObserver((list) => {
      for (const m of list) {
        if (m.type === "characterData") translateNode(m.target);
        else if (m.type === "attributes") translateNode(m.target);
        else m.addedNodes.forEach(translateNode);
      }
    }).observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ATTRS });
  };
  if (document.body) run();
  else addEventListener("DOMContentLoaded", run);
}
