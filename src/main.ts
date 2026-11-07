import { api, desktopProtocol, filesProtocol, ftpLabel, type Connection, type Group } from "./api";
import { ICON } from "./icons";
import { groupDot, groupMenu, initGroups, moveGroupBefore } from "./groups";
import { shellTab } from "./shell";
import { desktopTab } from "./desktop";
import { filesTab } from "./files";
import { detachedWindow, detachToken, initTabs, openTab, runTool, takeOver } from "./tabs";
import { ask, tell } from "./ui";
import { openHelp } from "./help";
import { initTitlebar, isMac } from "./titlebar";
import { explainRdpError, fetchPorts, initUbuntu } from "./ubuntu";
import { initTerminal, openTerminal } from "./terminal";
import { applyAppearance, openSettings } from "./settings";
import { initQuick } from "./quick";
import { runOnTarget } from "./setup";
import { initFieldHelp } from "./fieldhelp";
import { forgetThumb, renderThumb } from "./thumbs";
import { adoptFromConnection, initCreds, openCred } from "./creds";
import { invoke } from "@tauri-apps/api/core";
import { getCurrent as deepLinkCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { openMenu } from "./menu";
import { listen } from "@tauri-apps/api/event";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { enhanceSelect } from "./menu";

// Native Auswahllisten durch eigene Menüs ersetzen (einmal beim Start).
function thumbEl(c: Connection, kind: string) {
  const t = el("div", { class: "thumb" }, el("span", { class: "badge" }, kind));
  renderThumb(t, c);
  return t;
}
const credSelect = (document.getElementById("editor-form") as HTMLFormElement).elements.namedItem("credentialId") as HTMLSelectElement;
const credMenu = enhanceSelect(credSelect);
/** Auswahl «Zugangsdaten» füllen; bei gespeicherten Daten eigene Felder ausblenden. */
async function fillCreds(selectedId: string | null) {
  const list = await api.credsList().catch(() => []);
  credSelect.replaceChildren(
    new Option("Eigene für diese Verbindung", ""),
    ...list.map(([c]) => new Option(`${c.name}${c.user ? ` (${c.user})` : ""}`, c.id)),
  );
  credSelect.value = selectedId ?? "";
  credMenu.sync();
  syncCredFields();
}
function syncCredFields() {
  document.getElementById("editor-form")!.classList.toggle("uses-cred", credSelect.value !== "");
  document.getElementById("cred-adopt")!.hidden = credSelect.value !== "" || !editing || !(editing.hasPassword || editing.hasKey) || !items.some((x) => x.id === editing!.id);
}
credSelect.addEventListener("change", syncCredFields);
initCreds();
addEventListener("sesam:creds-changed", () => editing && fillCreds(credSelect.value || null));
addEventListener("sesam:creds-changed", () => editing && svcPickers.forEach((p) => void p.fill(p.value())));
document.getElementById("cred-adopt")!.onclick = async () => {
  if (!editing) return;
  const c = await adoptFromConnection(editing.id, editing.user ? `${editing.user} (${editing.name})` : editing.name);
  if (c) {
    editing.credentialId = c.id;
    await fillCreds(c.id);
    reload();
  }
};
const formEls = (document.getElementById("editor-form") as HTMLFormElement).elements;

/** Gespeicherte Zugangsdaten auch pro Dienst (RDP, VNC, Gateway): Liste, «Eigene …» mit Feldern, «Neue speichern …». */
const NEW_CRED = "__new";
const svcPickers = (["rdpCredentialId", "vncCredentialId", "gatewayCredentialId"] as const).map((name) => {
  const sel = formEls.namedItem(name) as HTMLSelectElement;
  const fields = sel.closest(".own-svc, .adv-fields")!.querySelector<HTMLElement>(".own-fields")!;
  const menu = enhanceSelect(sel);
  let last = "";
  const sync = () => (fields.hidden = sel.value !== "");
  const fill = async (selected: string | null) => {
    const list = await api.credsList().catch(() => []);
    sel.replaceChildren(
      new Option(sel.dataset.own!, ""),
      ...list.map(([c]) => new Option(`${c.name}${c.user ? ` (${c.user})` : ""}`, c.id)),
      new Option("Neue Zugangsdaten speichern …", NEW_CRED),
    );
    sel.value = last = selected && list.some(([c]) => c.id === selected) ? selected : "";
    menu.sync();
    sync();
  };
  sel.addEventListener("change", async () => {
    if (sel.value !== NEW_CRED) return ((last = sel.value), sync());
    const c = await openCred(null);
    await fill(c ? c.id : last);
  });
  return { name, sel, fill, value: () => sel.value || null };
});
const keyModeMenu = enhanceSelect(formEls.namedItem("keyMode") as HTMLSelectElement);
const fileTransferMenu = enhanceSelect(formEls.namedItem("fileTransfer") as HTMLSelectElement);
const ftpSecurity = formEls.namedItem("ftpSecurity") as HTMLSelectElement;
const ftpSecurityMenu = enhanceSelect(ftpSecurity);
const jumpHost = formEls.namedItem("jumpHost") as HTMLSelectElement;
const jumpHostMenu = enhanceSelect(jumpHost);
const osMenu = enhanceSelect(formEls.namedItem("os") as HTMLSelectElement);

// ---------- RDP zu Ubuntu: «Was möchtest du öffnen?» mit genau einem Zugangsdaten-Block ----------
const rdpTargetSel = formEls.namedItem("rdpTarget") as HTMLSelectElement;
const rdpTargetMenu = enhanceSelect(rdpTargetSel);
let rdpPorts: [string, number][] = [];
const fld = (n: string) => formEls.namedItem(n) as HTMLInputElement;
function syncRdpTarget() {
  const v = rdpTargetSel.value;
  const who = fld("rdpSessionUser").value.trim() || "diesem Benutzer";
  document.getElementById("rdp-session-row")!.hidden = v !== "session";
  document.getElementById("rdp-own-opt")!.hidden = v !== "";
  if (v) fld("rdpOwn").checked = true; // bei Ubuntu immer genau dieser eine Block
  const note = document.getElementById("rdp-cred-note")!;
  note.hidden = !v;
  note.textContent =
    v === "login"
      ? "Zugang zum Anmeldebildschirm (vom Einrichtungs-Script gesetzt). Danach meldest du dich wie gewohnt mit deinem Ubuntu-Konto an. Leer lassen, wenn es dieselben Angaben wie unter «Anmeldung» sind (empfohlen)."
      : `Freigabe-Zugangsdaten von ${who} (Ubuntu: Einstellungen › System › Remote-Desktop › Desktopfreigabe). Leer lassen, wenn es dieselben Angaben wie unter «Anmeldung» sind (empfohlen).`;
  if (v === "login" && !fld("rdpPort").value) fld("rdpPort").value = "3389";
}
rdpTargetSel.addEventListener("change", () => {
  if (rdpTargetSel.value === "login") fld("rdpPort").value = "3389";
  syncRdpTarget();
});
fld("rdpSessionUser").addEventListener("input", () => {
  const hit = rdpPorts.find(([u]) => u === fld("rdpSessionUser").value.trim());
  if (hit) fld("rdpPort").value = String(hit[1]);
  syncRdpTarget();
});
document.getElementById("rdp-ports-fetch")!.onclick = async () => {
  const note = document.getElementById("rdp-ports-note")!;
  const host = fld("address").value.trim();
  if (!host) return void (note.textContent = "Zuerst Hostname oder IP-Adresse eintragen.");
  note.textContent = "Lese …";
  try {
    rdpPorts = await fetchPorts(host, Number(fld("sshPort").value) || 22, fld("user").value.trim(), fld("password").value || undefined, editing && items.some((x) => x.id === editing!.id) ? editing.id : undefined);
    document.getElementById("rdp-users")!.replaceChildren(...rdpPorts.map(([u, p]) => new Option(`Port ${p}`, u)));
    note.textContent = rdpPorts.length ? `Gefunden: ${rdpPorts.map(([u, p]) => `${u} (${p})`).join(", ")}` : "Keine festen Ports eingerichtet.";
  } catch (e) {
    note.textContent = String(e);
  }
};
document.getElementById("rdp-check")!.onclick = async () => {
  const out = document.getElementById("rdp-check-result")!;
  if (!editing || !items.some((x) => x.id === editing!.id)) return void (out.textContent = "Bitte zuerst sichern, dann prüfen.");
  out.textContent = "Prüfe (gespeicherter Stand) …";
  try {
    await api.rdpCheck(editing.id);
    out.textContent = "Stimmt: die Zugangsdaten wurden angenommen.";
  } catch (e) {
    out.textContent = explainRdpError(String(e), editing);
  }
};

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

let items: Connection[] = [];
/** Gruppen in der Reihenfolge der Seitenleiste (auch leere). */
let groups: Group[] = [];
let filter: { kind: "all" } | { kind: "group"; name: string } = { kind: "all" };
let selected: string | null = null;
/** Gerade angezeigte Verbindungen (für die Pfeiltasten in der Liste). */
let shown: Connection[] = [];

// Darstellung «Kacheln / Liste» und Sortierung der Liste, lokal gemerkt.
type SortKey = "name" | "address" | "services" | "group" | "lastUsed";
const VIEW_KEY = "sesam.library";
let prefs: { view: "grid" | "list"; sort: SortKey; desc: boolean } = { view: "grid", sort: "name", desc: false };
try {
  prefs = { ...prefs, ...JSON.parse(localStorage.getItem(VIEW_KEY) ?? "{}") };
} catch {
  /* Standard */
}
const savePrefs = () => {
  try {
    localStorage.setItem(VIEW_KEY, JSON.stringify(prefs));
  } catch {
    /* nicht gemerkt */
  }
};
const services = (c: Connection) => [c.rdpPort && "RDP", c.vncPort && "VNC", c.sshPort && "SSH", c.ftpPort && "FTP"].filter(Boolean) as string[];
const byName = (a: Connection, b: Connection) => a.name.localeCompare(b.name, "de-CH", { numeric: true });
const SORT: Record<SortKey, (a: Connection, b: Connection) => number> = {
  name: byName,
  address: (a, b) => a.address.localeCompare(b.address, "de-CH", { numeric: true }),
  services: (a, b) => services(a).join(" ").localeCompare(services(b).join(" ")),
  // Ohne Gruppe zuletzt (aufsteigend)
  group: (a, b) => (a.group || "￿").localeCompare(b.group || "￿", "de-CH"),
  lastUsed: (a, b) => (Date.parse(a.lastUsed ?? "") || 0) - (Date.parse(b.lastUsed ?? "") || 0),
};

/** «vor 3 Tagen», «gestern», «Nie». */
function ago(iso: string | null) {
  const t = Date.parse(iso ?? "");
  if (!t) return "Nie";
  const s = (t - Date.now()) / 1000;
  const rtf = new Intl.RelativeTimeFormat("de-CH", { numeric: "auto" });
  const units = [["year", 31536000], ["month", 2592000], ["week", 604800], ["day", 86400], ["hour", 3600], ["minute", 60]] as const;
  for (const [u, n] of units) if (Math.abs(s) >= n) return rtf.format(Math.round(s / n), u);
  return "gerade eben";
}

const el = (tag: string, props: Record<string, string> = {}, ...kids: (Node | string)[]) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) e.setAttribute(k, v);
  e.append(...kids);
  return e;
};

