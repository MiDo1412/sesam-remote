import { api, desktopProtocol, type Connection } from "./api";
import { shellTab } from "./shell";
import { desktopTab } from "./desktop";
import { filesTab } from "./files";
import { initTabs, openTab, runTool } from "./tabs";
import { ask, tell } from "./ui";
import { openHelp } from "./help";
import { applyAppearance, openSettings } from "./settings";
import { initQuick } from "./quick";
import { runOnTarget } from "./setup";
import { initFieldHelp } from "./fieldhelp";
import { forgetThumb, renderThumb } from "./thumbs";
import { adoptFromConnection, initCreds } from "./creds";
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
const keyModeMenu = enhanceSelect(formEls.namedItem("keyMode") as HTMLSelectElement);
const fileTransferMenu = enhanceSelect(formEls.namedItem("fileTransfer") as HTMLSelectElement);
const jumpHost = formEls.namedItem("jumpHost") as HTMLSelectElement;
const jumpHostMenu = enhanceSelect(jumpHost);
const osMenu = enhanceSelect(formEls.namedItem("os") as HTMLSelectElement);

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

let items: Connection[] = [];
let filter: { kind: "all" } | { kind: "group"; name: string } = { kind: "all" };
let selected: string | null = null;

const el = (tag: string, props: Record<string, string> = {}, ...kids: (Node | string)[]) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) e.setAttribute(k, v);
  e.append(...kids);
  return e;
};

let startLinkDone = false;
async function reload() {
  try {
    items = await api.list();
  } catch (e) {
    tell("Bibliothek nicht lesbar", String(e));
  }
  render();
  if (!startLinkDone) {
    startLinkDone = true;
    deepLinkCurrent().then((urls) => urls?.forEach(handleLink)).catch(() => {});
  }
}

function render() {
  const groups = [...new Set(items.map((c) => c.group).filter(Boolean))].sort((a, b) => a.localeCompare(b, "de-CH"));
  const navItem = (label: string, count: number, on: boolean, click: () => void, group?: string) => {
    const a = el("button", { type: "button", class: "nav-item", "aria-current": on ? "page" : "false" }, el("span", {}, label), el("span", { class: "count" }, String(count)));
    a.onclick = click;
    if (group !== undefined) a.dataset.group = group;
    return a;
  };
  $("nav").replaceChildren(
    navItem("Alle", items.length, filter.kind === "all", () => ((filter = { kind: "all" }), render()), ""),
    ...(groups.length ? [el("div", { class: "nav-header" }, "Gruppen")] : []),
    ...groups.map((g) =>
      navItem(g, items.filter((c) => c.group === g).length, filter.kind === "group" && filter.name === g, () => ((filter = { kind: "group", name: g }), render()), g),
    ),
  );
  $("groups").replaceChildren(...groups.map((g) => el("option", { value: g })));
  $("title").textContent = filter.kind === "all" ? "Alle Verbindungen" : filter.name;

  const q = ($<HTMLInputElement>("search").value || "").toLowerCase();
  const visible = items
    .filter((c) => filter.kind === "all" || c.group === filter.name)
    .filter((c) => !q || `${c.name} ${c.address} ${c.group}`.toLowerCase().includes(q))
    .sort((a, b) => a.name.localeCompare(b.name, "de-CH"));

  $("grid").replaceChildren(
    ...visible.map((c) => {
      const proto = desktopProtocol(c);
      const kind = proto ? proto.toUpperCase() : c.sshPort ? "Shell" : "–";
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
  renderDetails();
}

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
      fact("Anmeldung", c.hasPassword || c.hasKey || c.hasVncPassword || c.hasRdpPassword ? "Im Tresor" : "Keine gespeichert"),
      fact("Schlüssel", c.hasKey ? "Im Tresor" : "–"),
    ),
    btn("Desktop öffnen", "primary", () => open(c, "desktop"), !desktopProtocol(c)),
    el(
      "div",
      { class: "pair" },
      btn("Shell", "", () => open(c, "shell"), !c.sshPort),
      btn("Dateien", "", () => open(c, "files"), !c.sshPort),
    ),
    btn("Bearbeiten …", "plain", () => edit(c)),
    ...(c.sshPort ? [btn("RDP auf Zielrechner einrichten …", "plain", () => runOnTarget(c))] : []),
  );
}

/** Verbindung auf eine Gruppe (Seitenleiste) ziehen = Gruppe wechseln. «Alle» = ohne Gruppe.
 *  Eigene Zeiger-Logik, weil Tauri HTML-Drag&Drop für Systemdateien abfängt. */
function dragCard(e: PointerEvent, card: HTMLElement, c: Connection) {
  if (e.button !== 0) return;
  const [sx, sy] = [e.clientX, e.clientY];
  let ghost: HTMLElement | null = null;
  let target: HTMLElement | null = null;
  const move = (m: PointerEvent) => {
    if (!ghost && Math.hypot(m.clientX - sx, m.clientY - sy) < 6) return;
    if (!ghost) {
      card.setPointerCapture(m.pointerId);
      ghost = el("div", { class: "drag-ghost" }, c.name);
      document.body.append(ghost);
    }
    ghost.style.transform = `translate(${m.clientX + 12}px, ${m.clientY + 8}px)`;
    const hit = document.elementFromPoint(m.clientX, m.clientY)?.closest<HTMLElement>(".nav-item[data-group]") ?? null;
    if (hit !== target) {
      target?.classList.remove("drop");
      target = hit;
      target?.classList.add("drop");
    }
  };
  const up = async () => {
    card.removeEventListener("pointermove", move);
    card.removeEventListener("pointerup", up);
    if (!ghost) return;
    ghost.remove();
    target?.classList.remove("drop");
    // Nach dem Ziehen keinen Klick auslösen
    card.addEventListener("click", (ev) => ev.stopImmediatePropagation(), { capture: true, once: true });
    const group = target?.dataset.group;
    if (group === undefined || group === c.group) return;
    try {
      await api.save({ ...c, group }, {});
      reload();
    } catch (err) {
      tell("Nicht verschoben", String(err));
    }
  };
  card.addEventListener("pointermove", move);
  card.addEventListener("pointerup", up);
}

