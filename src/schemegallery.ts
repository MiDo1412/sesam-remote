/** Galerie der Farbschemata mit echter Vorschau (Eingabezeile, farbige «ls»-Ausgabe, Farbleiste).
 *  Genutzt in den Einstellungen und als Aufklappfeld im Shell-Tab (dort mit Live-Vorschau beim Darüberfahren). */
import type { ITheme } from "@xterm/xterm";
import { resolveTheme, TERM_THEMES } from "./termthemes";

const ANSI: (keyof ITheme)[] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite"];

export const SCHEMES: [string, string][] = [["auto", "Automatisch (wie System)"], ...Object.entries(TERM_THEMES).map(([id, x]) => [id, x.label] as [string, string])];

function span(text: string, color?: string, bold = false) {
  const s = document.createElement("span");
  s.textContent = text;
  if (color) s.style.color = color;
  if (bold) s.style.fontWeight = "700";
  return s;
}

/** Kleines Terminal-Bild für ein Schema. */
export function schemePreview(id: string) {
  const th = resolveTheme(id);
  const box = document.createElement("div");
  box.className = "scheme-shot";
  box.style.background = th.background ?? "";
  box.style.color = th.foreground ?? "";
  box.setAttribute("aria-hidden", "true");
  const line = (...parts: HTMLElement[]) => {
    const d = document.createElement("div");
    d.append(...parts);
    return d;
  };
  box.append(
    line(span("benutzer@pc", th.green as string, true), span(":"), span("~", th.blue as string, true), span("$ ls")),
    line(span("Bilder", th.blue as string, true), span("  "), span("start.sh", th.green as string, true), span("  notiz.txt")),
    line(span("Fehler", th.red as string), span(" · "), span("Warnung", th.yellow as string), span(" · "), span("Info", th.cyan as string)),
  );
  const bar = document.createElement("div");
  bar.className = "scheme-bar";
  for (const k of ANSI.slice(0, 8)) {
    const c = document.createElement("i");
    c.style.background = (th[k] as string) ?? "";
    bar.append(c);
  }
  box.append(bar);
  return box;
}

/**
 * Galerie als Auswahlgruppe. `onPick` beim Auswählen (Klick, Enter, Leertaste),
 * `onPreview` beim Darüberfahren/Fokussieren (null = Vorschau beenden).
 */
export function schemeGallery(current: string, onPick: (id: string) => void, onPreview?: (id: string | null) => void) {
  const grid = document.createElement("div");
  grid.className = "scheme-gallery";
  grid.setAttribute("role", "radiogroup");
  grid.setAttribute("aria-label", "Farbschema");
  const cards = SCHEMES.map(([id, label]) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "scheme-card";
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(id === current));
    b.tabIndex = id === current ? 0 : -1;
    const name = document.createElement("span");
    name.className = "scheme-name";
    name.textContent = label;
    b.append(schemePreview(id), name);
    b.onclick = () => {
      for (const c of cards) (c.setAttribute("aria-checked", String(c === b)), (c.tabIndex = c === b ? 0 : -1));
      onPick(id);
    };
    b.onmouseenter = b.onfocus = () => onPreview?.(id);
    return b;
  });
  grid.onmouseleave = () => onPreview?.(null);
  // Pfeiltasten wie in einer Auswahlgruppe
  grid.onkeydown = (e) => {
    const i = cards.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0) return;
    const cols = getComputedStyle(grid).gridTemplateColumns.split(" ").length || 1;
    const step = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: cols, ArrowUp: -cols }[e.key];
    if (!step) return;
    e.preventDefault();
    cards[Math.max(0, Math.min(cards.length - 1, i + step))].focus();
  };
  grid.append(...cards);
  return grid;
}