let startLinkDone = false;
async function reload() {
  try {
    [items, groups] = await Promise.all([api.list(), api.groupsList()]);
  } catch (e) {
    tell("Bibliothek nicht lesbar", String(e));
  }
  const f = filter;
  if (f.kind === "group" && !groups.some((g) => g.name === f.name)) filter = { kind: "all" };
  render();
  if (!startLinkDone && !detachedWindow) {
    startLinkDone = true;
    deepLinkCurrent().then((urls) => urls?.forEach(handleLink)).catch(() => {});
  }
}

function render() {
  const navItem = (label: string, count: number, on: boolean, click: () => void, group?: string, color = "") => {
    const a = el("button", { type: "button", class: "nav-item", "aria-current": on ? "page" : "false" }, el("span", {}, groupDot(color), label), el("span", { class: "count" }, String(count)));
    a.onclick = click;
    if (group !== undefined) a.dataset.group = group;
    return a;
  };
  const all = navItem("Alle", items.length, filter.kind === "all", () => ((filter = { kind: "all" }), render()), "");
  all.oncontextmenu = (e) => (e.preventDefault(), groupAction(all, null));
  const header = el("div", { class: "nav-header" }, "Gruppen");
  header.oncontextmenu = (e) => (e.preventDefault(), groupAction(all, null));
  $("nav").replaceChildren(
    all,
    ...(groups.length ? [header] : []),
    ...groups.map((g) => {
      const a = navItem(g.name, items.filter((c) => c.group === g.name).length, filter.kind === "group" && filter.name === g.name, () => ((filter = { kind: "group", name: g.name }), render()), g.name, g.color);
      a.title = "Rechtsklick: Gruppe bearbeiten · Ziehen: Reihenfolge ändern";
      a.onpointerdown = (e) => dragGroup(e, a, g.name);
      a.oncontextmenu = (e) => (e.preventDefault(), groupAction(a, g));
      return a;
    }),
  );
  $("groups").replaceChildren(...groups.map((g) => el("option", { value: g.name })));
  $("title").textContent = filter.kind === "all" ? "Alle Verbindungen" : filter.name;

  const q = ($<HTMLInputElement>("search").value || "").toLowerCase();
  const dir = prefs.desc ? -1 : 1;
  const visible = items
    .filter((c) => filter.kind === "all" || c.group === filter.name)
    .filter((c) => !q || `${c.name} ${c.address} ${c.group}`.toLowerCase().includes(q))
    .sort(prefs.view === "list" ? (a, b) => SORT[prefs.sort](a, b) * dir || byName(a, b) : byName);
  shown = visible;
  // Fokus nach dem Neuzeichnen auf der Auswahl halten (Pfeiltasten, Klick).
  const refocus = $("grid").contains(document.activeElement) || $("conn-rows").contains(document.activeElement);
  const list = prefs.view === "list";
  for (const r of document.querySelectorAll<HTMLInputElement>("[name=libView]")) r.checked = r.value === prefs.view;
  $("grid").hidden = list;
  $("conn-table").hidden = !list || !items.length;
  if (list) renderTable(visible);
  else $("conn-rows").replaceChildren();

  $("grid").replaceChildren(
    ...(list ? [] : visible).map((c) => {
      const proto = desktopProtocol(c);
      const kind = proto ? proto.toUpperCase() : c.sshPort ? "Shell" : c.ftpPort ? "FTP" : "–";
      const card = el(
        "button",
        { type: "button", class: "card", "aria-pressed": String(c.id === selected) },
        thumbEl(c, kind),
        el("div", { class: "card-name" }, c.name),
        el("div", { class: "card-sub" }, c.address),
      );
      card.onclick = () => ((selected = c.id), render());
      card.onpointerdown = (e) => dragCard(e, card, c);
      card.ondblclick = () => open(c, c.defaultView);
      card.oncontextmenu = (e) => (e.preventDefault(), cardMenu(card, c));
      return card;
    }),
  );
  $("empty").hidden = items.length > 0;
  if (refocus) $(list ? "conn-rows" : "grid").querySelector<HTMLElement>(list ? "tr[tabindex='0']" : ".card[aria-pressed=true]")?.focus();
  renderDetails();
}

