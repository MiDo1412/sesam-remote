/** Galerie der Farbschemata mit echter Vorschau (Eingabezeile, farbige «ls»-Ausgabe, Fehler, Auswahl, Cursor, 16 Farben).
 *  Genutzt in den Einstellungen und als Aufklappfeld im Shell-Tab (dort mit Live-Vorschau beim Darüberfahren).
 *  Dazu: Suche, Filter hell/dunkel, eigene Designs (duplizieren, anpassen, importieren, exportieren). */
import type { ITheme } from "@xterm/xterm";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { AUTO_PAIRS, deleteCustomTheme, exportWindowsTerminal, isDark, parseItermcolors, parseWindowsTerminal, resolveTheme, saveCustomTheme, TERM_THEMES } from "./termthemes";
import { ask, tell } from "./ui";

const ANSI: (keyof ITheme)[] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite"];

/** Alle wählbaren Designs: die automatischen Paare zuerst, dann die festen und eigenen. */
export const schemes = (): [string, string][] => [
  ...Object.entries(AUTO_PAIRS).map(([id, p]) => [id, p.label] as [string, string]),
  ...Object.entries(TERM_THEMES).map(([id, x]) => [id, x.label] as [string, string]),
];

function span(text: string, color?: string, bold = false) {
  const s = document.createElement("span");
  s.textContent = text;
  if (color) s.style.color = color;
  if (bold) s.style.fontWeight = "700";
  return s;
}

/** Kleines Terminal-Bild für ein Schema, in seinen Farben und (falls vorgegeben) seiner Schrift. */
export function schemePreview(id: string, theme?: ITheme) {
  const th = theme ?? resolveTheme(id);
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
  const sel = span("notiz.txt");
  sel.style.background = (th.selectionBackground as string) ?? "";
  const cursor = span(" ");
  cursor.className = "scheme-cursor";
  cursor.style.background = (th.cursor as string) ?? (th.foreground as string) ?? "";
  box.append(
    line(span("benutzer@pc", th.green as string, true), span(":"), span("~", th.blue as string, true), span("$ ls")),
    line(span("Bilder", th.blue as string, true), span("  "), span("start.sh", th.green as string, true), span("  "), sel),
    line(span("rm: x: Datei nicht gefunden", th.red as string)),
    line(span("benutzer@pc", th.green as string, true), span(":"), span("~", th.blue as string, true), span("$ "), cursor),
  );
  const dots = document.createElement("div");
  dots.className = "scheme-dots";
  for (const k of ANSI) {
    const c = document.createElement("i");
    c.style.background = (th[k] as string) ?? "";
    dots.append(c);
  }
  return [box, dots] as const;
}

const darkOf = (id: string) => (AUTO_PAIRS[id] ? null : isDark(resolveTheme(id).background));

/**
 * Galerie als Auswahlgruppe mit Werkzeugleiste. `onPick` beim Auswählen (Klick, Enter, Leertaste),
 * `onPreview` beim Darüberfahren/Fokussieren (null = Vorschau beenden).
 */
