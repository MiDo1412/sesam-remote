/** Sitzungen als Tabs. «Bibliothek» und «Minimieren» lassen sie im Hintergrund weiterlaufen; erst «Trennen» beendet sie. */
import type { Connection } from "./api";
import { ICON } from "./icons";
import { openMenu } from "./menu";
import { ask, choose, tell } from "./ui";
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
      { value: "detach", label: detachedWindow ? "Ins Hauptfenster zurück" : "In eigenem Fenster öffnen", icon: ICON.detach, separator: true },
      { value: "min", label: "Minimieren", icon: ICON.minimize, disabled: detachedWindow },
      { value: "close", label: "Trennen", icon: ICON.disconnect },
    ]);
    if (v === "save") dispatchEvent(new CustomEvent("sesam:save-quick", { detail: conn }));
    if (v === "dup") openTab(conn, kind, true);
    if (v === "detach") detachTab(tab);
    if (v === "min") minimizeTab(tab);
    if (v === "close") closeTab(tab);
  };
  tabs.push(tab);
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
  const h = JSON.parse(raw) as { conn: Connection; kind: Kind; sid: string; state?: unknown };
  openTab(h.conn, h.kind, true, { attach: { token, sid: h.sid, state: h.state } });
  return true;
}

export async function detachTab(tab: Tab) {
  const why = tab.handle.busy?.();
  if (why) return tell("Gerade nicht möglich", `${why} Bitte warten, bis sie fertig sind.`);
  const info = tab.state === "connected" ? tab.handle.handoff?.() : null;
  if (!info) return tell("Gerade nicht möglich", "Nur verbundene Sitzungen lassen sich in ein anderes Fenster verschieben.");
  const token = crypto.randomUUID();
  try {
    localStorage.setItem(`sesam.handoff.${token}`, JSON.stringify({ conn: tab.ctx.conn, kind: tab.kind, ...info }));
  } catch {
    return tell("Gerade nicht möglich", "Der Zustand der Sitzung ist zu gross für die Übergabe.");
  }
  pending.set(token, tab);
  try {
    if (detachedWindow) await emitTo("main", "sesam:handoff", token);
    else await invoke("open_detached", { token, title: `${tab.ctx.conn.name} · ${kindLabel[tab.kind]}` });
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
  b.title = opts.shortcut ? `${label} (${keyLabel(opts.shortcut)})` : label;
  b.setAttribute("aria-label", label);
  b.onclick = onClick;
  return b;
}

export const toolSep = () => Object.assign(document.createElement("span"), { className: "tool-sep" });