/** Listenansicht: eine Zeile je Verbindung; nur die ausgewählte (sonst die erste) ist per Tab erreichbar. */
function renderTable(visible: Connection[]) {
  for (const th of document.querySelectorAll<HTMLElement>("#conn-table th[data-key]"))
    th.setAttribute("aria-sort", th.dataset.key === prefs.sort ? (prefs.desc ? "descending" : "ascending") : "none");
  const tabId = visible.some((c) => c.id === selected) ? selected : visible[0]?.id;
  const svcIcon: Record<string, string> = { RDP: ICON.desktop, VNC: ICON.screencast, SSH: ICON.shell, FTP: ICON.globe };
  const port: Record<string, (c: Connection) => number | null> = { RDP: (c) => c.rdpPort, VNC: (c) => c.vncPort, SSH: (c) => c.sshPort, FTP: (c) => c.ftpPort };
  $("conn-rows").replaceChildren(
    ...visible.map((c) => {
      const svc = el("td", { class: "svc-col" });
      for (const s of services(c)) {
        const i = el("span", { class: "svc-icon", role: "img", "aria-label": s, title: `${s} · Port ${port[s](c)}` });
        i.innerHTML = svcIcon[s];
        svc.append(i);
      }
      const grp = el("td", {}, ...(c.group ? [groupDot(groups.find((g) => g.name === c.group)?.color ?? ""), c.group] : ["–"]));
      const date = el("td", { class: "date-col" }, ago(c.lastUsed));
      if (c.lastUsed) date.title = new Date(c.lastUsed).toLocaleString("de-CH");
      const tr = el("tr", { "aria-selected": String(c.id === selected) }, el("td", { class: "name" }, c.name), el("td", { class: "mono" }, c.address), svc, grp, date);
      tr.tabIndex = c.id === tabId ? 0 : -1;
      tr.onclick = () => ((selected = c.id), render());
      tr.onpointerdown = (e) => dragCard(e, tr, c);
      tr.ondblclick = () => open(c, c.defaultView);
      tr.oncontextmenu = (e) => (e.preventDefault(), cardMenu(tr, c));
      return tr;
    }),
    ...(visible.length ? [] : [el("tr", {}, el("td", { class: "empty", colspan: "5" }, "Keine Verbindungen gefunden."))]),
  );
}

