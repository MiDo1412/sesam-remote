/** Sitzungen als Tabs. «Bibliothek» und «Minimieren» lassen sie im Hintergrund weiterlaufen; erst «Trennen» beendet sie. */
import type { Connection } from "./api";
import { ICON } from "./icons";
import { openMenu } from "./menu";
import { ask, askText, choose, tell } from "./ui";
import { GROUP_COLORS, colorMenu, dotSvg, groupDot } from "./groups";
import { emit, emitTo, listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { keyLabel } from "./titlebar";

/** Dieses Fenster zeigt abgekoppelte Sitzungen (ohne Bibliothek). */
export const detachToken: string = (window as unknown as { __SESAM_DETACH__?: string }).__SESAM_DETACH__ ?? new URLSearchParams(location.search).get("detach") ?? "";
export const detachedWindow = detachToken !== "";

export type Kind = "desktop" | "shell" | "files";

export interface TabCtx {
  id: string;
  conn: Connection;
  /** Inhalt des Tabs. */
  pane: HTMLElement;
  /** Werkzeugleiste des Tabs (nur sichtbar, wenn aktiv). */
  toolbar: HTMLElement;
  status: HTMLElement;
  setSub(text: string): void;
  isActive(): boolean;
  /** Statuspunkt im Tab und in der Bibliothek. */
  setState(s: "connecting" | "connected" | "closed"): void;
  /** Weitere Shell usw. als neuen Tab öffnen. */
  openTab(kind: Kind, fresh?: boolean): void;
  /** Zusatzangaben beim Öffnen, z. B. ein Befehl, den die Shell nach dem Verbinden tippt,
   *  oder eine laufende Sitzung aus einem anderen Fenster (`attach`). */
  extra?: { command?: string; attach?: Attach };
  closeSelf(): void;
}

export interface TabHandle {
  close(): void;
  onShow?(): void;
  onHide?(): void;
  /** Grund, warum Schliessen nachfragen soll (z. B. laufende Übertragungen). */
  busy?(): string | null;
  /** Für die Übergabe an ein anderes Fenster: Sitzungs-ID und Zustand (Bild, Text …); null = nicht möglich. */
  handoff?(): { sid: string; state?: unknown } | null;
  /** Nach erfolgreicher Übergabe: Oberfläche aufräumen, ohne die Verbindung zu trennen. */
  release?(): void;
}

/** Übernahme einer laufenden Sitzung; nach dem Anhängen meldet der Tab `sesam:handoff-done` mit `token`. */
export interface Attach {
  token: string;
  sid: string;
  state?: unknown;
}

type Factory = (ctx: TabCtx) => TabHandle;

interface Tab {
  ctx: TabCtx;
  kind: Kind;
  handle: TabHandle;
  tabEl: HTMLElement;
  state: "connecting" | "connected" | "closed";
  sub: string;
  /** Aus der Tab-Leiste genommen, läuft weiter; zurück über «Aktive Sitzungen». */
  minimized: boolean;
  /** Eigener Name, Farbe (Schlüssel aus GROUP_COLORS), Symbol (Schlüssel aus TAB_ICONS); leer = automatisch. */
  name: string;
  color: string;
  icon: string;
  group?: TabGroup;
}

/** Tab-Gruppe; ihre Tabs stehen in `tabs` immer direkt hintereinander. */
interface TabGroup {
  name: string;
  color: string;
  collapsed: boolean;
  el: HTMLButtonElement;
}

/** Wählbare Tab-Symbole («Standard» = Symbol der Art). */
const TAB_ICONS: Record<string, { label: string; svg: string }> = {
  "": { label: "Standard", svg: "" },
  desktop: { label: "Desktop", svg: ICON.desktop },
  shell: { label: "Shell", svg: ICON.shell },
  folder: { label: "Ordner", svg: ICON.folder },
  globe: { label: "Netz", svg: ICON.globe },
  keyboard: { label: "Tastatur", svg: ICON.keyboard },
  quick: { label: "Blitz", svg: ICON.quick },
  osWindows: { label: "Windows", svg: ICON.osWindows },
  osMac: { label: "macOS", svg: ICON.osMac },
  osLinux: { label: "Linux", svg: ICON.osLinux },
  osPi: { label: "Raspberry Pi", svg: ICON.osPi },
};

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const tabs: Tab[] = [];
let active: Tab | null = null;
let factories: Record<Kind, Factory>;
let onLibrary: () => void = () => {};
let seq = 0;

const kindLabel: Record<Kind, string> = { desktop: "Desktop", shell: "Shell", files: "Dateien" };
const kindIcon: Record<Kind, string> = { desktop: ICON.desktop, shell: ICON.shell, files: ICON.folder };
// Fallbacks: künftige Arten (z. B. lokales Terminal) ohne Eintrag oben funktionieren trotzdem.
const kindName = (k: Kind) => kindLabel[k] ?? k;
const kindSvg = (k: Kind) => kindIcon[k] ?? ICON.shell;
const tabTitle = (t: Tab) => t.name || `${t.ctx.conn.name} · ${kindName(t.kind)}`;
const tabSvg = (t: Tab) => TAB_ICONS[t.icon]?.svg || kindSvg(t.kind);
const colorCss = (c: string | undefined) => (c && GROUP_COLORS[c]?.css) || "";
const isMenuKey = (e: KeyboardEvent) => e.key === "ContextMenu" || (e.shiftKey && e.key === "F10");
const liveGroups = () => [...new Set(tabs.map((t) => t.group).filter((g): g is TabGroup => !!g))];
const lastIndex = <T>(a: T[], f: (x: T) => boolean) => {
  for (let i = a.length - 1; i >= 0; i--) if (f(a[i]!)) return i;
  return -1;
};

// ---------- Gespeicherte Tab-Angaben (localStorage «sesam.tabs») ----------
// Schlüssel: Verbindungs-ID + Art. Nur Name, Farbe, Symbol und Gruppe; keine Zugangsdaten, keine Sitzungs-IDs.
// shortcut: mehrere Tabs derselben Verbindung und Art teilen einen Eintrag (zuletzt geändert gewinnt).
interface Saved { name?: string; color?: string; icon?: string; group?: string }
const STORE = "sesam.tabs";
const str = (v: unknown) => (typeof v === "string" ? v : "");
const saved: { tabs: Record<string, Saved>; groups: Record<string, { color: string; collapsed: boolean }> } = (() => {
  try {
    const v = JSON.parse(localStorage.getItem(STORE) ?? "null");
    if (v && typeof v.tabs === "object" && typeof v.groups === "object") return v;
  } catch {}
  return { tabs: {}, groups: {} };
})();
const savedKey = (t: Tab) => `${t.ctx.conn.id}|${t.kind}`;

/** Angaben der geänderten Tabs und aller Gruppen sichern. */
function remember(...changed: Tab[]) {
  for (const t of changed) {
    const e: Saved = { name: t.name || undefined, color: t.color || undefined, icon: t.icon || undefined, group: t.group?.name };
    if (Object.values(e).some(Boolean)) saved.tabs[savedKey(t)] = e;
    else delete saved.tabs[savedKey(t)];
  }
  for (const g of liveGroups()) saved.groups[g.name] = { color: g.color, collapsed: g.collapsed };
  for (const n of Object.keys(saved.groups)) if (!Object.values(saved.tabs).some((e) => e.group === n)) delete saved.groups[n];
  try {
    localStorage.setItem(STORE, JSON.stringify(saved));
  } catch {}
}

export function initTabs(f: Record<Kind, Factory>, library: () => void) {
  factories = f;
  onLibrary = library;
  $("session-back").onclick = showLibrary;
}

export const hasTabs = () => tabs.length > 0;

/** Tab öffnen oder vorhandenen gleicher Art zeigen (`fresh` erzwingt einen neuen, z. B. weitere Shell). */
export function openTab(conn: Connection, kind: Kind, fresh = false, extra?: TabCtx["extra"]) {
  const existing = !fresh && tabs.find((t) => t.ctx.conn.id === conn.id && t.kind === kind && t.state !== "closed");
  if (existing) return activate(existing);

  const id = `tab${++seq}`;
  const pane = Object.assign(document.createElement("div"), { className: `tab-pane ${kind}` });
  const toolbar = Object.assign(document.createElement("div"), { className: "tools tab-tools" });
  const status = Object.assign(document.createElement("div"), { className: "status" });
  status.setAttribute("role", "status");
  pane.append(status);
  $("session-panes").append(pane);
  $("session-toolbar").append(toolbar);

  const tabEl = document.createElement("div");
  tabEl.className = "tab";
  tabEl.setAttribute("role", "tab");
  tabEl.tabIndex = 0;
  const label = document.createElement("span");
  label.className = "tab-label";
  const dot = Object.assign(document.createElement("span"), { className: "dot" });
  const x = document.createElement("button");
  x.type = "button";
  x.className = "tab-close";
  x.innerHTML = ICON.close;
  x.title = "Trennen";
  const min = document.createElement("button");
  min.type = "button";
  min.className = "tab-close";
  min.innerHTML = ICON.minimize;
  min.title = "Minimieren (läuft weiter)";
  tabEl.innerHTML = kindSvg(kind);
  tabEl.append(dot, label, min, x);

  const tab: Tab = {
    kind,
    tabEl,
    state: "connecting",
    sub: "",
    minimized: false,
    name: "",
    color: "",
    icon: "",
    handle: { close() {} },
    ctx: {
      id,
      conn,
      pane,
      toolbar,
      status,
      setSub: (t) => {
        tab.sub = t;
        if (active === tab) $("session-sub").textContent = t;
        tabEl.title = `${conn.name}\n${t}`;
      },
      isActive: () => active === tab,
      setState: (s) => {
        tab.state = s;
        tabEl.dataset.state = s;
        renderActiveBar();
      },
      openTab: (k, f) => openTab(conn, k, f),
      extra,
      closeSelf: () => closeTab(tab),
    },
  };
  tabEl.onclick = () => activate(tab);
  tabEl.onkeydown = (e) => {
    if (e.target !== tabEl) return;
    if (e.key === "Enter" || e.key === " ") activate(tab);
    else if (isMenuKey(e)) {
      e.preventDefault();
      void menu();
    }
  };
  label.ondblclick = () => void renameTab(tab);
  dragTab(tab);
  min.onclick = (e) => {
    e.stopPropagation();
    minimizeTab(tab);
  };
  x.onclick = (e) => {
    e.stopPropagation();
    closeTab(tab);
  };
  tabEl.oncontextmenu = (e) => {
    e.preventDefault();
    void menu();
  };
  const menu = async () => {
    const v = await openMenu(tabEl, [
      { value: "rename", label: "Umbenennen …", icon: ICON.rename },
      { value: "color", label: "Farbe …", icon: dotSvg(colorCss(tab.color)) },
      { value: "icon", label: "Symbol …", icon: tabSvg(tab) },
      { value: "group", label: "Zu Gruppe hinzufügen …", icon: ICON.plus },
      ...(tab.group ? [{ value: "ungroup", label: "Aus Gruppe entfernen" }] : []),
      { value: "dup", label: `Weitere ${kindName(kind)}-Sitzung`, icon: ICON.plus, disabled: kind === "desktop", separator: true },
      ...(conn.id.startsWith("quick-") ? [{ value: "save", label: "Als Verbindung sichern …", icon: ICON.save }] : []),
      { value: "detach", label: detachedWindow ? "Ins Hauptfenster zurück" : "In eigenem Fenster öffnen", icon: ICON.detach, separator: true },
      { value: "min", label: "Minimieren", icon: ICON.minimize, disabled: detachedWindow },
      { value: "close", label: "Trennen", icon: ICON.disconnect },
    ]);
    if (v === "save") dispatchEvent(new CustomEvent("sesam:save-quick", { detail: conn }));
    if (v === "dup") openTab(conn, kind, true);
    if (v === "detach") detachTab(tab);
    if (v === "min") minimizeTab(tab);
    if (v === "close") closeTab(tab);
    if (v === "rename") renameTab(tab);
    if (v === "color") pickTabColor(tab);
    if (v === "icon") pickTabIcon(tab);
    if (v === "group") addToGroup(tab);
    if (v === "ungroup") setGroup(tab, undefined);
  };
  // Gespeicherte Angaben dieser Verbindung und Art übernehmen (Werte aus dem localStorage prüfen).
  const s = saved.tabs[`${conn.id}|${kind}`];
  if (s) {
    tab.name = str(s.name);
    tab.color = str(s.color);
    tab.icon = str(s.icon);
    const gn = str(s.group);
    if (gn) {
      const sg = saved.groups[gn];
      tab.group = liveGroups().find((g) => g.name === gn) ?? makeGroup(gn, str(sg?.color), sg?.collapsed === true);
    }
  }
  const after = tab.group ? lastIndex(tabs, (t) => t.group === tab.group) : -1;
  tabs.splice(after < 0 ? tabs.length : after + 1, 0, tab);
  paintTab(tab);
  tab.ctx.setState("connecting");
  activate(tab);
  tab.handle = factories[kind](tab.ctx);
  // «Abkoppeln» und «Minimieren» vor «Trennen» in jeder Werkzeugleiste (auch für das Menü «Sitzung»).
  const extraBtns = [toolButton(ICON.detach, detachedWindow ? "Andocken" : "Abkoppeln", () => detachTab(tab), { shortcut: detachedWindow ? "zurück ins Hauptfenster" : "eigenes Fenster" })];
  if (!detachedWindow) extraBtns.push(toolButton(ICON.minimize, "Minimieren", () => minimizeTab(tab), { shortcut: "läuft weiter" }));
  const last = toolbar.lastElementChild;
  for (const b of extraBtns) last ? toolbar.insertBefore(b, last) : toolbar.append(b);
}

// ---------- Abkoppeln / Andocken ----------
// Die Verbindung bleibt bestehen: das Backend sendet ab der Übernahme an das neue Fenster.
// Ablauf: Zustand in den localStorage, anderes Fenster übernimmt und meldet `sesam:handoff-done`.
const pending = new Map<string, Tab>();
listen<string>("sesam:handoff-done", (e) => {
  const tab = pending.get(e.payload);
  if (!tab) return;
  pending.delete(e.payload);
  tab.handle.release?.();
  removeTab(tab);
});
// Hauptfenster: Sitzung aus einem abgekoppelten Fenster zurücknehmen.
if (!detachedWindow) listen<string>("sesam:handoff", (e) => takeOver(e.payload));

/** Übergabe aus dem localStorage übernehmen (Hauptfenster: Andocken; neues Fenster: Abkoppeln). */
export function takeOver(token: string) {
  const key = `sesam.handoff.${token}`;
  const raw = localStorage.getItem(key);
  localStorage.removeItem(key);
  if (!raw) return false;
  const h = JSON.parse(raw) as { conn: Connection; kind: Kind; sid: string; state?: unknown; name?: string; color?: string; icon?: string };
  openTab(h.conn, h.kind, true, { attach: { token, sid: h.sid, state: h.state } });
  // Mit `fresh` legt openTab immer einen neuen Tab an; Name, Farbe und Symbol wandern mit, die Gruppe nicht.
  const t = tabs.find((x) => x.ctx.extra?.attach?.token === token);
  if (t) Object.assign(t, { name: str(h.name), color: str(h.color), icon: str(h.icon) }), paintTab(t);
  return true;
}

export async function detachTab(tab: Tab) {
  const why = tab.handle.busy?.();
  if (why) return tell("Gerade nicht möglich", `${why} Bitte warten, bis sie fertig sind.`);
  const info = tab.state === "connected" ? tab.handle.handoff?.() : null;
  if (!info) return tell("Gerade nicht möglich", "Nur verbundene Sitzungen lassen sich in ein anderes Fenster verschieben.");
  const token = crypto.randomUUID();
  try {
    localStorage.setItem(`sesam.handoff.${token}`, JSON.stringify({ conn: tab.ctx.conn, kind: tab.kind, name: tab.name, color: tab.color, icon: tab.icon, ...info }));
  } catch {
    return tell("Gerade nicht möglich", "Der Zustand der Sitzung ist zu gross für die Übergabe.");
  }
  pending.set(token, tab);
  try {
    if (detachedWindow) await emitTo("main", "sesam:handoff", token);
    else await invoke("open_detached", { token, title: tabTitle(tab) });
  } catch (e) {
    pending.delete(token);
    localStorage.removeItem(`sesam.handoff.${token}`);
    return tell("Nicht verschoben", String(e));
  }
  // Kommt keine Bestätigung, bleibt die Sitzung hier.
  setTimeout(() => {
    if (pending.delete(token)) localStorage.removeItem(`sesam.handoff.${token}`);
  }, 20000);
}

/** Tab-Oberfläche entfernen (die Sitzung ist bereits geschlossen oder übergeben). */
function removeTab(tab: Tab) {
  tab.ctx.pane.remove();
  tab.ctx.toolbar.remove();
  tab.tabEl.remove();
  tabs.splice(tabs.indexOf(tab), 1);
  if (active === tab) {
    active = null;
    const next = [...tabs].reverse().find((t) => !t.minimized);
    if (next) activate(next);
    else showLibrary();
  }
  renderActiveBar();
}

/** Abgekoppeltes Fenster: Schliessen trennt seine Sitzungen (mit Rückfrage bei laufenden Übertragungen). */
if (detachedWindow) {
  // Schliessen: Standard ist Andocken (Sitzung zurück ins Hauptfenster), sonst Trennen. Das Fenster geht in jedem Fall zu.
  getCurrentWindow().onCloseRequested(async (e) => {
    if (!tabs.length) return;
    e.preventDefault();
    const v = await choose("Fenster schliessen", "Die Sitzung wieder ins Hauptfenster nehmen oder trennen?", "Beim Andocken bleibt die Verbindung bestehen.", "Wieder andocken", "Trennen");
    if (v === null) return;
    for (const t of [...tabs]) {
      if (v === "alt" || t.state !== "connected") await closeTab(t, true);
      else await detachTab(t);
    }
    // Falls ein Andocken nicht bestätigt wird: nach kurzer Zeit trotzdem schliessen (Sitzung bleibt im Hauptfenster nicht erhalten).
    setTimeout(() => {
      for (const t of [...tabs]) closeTab(t, true);
      getCurrentWindow().destroy().catch(() => {});
    }, 8000);
  });
}

/** Für die Tab-Fabriken: nach erfolgreichem Anhängen das abgebende Fenster benachrichtigen. */
export const handoffDone = (token: string) => emit("sesam:handoff-done", token);

/** Sitzung aus der Tab-Leiste nehmen; sie läuft weiter und kommt über «Aktive Sitzungen» zurück. */
export function minimizeTab(tab: Tab) {
  tab.minimized = true;
  if (active === tab) {
    const next = [...tabs].reverse().find((t) => !t.minimized);
    if (next) activate(next);
    else showLibrary();
  }
  renderActiveBar();
}

function activate(tab: Tab) {
  if (active && active !== tab) {
    active.ctx.pane.hidden = true;
    active.ctx.toolbar.hidden = true;
    active.tabEl.setAttribute("aria-selected", "false");
    active.handle.onHide?.();
  }
  active = tab;
  tab.minimized = false;
  if (tab.group?.collapsed) (tab.group.collapsed = false), remember();
  layoutTabs();
  for (const t of tabs) {
    t.ctx.pane.hidden = t !== tab;
    t.ctx.toolbar.hidden = t !== tab;
    t.tabEl.setAttribute("aria-selected", String(t === tab));
  }
  $("session-title").textContent = tab.name || tab.ctx.conn.name;
  $("session-sub").textContent = tab.sub;
  $("library").hidden = true;
  $("session").hidden = false;
  tab.handle.onShow?.();
}

export function showLibrary() {
  // Abgekoppeltes Fenster ohne Sitzungen: schliessen.
  if (detachedWindow) return void (tabs.length ? activate(tabs[tabs.length - 1]!) : getCurrentWindow().destroy());
  active?.handle.onHide?.();
  $("session").hidden = true;
  $("library").hidden = false;
  renderActiveBar();
  onLibrary();
}

export async function closeTab(tab: Tab, force = false) {
  const why = tab.handle.busy?.();
  if (why && !force && !(await ask("Sitzung trennen?", why, "Trennen"))) return;
  tab.handle.close();
  removeTab(tab);
}

// ---------- Name, Farbe, Symbol, Gruppen ----------

/** Beschriftung, Symbol und Titel eines Tabs nach einer Änderung neu setzen. */
function paintTab(t: Tab) {
  const [min, x] = t.tabEl.querySelectorAll(".tab-close");
  t.tabEl.firstElementChild!.outerHTML = tabSvg(t);
  t.tabEl.querySelector(".tab-label")!.textContent = tabTitle(t);
  min?.setAttribute("aria-label", `${tabTitle(t)} minimieren`);
  x?.setAttribute("aria-label", `${tabTitle(t)} trennen`);
  if (active === t) $("session-title").textContent = t.name || t.ctx.conn.name;
  renderActiveBar();
}

/** Tab-Leiste in der Reihenfolge von `tabs` aufbauen, vor jeder Gruppe ihr Chip. Nur bei Änderung umhängen (Fokus bleibt). */
function layoutTabs() {
  const want: HTMLElement[] = [];
  let prev: TabGroup | undefined;
  for (const t of tabs) {
    if (t.group && t.group !== prev) want.push(groupChip(t.group));
    prev = t.group;
    t.tabEl.hidden = t.minimized || !!t.group?.collapsed;
    t.tabEl.style.setProperty("--tab-color", colorCss(t.color) || colorCss(t.group?.color) || "transparent");
    want.push(t.tabEl);
  }
  const box = $("tabs");
  if (want.length !== box.children.length || want.some((el, i) => box.children[i] !== el)) box.replaceChildren(...want);
}

function groupChip(g: TabGroup) {
  const n = tabs.filter((t) => t.group === g).length;
  g.el.style.setProperty("--tab-color", colorCss(g.color) || "var(--chrome-muted)");
  g.el.setAttribute("aria-expanded", String(!g.collapsed));
  g.el.setAttribute("aria-label", `Gruppe ${g.name}, ${n} Tab(s)`);
  g.el.title = g.collapsed ? "Gruppe aufklappen" : "Gruppe zuklappen (Sitzungen laufen weiter)";
  g.el.replaceChildren(Object.assign(document.createElement("span"), { className: "tab-label", textContent: g.name }));
  if (g.collapsed) g.el.append(Object.assign(document.createElement("span"), { className: "tab-group-count", textContent: String(n) }));
  return g.el;
}

function makeGroup(name: string, color: string, collapsed = false): TabGroup {
  const g: TabGroup = { name, color, collapsed, el: document.createElement("button") };
  g.el.type = "button";
  g.el.className = "tab-group";
  g.el.onclick = () => toggleGroup(g);
  g.el.oncontextmenu = (e) => {
    e.preventDefault();
    void groupMenu(g);
  };
  g.el.onkeydown = (e) => {
    if (!isMenuKey(e)) return;
    e.preventDefault();
    void groupMenu(g);
  };
  dropTarget(g.el, () => ({ before: tabs.find((t) => t.group === g) ?? null, group: g }));
  return g;
}

function toggleGroup(g: TabGroup) {
  g.collapsed = !g.collapsed;
  // Zuklappen mit aktivem Tab darin: zu einem sichtbaren Tab wechseln (falls es einen gibt).
  if (g.collapsed && active?.group === g) {
    const next = tabs.find((t) => !t.minimized && !t.group?.collapsed);
    if (next) activate(next);
  }
  layoutTabs();
  remember();
}

async function groupMenu(g: TabGroup) {
  const members = () => tabs.filter((t) => t.group === g);
  const v = await openMenu(g.el, [
    { value: "rename", label: "Umbenennen …", icon: ICON.rename },
    { value: "color", label: "Farbe …", icon: dotSvg(colorCss(g.color)) },
    { value: "ungroup", label: "Gruppe auflösen", separator: true },
    { value: "close", label: "Alle Tabs der Gruppe trennen …", icon: ICON.disconnect },
  ]);
  if (v === "rename") {
    const to = await askText("Gruppe umbenennen", "Neuer Name der Tab-Gruppe.", "Umbenennen", g.name);
    if (to && to !== g.name) {
      for (const e of Object.values(saved.tabs)) if (e.group === g.name) e.group = to;
      delete saved.groups[g.name];
      g.name = to;
    }
  }
  if (v === "color") {
    const c = await colorMenu(g.el, g.color);
    if (c !== null) g.color = c;
  }
  if (v === "ungroup") {
    for (const e of Object.values(saved.tabs)) if (e.group === g.name) delete e.group;
    for (const t of members()) t.group = undefined;
  }
  if (v === "close") {
    const m = members();
    if (!(await ask(`Gruppe «${g.name}» trennen?`, `${m.length} Sitzung(en) werden getrennt.`, "Trennen"))) return;
    for (const t of m) await closeTab(t);
  }
  renderActiveBar();
  remember(...members());
}

async function renameTab(tab: Tab) {
  const v = await askText("Tab umbenennen", "Leer lassen für den automatischen Namen.", "Umbenennen", tab.name);
  if (v === null) return;
  tab.name = v;
  paintTab(tab);
  remember(tab);
}

async function pickTabColor(tab: Tab) {
  const c = await colorMenu(tab.tabEl, tab.color);
  if (c === null) return;
  tab.color = c;
  paintTab(tab);
  remember(tab);
}

async function pickTabIcon(tab: Tab) {
  const v = await openMenu(
    tab.tabEl,
    Object.entries(TAB_ICONS).map(([value, i]) => ({ value, label: i.label, icon: i.svg || kindSvg(tab.kind) })),
    { checked: tab.icon, listbox: true },
  );
  if (v === null) return;
  tab.icon = v;
  paintTab(tab);
  remember(tab);
}

async function addToGroup(tab: Tab) {
  const gs = liveGroups().filter((g) => g !== tab.group);
  const v = await openMenu(tab.tabEl, [
    ...gs.map((g, i) => ({ value: String(i), label: g.name, icon: dotSvg(colorCss(g.color)) })),
    { value: "new", label: "Neue Gruppe …", icon: ICON.plus, separator: gs.length > 0 },
  ]);
  if (v === null) return;
  let g = gs[Number(v)];
  if (v === "new") {
    const name = await askText("Neue Gruppe", "Name der Tab-Gruppe. Danach wählst du die Farbe.", "Weiter");
    if (!name) return;
    g = liveGroups().find((x) => x.name === name) ?? makeGroup(name, (await colorMenu(tab.tabEl, "")) ?? "");
  }
  if (g) setGroup(tab, g);
}

/** Tab in eine Gruppe (ans Ende) oder aus ihr heraus (direkt hinter die Gruppe) setzen. */
function setGroup(tab: Tab, g: TabGroup | undefined) {
  const anchor = g ?? tab.group;
  const i = tabs.indexOf(tab);
  tabs.splice(i, 1);
  const last = lastIndex(tabs, (t) => t.group === anchor);
  tabs.splice(last < 0 ? i : last + 1, 0, tab);
  tab.group = g;
  if (g) g.collapsed = false;
  renderActiveBar();
  remember(tab);
}

/** Tab vor `before` (null = ans Ende) und in `group` setzen. */
function moveTab(tab: Tab, before: Tab | null, group: TabGroup | undefined) {
  if (before === tab) return;
  tabs.splice(tabs.indexOf(tab), 1);
  tabs.splice(before ? tabs.indexOf(before) : tabs.length, 0, tab);
  tab.group = group;
  if (group) group.collapsed = false;
  renderActiveBar();
  remember(tab);
}

// ---------- Ziehen (Zeiger); Tastatur-Alternative ist das Kontextmenü ----------
let dragging: Tab | null = null;
const dropTargets = new WeakMap<HTMLElement, (x: number) => { before: Tab | null; group: TabGroup | undefined }>();

function dropTarget(el: HTMLElement, at: (x: number) => { before: Tab | null; group: TabGroup | undefined }) {
  dropTargets.set(el, at);
}

function dragTab(tab: Tab) {
  // Linke Hälfte: davor, rechte Hälfte: dahinter; die Gruppe des Ziel-Tabs wird übernommen (Gruppen bleiben zusammenhängend).
  dropTarget(tab.tabEl, (x) => {
    const r = tab.tabEl.getBoundingClientRect();
    const rest = tabs.filter((t) => t !== dragging);
    return { before: x < r.left + r.width / 2 ? tab : (rest[rest.indexOf(tab) + 1] ?? null), group: tab.group };
  });
  let start: { x: number; id: number } | null = null;
  let over: HTMLElement | null = null;
  const mark = (el: HTMLElement | null) => {
    over?.classList.remove("drop-target");
    over = el;
    el?.classList.add("drop-target");
  };
  tab.tabEl.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    start = { x: e.clientX, id: e.pointerId };
  });
  tab.tabEl.addEventListener("pointermove", (e) => {
    if (!start || e.pointerId !== start.id) return;
    if (!dragging) {
      if (Math.abs(e.clientX - start.x) < 6) return;
      dragging = tab;
      tab.tabEl.setPointerCapture(e.pointerId);
      tab.tabEl.classList.add("dragging");
    }
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>(".tab, .tab-group") ?? null;
    mark(el && el !== tab.tabEl && dropTargets.has(el) ? el : null);
  });
  const end = (e: PointerEvent) => {
    if (!start || e.pointerId !== start.id) return;
    start = null;
    if (dragging !== tab) return;
    const target = over;
    const under = document.elementFromPoint(e.clientX, e.clientY);
    mark(null);
    tab.tabEl.classList.remove("dragging");
    const d = target ? dropTargets.get(target)!(e.clientX) : null;
    dragging = null;
    if (d) moveTab(tab, d.before, d.group);
    // Freie Fläche der Leiste: ans Ende, ohne Gruppe.
    else if (under?.closest("#tabs") && !under.closest(".tab, .tab-group")) moveTab(tab, null, undefined);
  };
  tab.tabEl.addEventListener("pointerup", end);
  tab.tabEl.addEventListener("pointercancel", (e) => {
    mark(null);
    tab.tabEl.classList.remove("dragging");
    if (start?.id === e.pointerId) start = null;
    dragging = null;
  });
}