export function schemeGallery(current: string, onPick: (id: string) => void, onPreview?: (id: string | null) => void) {
  const wrap = document.createElement("div");
  wrap.className = "scheme-wrap";
  const bar = document.createElement("div");
  bar.className = "scheme-tools";
  const search = Object.assign(document.createElement("input"), { type: "search", placeholder: "Design suchen" });
  search.setAttribute("aria-label", "Design suchen");
  const filter = document.createElement("div");
  filter.className = "segmented scheme-filter";
  filter.setAttribute("role", "radiogroup");
  filter.setAttribute("aria-label", "Hell oder dunkel");
  let mode: "all" | "light" | "dark" = "all";
  for (const [v, l] of [["all", "Alle"], ["light", "Hell"], ["dark", "Dunkel"]] as const) {
    const lab = document.createElement("label");
    const r = Object.assign(document.createElement("input"), { type: "radio", name: `scheme-filter-${Math.random()}`, value: v, checked: v === "all" });
    r.onchange = () => ((mode = v), render());
    lab.append(r, l);
    filter.append(lab);
  }
  const newBtn = Object.assign(document.createElement("button"), { type: "button", textContent: "Eigenes …", title: "Gewähltes Design duplizieren und anpassen" });
  const importBtn = Object.assign(document.createElement("button"), { type: "button", textContent: "Importieren …", title: ".itermcolors oder Windows-Terminal-Schema (JSON)" });
  bar.append(search, filter, newBtn, importBtn);

  const grid = document.createElement("div");
  grid.className = "scheme-gallery";
  grid.setAttribute("role", "radiogroup");
  grid.setAttribute("aria-label", "Farbschema");
  let chosen = current;
  let cards: HTMLButtonElement[] = [];

  const render = () => {
    const q = search.value.trim().toLowerCase();
    cards = schemes()
      .filter(([id, label]) => (!q || label.toLowerCase().includes(q)) && (mode === "all" || darkOf(id) === null || darkOf(id) === (mode === "dark")))
      .map(([id, label]) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "scheme-card";
        b.setAttribute("role", "radio");
        b.setAttribute("aria-checked", String(id === chosen));
        b.dataset.id = id;
        b.tabIndex = id === chosen ? 0 : -1;
        const name = document.createElement("span");
        name.className = "scheme-name";
        name.textContent = label;
        const [shot, dots] = schemePreview(id);
        b.append(shot, name, dots);
        if (TERM_THEMES[id]?.font) shot.style.fontFamily = `${TERM_THEMES[id].font}, ${getComputedStyle(shot).fontFamily}`;
        b.onclick = () => {
          chosen = id;
          for (const c of cards) (c.setAttribute("aria-checked", String(c === b)), (c.tabIndex = c === b ? 0 : -1));
          onPick(id);
        };
        b.oncontextmenu = (e) => (e.preventDefault(), void customMenu(id));
        b.onmouseenter = b.onfocus = () => onPreview?.(id);
        return b;
      });
    if (cards.length && !cards.some((c) => c.tabIndex === 0)) cards[0].tabIndex = 0;
    grid.replaceChildren(...cards);
    if (!cards.length) grid.append(Object.assign(document.createElement("p"), { className: "note", textContent: "Kein Design passt zur Suche." }));
  };
  search.oninput = render;
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

  // Eigenes Design: vom gewählten (bzw. bei «automatisch» vom gerade sichtbaren) ausgehen.
  newBtn.onclick = async () => {
    const base = resolveTheme(chosen); // bei «automatisch» das gerade sichtbare
    const label = `${schemes().find(([id]) => id === chosen)?.[1] ?? "Design"} (eigen)`;
    const id = await editTheme(`custom:${crypto.randomUUID().slice(0, 8)}`, label, { ...base });
    if (id) ((chosen = id), render(), onPick(id));
  };
  importBtn.onclick = () => {
    const file = Object.assign(document.createElement("input"), { type: "file", accept: ".itermcolors,.json,application/json" });
    file.onchange = async () => {
      const f = file.files?.[0];
      if (!f) return;
      try {
        const text = await f.text();
        const { name, theme } = f.name.endsWith(".itermcolors") ? { name: f.name.replace(/\.itermcolors$/, ""), theme: parseItermcolors(text) } : parseWindowsTerminal(text);
        const id = `custom:${crypto.randomUUID().slice(0, 8)}`;
        saveCustomTheme(id, name, theme);
        chosen = id;
        render();
        onPick(id);
      } catch (e) {
        tell("Nicht importiert", String(e instanceof Error ? e.message : e));
      }
    };
    file.click();
  };

  /** Rechtsklick auf ein eigenes Design: anpassen, umbenennen, exportieren, löschen. */
  async function customMenu(id: string) {
    const { openMenu } = await import("./menu");
    const custom = id.startsWith("custom:");
    const card = cards.find((c) => c.dataset.id === id);
    if (!card) return;
    const v = await openMenu(card, [
      { value: "dup", label: "Duplizieren und anpassen …" },
      ...(custom ? [{ value: "edit", label: "Anpassen …" }, { value: "del", label: "Löschen", separator: true }] : []),
      { value: "export", label: "Exportieren …", separator: !custom },
    ]);
    const t = resolveTheme(id);
    const label = TERM_THEMES[id]?.label ?? AUTO_PAIRS[id]?.label ?? id;
    if (v === "dup") {
      const nid = await editTheme(`custom:${crypto.randomUUID().slice(0, 8)}`, `${label} (eigen)`, { ...t });
      if (nid) ((chosen = nid), render(), onPick(nid));
    } else if (v === "edit" && (await editTheme(id, label, { ...t }))) render();
    else if (v === "del" && (await ask(`«${label}» löschen?`, "Shells mit diesem Design nehmen danach «Automatisch».", "Löschen"))) (deleteCustomTheme(id), render());
    else if (v === "export") {
      const path = await saveDialog({ defaultPath: `${label}.json`, filters: [{ name: "Windows-Terminal-Schema", extensions: ["json"] }] });
      if (path) await invoke("save_text_file", { path, text: exportWindowsTerminal(label, t) }).catch((e) => tell("Nicht exportiert", String(e)));
    }
  }

  render();
  // Automatische Designs zeigen nach einem Wechsel hell/dunkel die passende Hälfte.
  const scheme = matchMedia("(prefers-color-scheme: dark)");
  const onScheme = () => (wrap.isConnected ? render() : scheme.removeEventListener("change", onScheme));
  scheme.addEventListener("change", onScheme);
  wrap.append(bar, grid);
  return wrap;
}