// Liste: Spaltenköpfe sortieren (Knopf: Enter/Leertaste), Pfeiltasten wählen, Enter verbindet.
for (const th of document.querySelectorAll<HTMLElement>("#conn-table th[data-key]")) {
  th.querySelector("button")!.onclick = () => {
    const k = th.dataset.key as SortKey;
    prefs.desc = prefs.sort === k ? !prefs.desc : k === "lastUsed"; // Neueste zuerst
    prefs.sort = k;
    savePrefs();
    render();
  };
}
$("conn-rows").onkeydown = (e) => {
  const rows = [...$<HTMLTableSectionElement>("conn-rows").rows];
  const i = rows.indexOf(document.activeElement as HTMLTableRowElement);
  const c = shown[i];
  if (i < 0 || !c) return;
  const j = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: shown.length - 1, PageDown: i + 10, PageUp: i - 10 }[e.key];
  if (j !== undefined) selected = shown[Math.max(0, Math.min(shown.length - 1, j))].id;
  else if (e.key === "Enter") open(c, c.defaultView);
  else if (e.key === " ") selected = c.id;
  else return;
  e.preventDefault();
  render();
};
for (const r of document.querySelectorAll<HTMLInputElement>("[name=libView]")) r.onchange = () => ((prefs.view = r.value as typeof prefs.view), savePrefs(), render());
for (const s of document.querySelectorAll<HTMLElement>(".view-switch .seg-icon")) s.innerHTML = ICON[s.dataset.icon as keyof typeof ICON];

/** Kontextmenü der Seitenleiste (Gruppe oder «Neue Gruppe»). */
async function groupAction(anchor: HTMLElement, g: Group | null) {
  const count = g ? items.filter((c) => c.group === g.name).length : 0;
  const r = await groupMenu(anchor, g, count, g ? groups.indexOf(g) : -1, groups.length);
  if (r !== null) afterGroupChange(r || g?.name || null, g?.name);
}

/** Nach einer Gruppenänderung neu laden; die Auswahl in der Seitenleiste folgt einer umbenannten Gruppe. */
async function afterGroupChange(name: string | null, from?: string) {
  const followed = filter.kind === "group" && filter.name === from;
  await reload();
  if (followed && name && from && !groups.some((g) => g.name === from) && groups.some((g) => g.name === name)) {
    filter = { kind: "group", name };
    render();
  }
  if (document.querySelector("dialog[open]")) return;
  const navs = [...$("nav").querySelectorAll<HTMLElement>(".nav-item[data-group]")];
  (navs.find((n) => n.dataset.group === name) ?? navs[0])?.focus();
}
addEventListener("sesam:groups-changed", (e) => {
  const d = (e as CustomEvent<{ from?: string; to?: string }>).detail;
  afterGroupChange(d.to ?? null, d.from);
});
initGroups();

function renderDetails() {
  const c = items.find((x) => x.id === selected);
  const aside = $("details");
  aside.hidden = !c;
  if (!c) return;
  const fact = (k: string, v: string) => el("div", {}, el("dt", {}, k), el("dd", {}, v));
  const btn = (label: string, cls: string, on: () => void, disabled = false) => {
    const b = el("button", { type: "button", class: cls }, label) as HTMLButtonElement;
    b.disabled = disabled;
    b.onclick = on;
    return b;
  };
  aside.replaceChildren(
    el("div", { class: "detail-title" }, c.name),
    el("div", { class: "sub" }, c.group || "Ohne Gruppe"),
    el(
      "dl",
      {},
      fact("Adresse", c.address),
      fact("Benutzer", c.user || "–"),
      fact("Desktop", desktopProtocol(c) === "rdp" ? `RDP · ${c.rdpPort}` : c.vncPort ? `VNC · ${c.vncPort}` : "aus"),
      fact("Shell", c.sshPort ? `SSH · ${c.sshPort}` : "aus"),
      fact("Dateien", filesProtocol(c) === "ftp" ? `${ftpLabel(c)} · ${c.ftpPort}` : c.sshPort ? `SFTP · ${c.sshPort}` : "aus"),
      fact("Anmeldung", c.hasPassword || c.hasKey || c.hasVncPassword || c.hasRdpPassword ? "Im Tresor" : "Keine gespeichert"),
      fact("Schlüssel", c.hasKey ? "Im Tresor" : "–"),
    ),
    btn("Desktop öffnen", "primary", () => open(c, "desktop"), !desktopProtocol(c)),
    el(
      "div",
      { class: "pair" },
      btn("Shell", "", () => open(c, "shell"), !c.sshPort),
      btn("Dateien", "", () => open(c, "files"), !filesProtocol(c)),
    ),
    btn("Bearbeiten …", "plain", () => edit(c)),
    ...(c.sshPort ? [btn("RDP auf Zielrechner einrichten …", "plain", () => runOnTarget(c))] : []),
  );
}

/** Verbindung auf eine Gruppe (Seitenleiste) ziehen = Gruppe wechseln. «Alle» = ohne Gruppe.
 *  Eigene Zeiger-Logik, weil Tauri HTML-Drag&Drop für Systemdateien abfängt. */
function dragCard(e: PointerEvent, card: HTMLElement, c: Connection) {
  pointerDrag(
    e,
    card,
    c.name,
    (m) => {
      const t = document.elementFromPoint(m.clientX, m.clientY)?.closest<HTMLElement>(".nav-item[data-group]");
      return t ? { el: t, cls: "drop" } : null;
    },
    async (hit) => {
      const group = hit?.el.dataset.group;
      if (group === undefined || group === c.group) return;
      try {
        await api.save({ ...c, group }, {});
        reload();
      } catch (err) {
        tell("Nicht verschoben", String(err));
      }
    },
  );
}