/** Leiste «Aktive Sitzungen» in der Bibliothek. */
function renderActiveBar() {
  layoutTabs();
  const bar = $("active-sessions");
  bar.hidden = tabs.length === 0;
  bar.replaceChildren(
    Object.assign(document.createElement("span"), { className: "active-title", textContent: "Aktive Sitzungen" }),
    ...tabs.map((t) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "chip";
      b.dataset.state = t.state;
      b.innerHTML = tabSvg(t);
      b.title = t.minimized ? "Minimiert – klicken zum Fortsetzen" : "Zur Sitzung wechseln";
      b.append(Object.assign(document.createElement("span"), { className: "dot" }), groupDot(t.color || t.group?.color || ""),document.createTextNode(`${tabTitle(t)}${t.group ? ` (${t.group.name})` : ""}${t.minimized ? " (minimiert)" : ""}`));
      b.onclick = () => activate(t);
      return b;
    }),
  );
}

/** Menübefehl «Sitzung»: Werkzeugknopf des aktiven Tabs über seine Beschriftung auslösen. */
export function runTool(label: string) {
  if (label === "Bibliothek") return showLibrary();
  if (label === "Nächster Tab" || label === "Vorheriger Tab") {
    if (!tabs.length) return;
    const open = tabs.filter((t) => (!t.minimized && !t.group?.collapsed) || t === active);
    if (!open.length) return;
    const j = active ? open.indexOf(active) : -1;
    return activate(open[(j + (label === "Nächster Tab" ? 1 : open.length - 1) + open.length) % open.length]!);
  }
  if (!active || $("session").hidden) return;
  active.ctx.toolbar.querySelector<HTMLButtonElement>(`button[aria-label="${CSS.escape(label)}"]:not(:disabled)`)?.click();
}

/** Werkzeugknopf mit Icon; Tooltip mit Tastenkürzel. */
export function toolButton(icon: string, label: string, onClick: () => void, opts: { shortcut?: string; text?: boolean; danger?: boolean } = {}) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = `tool${opts.text ? " with-text" : ""}${opts.danger ? " danger" : ""}`;
  b.innerHTML = icon;
  if (opts.text) b.append(Object.assign(document.createElement("span"), { textContent: label }));
  b.title = opts.shortcut ? `${label} (${keyLabel(opts.shortcut)})` : label;
  b.setAttribute("aria-label", label);
  b.onclick = onClick;
  return b;
}

export const toolSep = () => Object.assign(document.createElement("span"), { className: "tool-sep" });