/** Dialog: Name und Farben (Hintergrund, Vordergrund, Cursor, Auswahl, 16 Farben) mit Vorschau.
 *  Jede Änderung wirkt sofort auf offene Shells mit diesem Design. Gibt die ID zurück oder null. */
async function editTheme(id: string, label: string, theme: ITheme): Promise<string | null> {
  const isNew = !TERM_THEMES[id];
  const original = TERM_THEMES[id] ? { ...TERM_THEMES[id].theme } : null;
  const dlg = document.createElement("dialog");
  dlg.className = "theme-edit";
  dlg.setAttribute("aria-label", "Design anpassen");
  const form = document.createElement("form");
  form.className = "sheet";
  form.method = "dialog";
  const h = Object.assign(document.createElement("h2"), { textContent: isNew ? "Eigenes Design" : "Design anpassen" });
  const nameLab = document.createElement("label");
  nameLab.textContent = "Name";
  const name = Object.assign(document.createElement("input"), { value: label, required: true });
  nameLab.append(name);
  const preview = document.createElement("div");
  preview.className = "theme-edit-preview";
  const fields = document.createElement("div");
  fields.className = "theme-edit-colors";
  const keys: [keyof ITheme, string][] = [
    ["background", "Hintergrund"], ["foreground", "Vordergrund"], ["cursor", "Cursor"], ["selectionBackground", "Auswahl"],
    ["black", "Schwarz"], ["red", "Rot"], ["green", "Grün"], ["yellow", "Gelb"], ["blue", "Blau"], ["magenta", "Magenta"], ["cyan", "Cyan"], ["white", "Weiss"],
    ["brightBlack", "Hell Schwarz"], ["brightRed", "Hell Rot"], ["brightGreen", "Hell Grün"], ["brightYellow", "Hell Gelb"], ["brightBlue", "Hell Blau"], ["brightMagenta", "Hell Magenta"], ["brightCyan", "Hell Cyan"], ["brightWhite", "Hell Weiss"],
  ];
  const draw = () => preview.replaceChildren(...schemePreview(id, theme));
  for (const [k, l] of keys) {
    const lab = document.createElement("label");
    const v = String(theme[k] ?? "#000000").slice(0, 7);
    const inp = Object.assign(document.createElement("input"), { type: "color", value: /^#[0-9a-f]{6}$/i.test(v) ? v : "#000000" });
    inp.oninput = () => {
      (theme as Record<string, string>)[k as string] = inp.value;
      draw();
      if (!isNew) saveCustomTheme(id, name.value.trim() || label, theme); // live auf offene Shells
    };
    lab.append(inp, l);
    fields.append(lab);
  }
  draw();
  const actions = document.createElement("div");
  actions.className = "row actions";
  const cancel = Object.assign(document.createElement("button"), { type: "button", textContent: "Abbrechen" });
  const ok = Object.assign(document.createElement("button"), { type: "submit", className: "primary", textContent: "Sichern" });
  actions.append(document.createElement("span"), cancel, ok);
  form.append(h, nameLab, preview, fields, actions);
  dlg.append(form);
  document.body.append(dlg);
  const saved = await new Promise<boolean>((resolve) => {
    cancel.onclick = () => dlg.close();
    form.onsubmit = (e) => (e.preventDefault(), resolve(true), dlg.close());
    dlg.onclose = () => resolve(false);
    dlg.showModal();
  });
  dlg.remove();
  if (!saved) {
    if (original) saveCustomTheme(id, label, original); // Live-Änderungen zurücknehmen
    return null;
  }
  saveCustomTheme(id, name.value.trim() || label, theme);
  return id;
}