/** Gruppe in der Seitenleiste ziehen = Reihenfolge ändern (Tastatur: Kontextmenü «Nach oben/unten»). */
function dragGroup(e: PointerEvent, item: HTMLElement, name: string) {
  pointerDrag(
    e,
    item,
    name,
    (m) => {
      const t = document.elementFromPoint(m.clientX, m.clientY)?.closest<HTMLElement>(".nav-item[data-group]");
      if (!t?.dataset.group || t === item) return null;
      const r = t.getBoundingClientRect();
      return { el: t, cls: m.clientY < r.top + r.height / 2 ? "insert-before" : "insert-after" };
    },
    async (hit) => {
      if (!hit) return;
      const names = groups.map((g) => g.name);
      const target = hit.el.dataset.group!;
      const before = hit.cls === "insert-before" ? target : (names[names.indexOf(target) + 1] ?? null);
      if (await moveGroupBefore(name, before)) afterGroupChange(name);
    },
  );
}

/** Eigene Zeiger-Logik für Ziehen und Ablegen; `pick` liefert das Ziel unter dem Zeiger und dessen Markierung. */
function pointerDrag(
  e: PointerEvent,
  src: HTMLElement,
  label: string,
  pick: (m: PointerEvent) => { el: HTMLElement; cls: string } | null,
  drop: (hit: { el: HTMLElement; cls: string } | null) => void,
) {
  if (e.button !== 0) return;
  const [sx, sy] = [e.clientX, e.clientY];
  let ghost: HTMLElement | null = null;
  let hit: { el: HTMLElement; cls: string } | null = null;
  const move = (m: PointerEvent) => {
    if (!ghost && Math.hypot(m.clientX - sx, m.clientY - sy) < 6) return;
    if (!ghost) {
      src.setPointerCapture(m.pointerId);
      ghost = el("div", { class: "drag-ghost" }, label);
      document.body.append(ghost);
    }
    ghost.style.transform = `translate(${m.clientX + 12}px, ${m.clientY + 8}px)`;
    const h = pick(m);
    if (h?.el !== hit?.el || h?.cls !== hit?.cls) {
      hit?.el.classList.remove(hit.cls);
      hit = h;
      hit?.el.classList.add(hit.cls);
    }
  };
  const up = () => {
    src.removeEventListener("pointermove", move);
    src.removeEventListener("pointerup", up);
    if (!ghost) return;
    ghost.remove();
    hit?.el.classList.remove(hit.cls);
    // Nach dem Ziehen keinen Klick auslösen
    src.addEventListener("click", (ev) => ev.stopImmediatePropagation(), { capture: true, once: true });
    drop(hit);
  };
  src.addEventListener("pointermove", move);
  src.addEventListener("pointerup", up);
}

/** Kontextmenü einer Verbindung in der Bibliothek. */
async function cardMenu(anchor: HTMLElement, c: Connection) {
  const v = await openMenu(anchor, [
    { value: "open:desktop", label: "Desktop öffnen", disabled: !desktopProtocol(c) },
    { value: "open:shell", label: "Shell öffnen", disabled: !c.sshPort },
    { value: "open:files", label: "Dateien öffnen", disabled: !filesProtocol(c) },
    { value: "edit", label: "Bearbeiten …", separator: true },
    { value: "lnk:desktop", label: "Desktop", section: isMac ? "Verknüpfung auf dem Schreibtisch" : "Verknüpfung auf dem Desktop", disabled: !desktopProtocol(c) },
    { value: "lnk:shell", label: "Shell", disabled: !c.sshPort },
    { value: "lnk:files", label: "Dateien", disabled: !filesProtocol(c) },
  ]);
  if (!v) return;
  const [cmd, arg] = v.split(":");
  if (cmd === "open") open(c, arg as Connection["defaultView"]);
  if (cmd === "edit") edit(c);
  if (cmd === "lnk")
    invoke<string>("create_shortcut", { id: c.id, view: arg })
      .then((p) => tell("Verknüpfung erstellt", p))
      .catch((e) => tell("Verknüpfung nicht erstellt", String(e)));
}

/** sesam://open?id=…&view=… (Desktop-Verknüpfung) */
function handleLink(url: string) {
  try {
    const u = new URL(url);
    if (u.protocol !== "sesam:") return;
    const id = u.searchParams.get("id");
    const view = (u.searchParams.get("view") ?? "") as Connection["defaultView"];
    const c = items.find((x) => x.id === id);
    if (!c) return tell("Verbindung nicht gefunden", "Die Verknüpfung zeigt auf eine Verbindung, die es nicht mehr gibt.");
    open(c, ["desktop", "shell", "files"].includes(view) ? view : c.defaultView);
  } catch {
    /* kein gültiger Link */
  }
}

function open(c: Connection, view: Connection["defaultView"]) {
  openTab(c, view);
}

initTabs({ desktop: desktopTab, shell: shellTab, files: filesTab }, () => reload());
// Abgekoppeltes Fenster: nur die übergebene Sitzung, keine Bibliothek.
if (detachedWindow) {
  document.body.classList.add("detached");
  if (!takeOver(detachToken)) void import("@tauri-apps/api/window").then((w) => w.getCurrentWindow().destroy());
}

// ---------- Sheet «Verbindung bearbeiten» ----------

