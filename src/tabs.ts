/** Sitzungen als Tabs. «Bibliothek» und «Minimieren» lassen sie im Hintergrund weiterlaufen; erst «Trennen» beendet sie. */
import type { Connection } from "./api";
import { ICON } from "./icons";
import { openMenu } from "./menu";
import { ask } from "./ui";

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
  /** Zusatzangaben beim Öffnen, z. B. ein Befehl, den die Shell nach dem Verbinden tippt. */
  extra?: { command?: string };
  closeSelf(): void;
}

export interface TabHandle {
  close(): void;
  onShow?(): void;
  onHide?(): void;
  /** Grund, warum Schliessen nachfragen soll (z. B. laufende Übertragungen). */
  busy?(): string | null;
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
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const tabs: Tab[] = [];
let active: Tab | null = null;
let factories: Record<Kind, Factory>;
let onLibrary: () => void = () => {};
let seq = 0;

const kindLabel: Record<Kind, string> = { desktop: "Desktop", shell: "Shell", files: "Dateien" };
const kindIcon: Record<Kind, string> = { desktop: ICON.desktop, shell: ICON.shell, files: ICON.folder };

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
  label.textContent = `${conn.name} · ${kindLabel[kind]}`;
  const dot = Object.assign(document.createElement("span"), { className: "dot" });
  const x = document.createElement("button");
  x.type = "button";
  x.className = "tab-close";
  x.innerHTML = ICON.close;
  x.title = "Trennen";
  x.setAttribute("aria-label", `${label.textContent} trennen`);
  const min = document.createElement("button");
  min.type = "button";
  min.className = "tab-close";
  min.innerHTML = ICON.minimize;
  min.title = "Minimieren (läuft weiter)";
  min.setAttribute("aria-label", `${label.textContent} minimieren`);
  tabEl.innerHTML = kindIcon[kind];
  tabEl.append(dot, label, min, x);
  $("tabs").append(tabEl);

  const tab: Tab = {
    kind,
    tabEl,
    state: "connecting",
    sub: "",
    minimized: false,
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
    if (e.key === "Enter" || e.key === " ") activate(tab);
  };
  min.onclick = (e) => {
    e.stopPropagation();
    minimizeTab(tab);
  };
  x.onclick = (e) => {
    e.stopPropagation();
    closeTab(tab);
  };
  tabEl.oncontextmenu = async (e) => {
    e.preventDefault();
    const v = await openMenu(tabEl, [
      { value: "dup", label: `Weitere ${kindLabel[kind]}-Sitzung`, icon: ICON.plus, disabled: kind === "desktop" },
      ...(conn.id.startsWith("quick-") ? [{ value: "save", label: "Als Verbindung sichern …", icon: ICON.save }] : []),
      { value: "min", label: "Minimieren", icon: ICON.minimize, separator: true },
      { value: "close", label: "Trennen", icon: ICON.disconnect },
    ]);
    if (v === "save") dispatchEvent(new CustomEvent("sesam:save-quick", { detail: conn }));
    if (v === "dup") openTab(conn, kind, true);
    if (v === "min") minimizeTab(tab);
    if (v === "close") closeTab(tab);
  };
  tabs.push(tab);
  tab.ctx.setState("connecting");
  activate(tab);
  tab.handle = factories[kind](tab.ctx);
  // «Minimieren» vor «Trennen» in jeder Werkzeugleiste (auch für das Menü «Sitzung»).
  const minBtn = toolButton(ICON.minimize, "Minimieren", () => minimizeTab(tab), { shortcut: "läuft weiter" });
  const last = toolbar.lastElementChild;
  if (last) toolbar.insertBefore(minBtn, last);
  else toolbar.append(minBtn);
}

/** Sitzung aus der Tab-Leiste nehmen; sie läuft weiter und kommt über «Aktive Sitzungen» zurück. */
export function minimizeTab(tab: Tab) {
  tab.minimized = true;
  tab.tabEl.hidden = true;
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
  tab.tabEl.hidden = false;
  for (const t of tabs) {
    t.ctx.pane.hidden = t !== tab;
    t.ctx.toolbar.hidden = t !== tab;
    t.tabEl.setAttribute("aria-selected", String(t === tab));
  }
  $("session-title").textContent = tab.ctx.conn.name;
  $("session-sub").textContent = tab.sub;
  $("library").hidden = true;
  $("session").hidden = false;
  tab.handle.onShow?.();
}

export function showLibrary() {
  active?.handle.onHide?.();
  $("session").hidden = true;
  $("library").hidden = false;
  renderActiveBar();
  onLibrary();
}

export async function closeTab(tab: Tab) {
  const why = tab.handle.busy?.();
  if (why && !(await ask("Sitzung trennen?", why, "Trennen"))) return;
  tab.handle.close();
  tab.ctx.pane.remove();
  tab.ctx.toolbar.remove();
  tab.tabEl.remove();
  tabs.splice(tabs.indexOf(tab), 1);
  if (active === tab) {
    active = null;
    const next = tabs[tabs.length - 1];
    if (next) activate(next);
    else showLibrary();
  }
  renderActiveBar();
}

/** Leiste «Aktive Sitzungen» in der Bibliothek. */
function renderActiveBar() {
  const bar = $("active-sessions");
  bar.hidden = tabs.length === 0;
  bar.replaceChildren(
    Object.assign(document.createElement("span"), { className: "active-title", textContent: "Aktive Sitzungen" }),
    ...tabs.map((t) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "chip";
      b.dataset.state = t.state;
      b.innerHTML = kindIcon[t.kind];
      b.title = t.minimized ? "Minimiert – klicken zum Fortsetzen" : "Zur Sitzung wechseln";
      b.append(Object.assign(document.createElement("span"), { className: "dot" }), document.createTextNode(`${t.ctx.conn.name} · ${kindLabel[t.kind]}${t.minimized ? " (minimiert)" : ""}`));
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
    const open = tabs.filter((t) => !t.minimized || t === active);
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
  b.title = opts.shortcut ? `${label} (${opts.shortcut})` : label;
  b.setAttribute("aria-label", label);
  b.onclick = onClick;
  return b;
}

export const toolSep = () => Object.assign(document.createElement("span"), { className: "tool-sep" });