/** Kontextmenü einer Verbindung in der Bibliothek. */
async function cardMenu(anchor: HTMLElement, c: Connection) {
  const v = await openMenu(anchor, [
    { value: "open:desktop", label: "Desktop öffnen", disabled: !desktopProtocol(c) },
    { value: "open:shell", label: "Shell öffnen", disabled: !c.sshPort },
    { value: "open:files", label: "Dateien öffnen", disabled: !c.sshPort },
    { value: "edit", label: "Bearbeiten …", separator: true },
    { value: "lnk:desktop", label: "Desktop", section: "Verknüpfung auf Schreibtisch", disabled: !desktopProtocol(c) },
    { value: "lnk:shell", label: "Shell", disabled: !c.sshPort },
    { value: "lnk:files", label: "Dateien", disabled: !c.sshPort },
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

// ---------- Sheet «Verbindung bearbeiten» ----------

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
  f.rdpPassword.value = "";
  form.querySelector<HTMLInputElement>(`[name=desktopProtocol][value=${editing.desktopProtocol}]`)!.checked = true;
  f.sshOn.checked = editing.sshPort !== null;
  f.sshPort.value = String(editing.sshPort ?? 22);
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
  osMenu.sync();
  pendingThumb = null;
  clearThumb = false;
  $("thumb-name").textContent = editing.customThumb ? "Eigenes Bild" : "Letzter Desktop";
  f.rdpNoNtlm.checked = editing.rdpNoNtlm;
  f.rdpKdc.value = editing.rdpKdc;
  f.rdpGateway.value = editing.rdpGateway;
  f.rdpGatewayUser.value = editing.rdpGatewayUser;
  f.gatewayPassword.value = "";
  showPwHint("gw-pw", editing.hasGatewayPassword, "Nur mit eigenem Gateway-Benutzer.");
  ($("rdp-advanced") as HTMLDetailsElement).open = editing.rdpNoNtlm || !!editing.rdpKdc || !!editing.rdpGateway;
  ($("advanced") as HTMLDetailsElement).open = editing.skipCertCheck;
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
  // Aufgeklappt, sobald etwas vom Standard abweicht.
  ($("ssh-advanced") as HTMLDetailsElement).open =
    f.fileTransfer.value !== "auto" ||
    !!editing.sshKeepalive ||
    editing.sshCompression ||
    editing.sshLegacy ||
    f.forwards.value !== "" ||
    jumpHost.value !== "" ||
    editing.sshAgent;
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

/** «Desktop über» nur zeigen, wenn VNC und RDP beide an sind. */
function syncDesktopRow() {
  const on = (n: string) => (form.elements.namedItem(n) as HTMLInputElement).checked;
  $("desktop-protocol-row").hidden = !(on("vncOn") && on("rdpOn"));
}
for (const n of ["vncOn", "rdpOn"]) (form.elements.namedItem(n) as HTMLInputElement).addEventListener("change", syncDesktopRow);

let pendingThumb: string | null = null;
let clearThumb = false;
$("thumb-pick").onclick = async () => {
  const f = await pickFolder({ title: "Bild für die Verbindung", filters: [{ name: "Bilder", extensions: ["png", "jpg", "jpeg", "webp"] }] });
  if (typeof f === "string") ((pendingThumb = f), ($("thumb-name").textContent = f.split(/[\\/]/).pop() ?? f));
};
$("thumb-clear").onclick = () => ((pendingThumb = null), (clearThumb = true), ($("thumb-name").textContent = "Letzter Desktop"));

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
  const c: Connection = {
    ...editing,
    name: s("name"),
    group: s("group"),
    address: s("address"),
    vncPort: port("vncOn", "vncPort"),
    sshPort: port("sshOn", "sshPort"),
    rdpPort: port("rdpOn", "rdpPort"),
    rdpDomain: s("rdpDomain"),
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
  const secret = (k: string) => String(fd.get(k) ?? "") || (clearing.has(k) ? "" : undefined);
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
onOpenUrl((urls) => urls.forEach(handleLink)).catch(() => {});
for (const b of document.querySelectorAll<HTMLElement>("[data-help]")) b.onclick = () => openHelp(b.dataset.help!);
listen<string>("help", (e) => openHelp(e.payload));
applyAppearance();
$("open-settings").onclick = () => openSettings();
// Befehle aus dem App-Menü und Tastenkürzel
const quick = initQuick((c) => edit({ ...c, id: crypto.randomUUID(), name: c.address }, true));
$("empty-new").onclick = () => edit(null);
initFieldHelp(document.getElementById("editor-form")!);
initFieldHelp(document.getElementById("quick-form")!);
const command = (cmd: string) => {
  if (cmd === "settings") openSettings();
  else if (cmd === "new") edit(null);
  else if (cmd === "quick") quick.open();
};
addEventListener("sesam:save-quick", (e) => quick.saveAs((e as CustomEvent<Connection>).detail));
listen<string>("cmd", (e) => command(e.payload));
listen<string>("tool", (e) => runTool(e.payload));
addEventListener("keydown", (e) => {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
  if (e.key === ",") (e.preventDefault(), command("settings"));
  else if (e.key.toLowerCase() === "n" && !e.shiftKey && !document.querySelector("dialog[open]")) (e.preventDefault(), command("new"));
});
$("search").addEventListener("input", render);
reload();