/** Reiter oben im Formular: je Abschnitt (h3.sec) eine Seite, damit nichts lange scrollt. */
const sectionTabs = document.createElement("div");
sectionTabs.className = "segmented sheet-tabs span";
sectionTabs.setAttribute("role", "tablist");
{
  const form = document.getElementById("editor-form")!;
  let sec = "";
  for (const n of [...form.children] as HTMLElement[]) {
    if (n.matches("h3.sec")) {
      sec = n.textContent!.trim();
      const b = document.createElement("button");
      b.type = "button";
      b.setAttribute("role", "tab");
      b.textContent = sec;
      b.onclick = () => showSection(b.textContent!);
      sectionTabs.append(b);
      n.hidden = true; // Titel steckt im Reiter
    } else if (sec && !n.matches(".actions, #editor-error")) n.dataset.sec = sec;
  }
  form.querySelector("h2")!.after(sectionTabs);
}
function showSection(name: string) {
  for (const b of sectionTabs.children) b.setAttribute("aria-selected", String(b.textContent === name));
  for (const n of document.querySelectorAll<HTMLElement>("#editor-form > [data-sec]")) n.classList.toggle("sec-hidden", n.dataset.sec !== name);
}

const dlg = $<HTMLDialogElement>("editor");
const form = $<HTMLFormElement>("editor-form");
let editing: Connection | null = null;

const blank = (): Connection => ({
  id: crypto.randomUUID(),
  name: "",
  group: filter.kind === "group" ? filter.name : "",
  address: "",
  vncPort: null,
  rdpPort: 3389,
  rdpDomain: "",
  rdpUser: "",
  rdpTarget: "",
  rdpSessionUser: "",
  desktopProtocol: "rdp",
  sshPort: 22,
  user: "",
  defaultView: "desktop",
  orientation: "landscape",
  sharedClipboard: true,
  playAudio: true,
  os: "",
  customThumb: false,
  credentialId: null,
  rdpCredentialId: null,
  vncCredentialId: null,
  gatewayCredentialId: null,
  sharedFolder: "",
  skipCertCheck: false,
  rdpNoNtlm: false,
  rdpKdc: "",
  rdpGateway: "",
  rdpGatewayUser: "",
  fitToWindow: true,
  hasPassword: false,
  hasVncPassword: false,
  hasRdpPassword: false,
  hasGatewayPassword: false,
  hasKey: false,
  fileTransfer: "auto",
  ftpPort: null,
  ftpSecurity: "explicit",
  ftpActive: false,
  filesVia: "ssh",
  sshLegacy: false,
  sshKeepalive: 0,
  sshCompression: false,
  forwards: [],
  jumpHost: "",
  sshAgent: false,
  agentForward: false,
  lastUsed: null,
});

function edit(c: Connection | null, asNew = false) {
  const isNew = !c || asNew;
  editing = c ?? blank();
  const f = form.elements as unknown as Record<string, HTMLInputElement>;
  $("editor-title").textContent = isNew ? "Neue Verbindung" : "Verbindung bearbeiten";
  f.name.value = editing.name;
  f.group.value = editing.group;
  f.address.value = editing.address;
  f.vncOn.checked = editing.vncPort !== null;
  f.vncPort.value = String(editing.vncPort ?? 5900);
  f.rdpOn.checked = editing.rdpPort !== null;
  f.rdpPort.value = String(editing.rdpPort ?? 3389);
  f.rdpDomain.value = editing.rdpDomain;
  f.rdpUser.value = editing.rdpUser ?? "";
  (formEls.namedItem("rdpTarget") as HTMLSelectElement).value = editing.rdpTarget ?? "";
  rdpTargetMenu.sync();
  f.rdpSessionUser.value = editing.rdpSessionUser ?? "";
  $("rdp-check-result").textContent = "";
  f.rdpOwn.checked = editing.hasRdpPassword || !!editing.rdpUser || !!editing.rdpCredentialId;
  f.vncOwn.checked = editing.hasVncPassword || !!editing.vncCredentialId;
  syncRdpTarget();
  f.rdpPassword.value = "";
  form.querySelector<HTMLInputElement>(`[name=desktopProtocol][value=${editing.desktopProtocol}]`)!.checked = true;
  f.sshOn.checked = editing.sshPort !== null;
  f.sshPort.value = String(editing.sshPort ?? 22);
  f.ftpOn.checked = editing.ftpPort !== null;
  f.ftpPort.value = String(editing.ftpPort ?? 21);
  f.ftpSecurity.value = editing.ftpSecurity ?? "explicit";
  ftpSecurityMenu.sync();
  form.querySelector<HTMLInputElement>(`[name=ftpMode][value=${editing.ftpActive ? "active" : "passive"}]`)!.checked = true;
  form.querySelector<HTMLInputElement>(`[name=filesVia][value=${editing.filesVia ?? "ssh"}]`)!.checked = true;
  f.user.value = editing.user;
  f.keyMode.value = editing.hasKey ? "keep" : "none";
  keyModeMenu.sync();
  f.key.value = "";
  f.password.value = "";
  f.vncPassword.value = "";
  clearing.clear();
  showPwHint("vnc-pw", editing.hasVncPassword, "Nur bei klassischem VNC ohne Benutzername.");
  showPwHint("rdp-pw", editing.hasRdpPassword, "Leer = Passwort oben.");
  syncDesktopRow();
  showPwHint("pw", editing.hasPassword, "");
  f.sharedClipboard.checked = editing.sharedClipboard;
  f.playAudio.checked = editing.playAudio;
  f.sharedFolder.value = editing.sharedFolder;
  f.skipCertCheck.checked = editing.skipCertCheck;
  (form.elements.namedItem("os") as HTMLSelectElement).value = editing.os ?? "";
  fillCreds(editing.credentialId ?? null);
  for (const p of svcPickers) void p.fill(editing[p.name] ?? null);
  osMenu.sync();
  pendingThumb = null;
  clearThumb = false;
  $("thumb-name").textContent = editing.customThumb ? "Eigenes Bild" : "Letzter Desktop";
  renderThumb($("thumb-preview"), editing);
  showSection("Allgemein");
  f.rdpNoNtlm.checked = editing.rdpNoNtlm;
  f.rdpKdc.value = editing.rdpKdc;
  f.rdpGateway.value = editing.rdpGateway;
  f.rdpGatewayUser.value = editing.rdpGatewayUser;
  f.gatewayPassword.value = "";
  showPwHint("gw-pw", editing.hasGatewayPassword, "Nur mit eigenem Gateway-Benutzer.");
  f.fileTransfer.value = editing.fileTransfer ?? "auto";
  fileTransferMenu.sync();
  f.sshKeepalive.value = editing.sshKeepalive ? String(editing.sshKeepalive) : "";
  f.sshCompression.checked = editing.sshCompression;
  f.sshLegacy.checked = editing.sshLegacy;
  f.forwards.value = (editing.forwards ?? []).join("\n");
  f.sshAgent.checked = editing.sshAgent;
  f.agentForward.checked = editing.agentForward;
  // Sprungserver: andere Verbindungen mit SSH.
  const self = editing.id;
  jumpHost.replaceChildren(
    el("option", { value: "" }, "Keiner, direkt verbinden"),
    ...items.filter((x) => x.id !== self && x.sshPort).map((x) => el("option", { value: x.id }, `${x.name} (${x.address})`)),
  );
  jumpHost.value = editing.jumpHost ?? "";
  jumpHostMenu.sync();
  f.fitToWindow.checked = editing.fitToWindow;
  form.querySelector<HTMLInputElement>(`[name=orientation][value=${editing.orientation}]`)!.checked = true;
  form.querySelector<HTMLInputElement>(`[name=defaultView][value=${editing.defaultView}]`)!.checked = true;
  $("key-row").hidden = true;
  $("editor-delete").hidden = isNew;
  $("editor-error").textContent = "";
  dlg.showModal();
}

