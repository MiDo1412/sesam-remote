/** Gruppen bearbeiten: anlegen, umbenennen, löschen, Reihenfolge, Farbe. Seitenleiste und Einstellungen › Gruppen. */
import { api, type Group } from "./api";
import { ask, askText, tell } from "./ui";
import { openMenu } from "./menu";

/** Vorgaben (Schlüssel wie in library.rs GROUP_COLORS). */
export const GROUP_COLORS: Record<string, { label: string; css: string }> = {
  "": { label: "Keine", css: "" },
  red: { label: "Rot", css: "#e5484d" },
  orange: { label: "Orange", css: "#f76b15" },
  yellow: { label: "Gelb", css: "#e2a336" },
  green: { label: "Grün", css: "#30a46c" },
  blue: { label: "Blau", css: "#3e63dd" },
  purple: { label: "Violett", css: "#8e4ec6" },
  gray: { label: "Grau", css: "#8b8d98" },
};

/** Farbpunkt einer Gruppe (leer = kein Punkt). */
export function groupDot(color: string) {
  const s = document.createElement("span");
  s.className = "group-dot";
  s.setAttribute("aria-hidden", "true");
  s.style.background = GROUP_COLORS[color]?.css ?? "";
  s.hidden = !GROUP_COLORS[color]?.css;
  return s;
}

const dotSvg = (css: string) =>
  `<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">${css ? `<circle cx="8" cy="8" r="5" fill="${css}"/>` : `<circle cx="8" cy="8" r="4.5" fill="none" stroke="currentColor" stroke-opacity=".5"/>`}</svg>`;

async function attempt(f: () => Promise<unknown>, fail: string) {
  try {
    await f();
    return true;
  } catch (e) {
    await tell(fail, String(e));
    return false;
  }
}

export async function newGroup(): Promise<string | null> {
  const name = await askText("Neue Gruppe", "Name der Gruppe. Verbindungen ziehst du danach in der Seitenleiste auf die Gruppe.", "Anlegen");
  if (!name) return null;
  const list = await api.groupsList();
  if (list.some((g) => g.name === name)) {
    await tell("Gruppe gibt es schon", `«${name}» steht bereits in der Liste.`);
    return null;
  }
  return (await attempt(() => api.groupsSave([...list, { name, color: "" }]), "Gruppe nicht angelegt")) ? name : null;
}

/** Gibt den neuen Namen zurück oder null. */
export async function renameGroup(name: string): Promise<string | null> {
  const to = await askText("Gruppe umbenennen", `Alle Verbindungen in «${name}» erhalten den neuen Namen.`, "Umbenennen", name);
  if (!to || to === name) return null;
  const list = await api.groupsList();
  if (list.some((g) => g.name === to) && !(await ask("Gruppen zusammenführen?", `«${to}» gibt es schon. Die Verbindungen aus «${name}» kommen dazu.`, "Zusammenführen"))) return null;
  return (await attempt(() => api.groupRename(name, to), "Nicht umbenannt")) ? to : null;
}

export async function deleteGroup(name: string, count: number) {
  const text = count ? `${count} Verbindung(en) kommen nach «Ohne Gruppe». Gelöscht wird keine Verbindung.` : "Die leere Gruppe wird entfernt.";
  if (!(await ask(`Gruppe «${name}» löschen?`, text, "Löschen"))) return false;
  return attempt(() => api.groupDelete(name), "Nicht gelöscht");
}

/** Gruppe `name` vor `before` setzen (null = ans Ende). */
export async function moveGroupBefore(name: string, before: string | null) {
  const list = await api.groupsList();
  const g = list.find((x) => x.name === name);
  if (!g || name === before) return false;
  const rest = list.filter((x) => x !== g);
  const i = before === null ? rest.length : rest.findIndex((x) => x.name === before);
  rest.splice(i < 0 ? rest.length : i, 0, g);
  return attempt(() => api.groupsSave(rest), "Nicht verschoben");
}

/** Um eine Stelle nach oben (-1) oder unten (+1). */
export async function moveGroup(name: string, delta: -1 | 1) {
  const list = await api.groupsList();
  const i = list.findIndex((x) => x.name === name);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= list.length) return false;
  [list[i], list[j]] = [list[j], list[i]];
  return attempt(() => api.groupsSave(list), "Nicht verschoben");
}