/** Gespeicherte Passwörter, die beim Sichern entfernt werden (Feldname). */
const clearing = new Set<string>();

function showPwHint(prefix: "pw" | "vnc-pw" | "rdp-pw" | "gw-pw", stored: boolean, emptyText: string) {
  const field = ({ pw: "password", "vnc-pw": "vncPassword", "rdp-pw": "rdpPassword", "gw-pw": "gatewayPassword" } as const)[prefix];
  const hint = $(`${prefix}-hint`);
  const btn = $(`${prefix}-clear`);
  hint.textContent = stored ? "Verschlüsselt im Tresor gespeichert. Leer lassen, um es zu behalten." : emptyText;
  btn.hidden = !stored;
  btn.onclick = () => {
    clearing.add(field);
    hint.textContent = "Wird beim Sichern entfernt.";
    btn.hidden = true;
  };
}

/** «Desktop über» nur zeigen, wenn VNC und RDP beide an sind; «Dateien über» bei SSH und FTP. */
function syncDesktopRow() {
  const on = (n: string) => (form.elements.namedItem(n) as HTMLInputElement).checked;
  $("desktop-protocol-row").hidden = !(on("vncOn") && on("rdpOn"));
  $("files-via-row").hidden = !(on("sshOn") && on("ftpOn"));
}
for (const n of ["vncOn", "rdpOn", "sshOn", "ftpOn"]) (form.elements.namedItem(n) as HTMLInputElement).addEventListener("change", syncDesktopRow);
// Implizites FTPS läuft meist auf 990, die anderen auf 21.
ftpSecurity.addEventListener("change", () => {
  const p = formEls.namedItem("ftpPort") as HTMLInputElement;
  if (ftpSecurity.value === "implicit" && p.value === "21") p.value = "990";
  else if (ftpSecurity.value !== "implicit" && p.value === "990") p.value = "21";
});

let pendingThumb: string | null = null;
let clearThumb = false;
$("thumb-menu").onclick = async () => {
  const v = await openMenu($("thumb-menu"), [
    { value: "pick", label: "Eigenes Bild wählen …" },
    { value: "clear", label: "Letzter Desktop (Standard)" },
  ]);
  if (v === "pick") pickThumb();
  if (v === "clear") ((pendingThumb = null), (clearThumb = true), ($("thumb-name").textContent = "Letzter Desktop"));
};
const pickThumb = async () => {
  const f = await pickFolder({ title: "Bild für die Verbindung", filters: [{ name: "Bilder", extensions: ["png", "jpg", "jpeg", "webp"] }] });
  if (typeof f === "string") ((pendingThumb = f), ($("thumb-name").textContent = f.split(/[\\/]/).pop() ?? f));
};

const sharedFolder = form.elements.namedItem("sharedFolder") as HTMLInputElement;
$("share-pick").onclick = async () => {
  const dir = await pickFolder({ directory: true, title: "Ordner für das entfernte Laufwerk" });
  if (typeof dir === "string") sharedFolder.value = dir;
};
$("share-clear").onclick = () => (sharedFolder.value = "");

const keyMode = form.elements.namedItem("keyMode") as HTMLSelectElement;
keyMode.addEventListener("change", () => ($("key-row").hidden = keyMode.value !== "import"));
$("editor-cancel").onclick = () => dlg.close();

$("editor-delete").onclick = async () => {
  if (!editing || !(await ask("Verbindung löschen?", `«${editing.name}» und die gespeicherten Zugangsdaten werden entfernt.`, "Löschen"))) return;
  try {
    await api.remove(editing.id);
    selected = null;
    dlg.close();
    reload();
  } catch (e) {
    $("editor-error").textContent = String(e);
  }
};

form.onsubmit = async (ev) => {
  ev.preventDefault();
  if (!editing) return;
  const fd = new FormData(form);
  const s = (k: string) => String(fd.get(k) ?? "").trim();
  const port = (on: string, k: string) => (fd.get(on) ? Number(s(k)) || null : null);
  const ed = editing;
  const c: Connection = {
    ...editing,
    name: s("name"),
    group: s("group"),
    address: s("address"),
    vncPort: port("vncOn", "vncPort"),
    sshPort: port("sshOn", "sshPort"),
    rdpPort: port("rdpOn", "rdpPort"),
    rdpDomain: s("rdpDomain"),
    rdpUser: fd.get("rdpOwn") ? s("rdpUser") : "",
    rdpTarget: s("rdpTarget") as Connection["rdpTarget"],
    rdpSessionUser: s("rdpTarget") === "session" ? s("rdpSessionUser") : "",
    // Gespeicherte Zugangsdaten pro Dienst (nur wenn der Schalter des Dienstes ein ist).
    rdpCredentialId: fd.get("rdpOwn") ? svcPickers[0].value() : null,
    vncCredentialId: fd.get("vncOwn") ? svcPickers[1].value() : null,
    gatewayCredentialId: svcPickers[2].value(),
    desktopProtocol: (s("desktopProtocol") || "rdp") as Connection["desktopProtocol"],
    user: s("user"),
    sharedClipboard: fd.get("sharedClipboard") !== null,
    playAudio: fd.get("playAudio") !== null,
    sharedFolder: s("sharedFolder"),
    skipCertCheck: fd.get("skipCertCheck") !== null,
    os: s("os") as Connection["os"],
    credentialId: s("credentialId") || null,
    customThumb: pendingThumb ? true : clearThumb ? false : editing.customThumb,
    rdpNoNtlm: fd.get("rdpNoNtlm") !== null,
    rdpKdc: s("rdpKdc"),
    rdpGateway: s("rdpGateway"),
    rdpGatewayUser: s("rdpGatewayUser"),
    fileTransfer: (s("fileTransfer") || "auto") as Connection["fileTransfer"],
    ftpPort: port("ftpOn", "ftpPort"),
    ftpSecurity: (s("ftpSecurity") || "explicit") as Connection["ftpSecurity"],
    ftpActive: s("ftpMode") === "active",
    filesVia: (s("filesVia") || "ssh") as Connection["filesVia"],
    sshKeepalive: Math.max(0, Math.min(3600, Math.round(Number(s("sshKeepalive")) || 0))),
    sshCompression: fd.get("sshCompression") !== null,
    sshLegacy: fd.get("sshLegacy") !== null,
    forwards: s("forwards")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean),
    jumpHost: s("jumpHost"),
    sshAgent: fd.get("sshAgent") !== null,
    agentForward: fd.get("agentForward") !== null,
    fitToWindow: fd.get("fitToWindow") !== null,
    orientation: s("orientation") as Connection["orientation"],
    defaultView: s("defaultView") as Connection["defaultView"],
  };
  const keyMode = s("keyMode");
  const sshKey = keyMode === "none" ? "" : keyMode === "import" ? String(fd.get("key") ?? "").trim() + "\n" : undefined;
  if (keyMode === "import" && sshKey!.trim() === "") return void ($("editor-error").textContent = "Bitte den privaten Schlüssel einfügen.");
  // Neuer Wert > «Entfernen» (leerer String löscht im Tresor) > unverändert.
  // Schalter «Eigene Zugangsdaten» aus: gespeichertes Dienst-Passwort entfernen.
  const own: Record<string, string> = { rdpPassword: "rdpOwn", vncPassword: "vncOwn" };
  const secret = (k: string) =>
    own[k] && !fd.get(own[k]) ? ((k === "rdpPassword" ? ed.hasRdpPassword : ed.hasVncPassword) ? "" : undefined) : String(fd.get(k) ?? "") || (clearing.has(k) ? "" : undefined);
  try {
    const saved = await api.save(c, { password: secret("password"), vncPassword: secret("vncPassword"), rdpPassword: secret("rdpPassword"), gatewayPassword: secret("gatewayPassword"), sshKey });
    if (pendingThumb) await invoke("thumb_import", { id: saved.id, file: pendingThumb }).catch((e) => console.warn(e));
    if (pendingThumb || clearThumb) forgetThumb(saved.id);
    selected = saved.id;
    dlg.close();
    reload();
  } catch (e) {
    $("editor-error").textContent = String(e);
  }
};

$("new").onclick = () => edit(null);
if (!detachedWindow) onOpenUrl((urls) => urls.forEach(handleLink)).catch(() => {});
for (const b of document.querySelectorAll<HTMLElement>("[data-help]")) b.onclick = () => openHelp(b.dataset.help!);
listen<string>("help", (e) => openHelp(e.payload));
initTitlebar({ command: (c) => (!detachedWindow || c === "settings") && command(c), tool: runTool, help: openHelp });
applyAppearance();
$("open-settings").onclick = () => openSettings();
// Befehle aus dem App-Menü und Tastenkürzel
const quick = initQuick((c) => edit({ ...c, id: crypto.randomUUID(), name: c.address }, true));
$("empty-new").onclick = () => edit(null);
const ubuntu = initUbuntu(blank, () => reload());
initTerminal();
$("empty-ubuntu").onclick = () => ubuntu.open();
initFieldHelp(document.getElementById("editor-form")!);
initFieldHelp(document.getElementById("quick-form")!);
const command = (cmd: string) => {
  if (cmd === "settings") openSettings();
  else if (cmd === "about") openSettings("about");
  else if (cmd === "addubuntu") ubuntu.open();
  else if (cmd === "terminal") void openTerminal();
  else if (cmd === "new") edit(null);
  else if (cmd === "quick") quick.open();
};
addEventListener("sesam:save-quick", (e) => quick.saveAs((e as CustomEvent<Connection>).detail));
// Menübefehle gehen an alle Fenster; nur das aktive reagiert.
listen<string>("cmd", (e) => document.hasFocus() && (!detachedWindow || e.payload === "settings") && command(e.payload));
listen<string>("tool", (e) => document.hasFocus() && runTool(e.payload));
addEventListener("keydown", (e) => {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
  if (e.key === ",") (e.preventDefault(), command("settings"));
  else if (e.key.toLowerCase() === "n" && !e.shiftKey && !detachedWindow && !document.querySelector("dialog[open]")) (e.preventDefault(), command("new"));
});
$("search").addEventListener("input", render);
reload();