export async function pickGroupColor(anchor: HTMLElement, name: string) {
  const list = await api.groupsList();
  const g = list.find((x) => x.name === name);
  if (!g) return false;
  const v = await openMenu(
    anchor,
    Object.entries(GROUP_COLORS).map(([value, c]) => ({ value, label: c.label, icon: dotSvg(c.css) })),
    { checked: g.color, listbox: true },
  );
  if (v === null || v === g.color) return false;
  g.color = v;
  return attempt(() => api.groupsSave(list), "Farbe nicht gesichert");
}

/** Kontextmenü einer Gruppe. Ergebnis: geänderter Name (nach Umbenennen/Anlegen), "" = sonst geändert, null = nichts. */
export async function groupMenu(anchor: HTMLElement, g: Group | null, count: number, index: number, total: number): Promise<string | null> {
  const v = await openMenu(anchor, [
    ...(g
      ? [
          { value: "rename", label: "Umbenennen …" },
          { value: "color", label: "Farbe …" },
          { value: "up", label: "Nach oben", disabled: index <= 0 },
          { value: "down", label: "Nach unten", disabled: index >= total - 1 },
        ]
      : []),
    { value: "new", label: "Neue Gruppe …", separator: !!g },
    ...(g ? [{ value: "delete", label: "Gruppe löschen …", separator: true }] : []),
  ]);
  if (v === "new") return newGroup();
  if (!g) return null;
  if (v === "rename") return renameGroup(g.name);
  const ok =
    v === "color" ? await pickGroupColor(anchor, g.name)
    : v === "up" ? await moveGroup(g.name, -1)
    : v === "down" ? await moveGroup(g.name, 1)
    : v === "delete" ? await deleteGroup(g.name, count)
    : false;
  return ok ? "" : null;
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const changed = (detail: { from?: string; to?: string } = {}) => dispatchEvent(new CustomEvent("sesam:groups-changed", { detail }));

/** Einstellungen › Gruppen. `focus`: nach dem Neuzeichnen diesen Knopf wieder fokussieren. */
export async function renderGroupSettings(focus?: { name: string; act: string }) {
  const box = $("settings-group-list");
  const [list, items] = await Promise.all([api.groupsList().catch(() => []), api.list().catch(() => [])]);
  if (!list.length) return void box.replaceChildren("Noch keine Gruppen.");
  box.replaceChildren(
    ...list.map((g, i) => {
      const uses = items.filter((c) => c.group === g.name).length;
      const row = document.createElement("div");
      row.className = "cert-row";
      row.dataset.name = g.name;
      const left = document.createElement("div");
      const title = Object.assign(document.createElement("div"), { className: "cert-host" });
      title.append(groupDot(g.color), g.name);
      left.append(title, Object.assign(document.createElement("code"), { textContent: `${uses} Verbindung(en)${g.color ? ` · ${GROUP_COLORS[g.color]?.label}` : ""}` }));
      const btn = (act: string, label: string, run: (b: HTMLButtonElement) => Promise<unknown>, aria?: string, disabled = false) => {
        const b = Object.assign(document.createElement("button"), { type: "button", className: "link", textContent: label, disabled });
        b.dataset.act = act;
        if (aria) b.setAttribute("aria-label", aria);
        b.onclick = async () => {
          const r = await run(b);
          if (r === false || r === null) return;
          const to = typeof r === "string" ? r : g.name;
          changed(typeof r === "string" ? { from: g.name, to } : {});
          await renderGroupSettings({ name: to, act });
        };
        return b;
      };
      const actions = document.createElement("div");
      actions.className = "row-actions";
      actions.append(
        btn("up", "↑", () => moveGroup(g.name, -1), `«${g.name}» nach oben`, i === 0),
        btn("down", "↓", () => moveGroup(g.name, 1), `«${g.name}» nach unten`, i === list.length - 1),
        btn("color", "Farbe", (b) => pickGroupColor(b, g.name)),
        btn("rename", "Umbenennen", () => renameGroup(g.name)),
        btn("delete", "Löschen", () => deleteGroup(g.name, uses)),
      );
      row.append(left, actions);
      return row;
    }),
  );
  if (focus) {
    const row = [...box.querySelectorAll<HTMLElement>(".cert-row")].find((r) => r.dataset.name === focus.name);
    const b = row?.querySelector<HTMLButtonElement>(`[data-act="${focus.act}"]`);
    (b && !b.disabled ? b : row?.querySelector<HTMLButtonElement>("button:not(:disabled)") ?? $("group-new")).focus();
  }
}

export function initGroups() {
  $("group-new").onclick = async () => {
    const name = await newGroup();
    if (name === null) return;
    changed();
    renderGroupSettings();
  };
}
