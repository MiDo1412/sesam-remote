import { api, desktopProtocol, type Connection } from "./api";
import { shellTab } from "./shell";
import { desktopTab } from "./desktop";
import { filesTab } from "./files";
import { initTabs, openTab } from "./tabs";
import { ask, tell } from "./ui";
import { openHelp } from "./help";
import { listen } from "@tauri-apps/api/event";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { enhanceSelect } from "./menu";

// Native Auswahllisten durch eigene Menüs ersetzen (einmal beim Start).
const formEls = (document.getElementById("editor-form") as HTMLFormElement).elements;
const keyModeMenu = enhanceSelect(formEls.namedItem("keyMode") as HTMLSelectElement);
const fileTransferMenu = enhanceSelect(formEls.namedItem("fileTransfer") as HTMLSelectElement);

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

async function reload() {
  try {
    items = await api.list();
  } catch (e) {
    tell("Bibliothek nicht lesbar", String(e));
  }
  render();
}

function render() {
  const groups = [...new Set(items.map((c) => c.group).filter(Boolean))].sort((a, b) => a.localeCompare(b, "de-CH"));
  const navItem = (label: string, count: number, on: boolean, click: () => void) => {
    const a = el("button", { type: "button", class: "nav-item", "aria-current": on ? "page" : "false" }, el("span", {}, label), el("span", { class: "count" }, String(count)));
    a.onclick = click;
    return a;
  };
  $("nav").replaceChildren(
    navItem("Alle", items.length, filter.kind === "all", () => ((filter = { kind: "all" }), render())),
    ...(groups.length ? [el("div", { class: "nav-header" }, "Gruppen")] : []),
    ...groups.map((g) =>
      navItem(g, items.filter((c) => c.group === g).length, filter.kind === "group" && filter.name === g, () => ((filter = { kind: "group", name: g }), render())),
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
        el("div", { class: "thumb" }, el("span", { class: "badge" }, kind)),
        el("div", { class: "card-name" }, c.name),
        el("div", { class: "card-sub" }, c.address),
      );
      card.onclick = () => ((selected = c.id), render());
      card.ondblclick = () => open(c, c.defaultView);
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
  );
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
  sharedFolder: "",
  skipCertCheck: false,
  fitToWindow: true,
  hasPassword: false,
  hasVncPassword: false,
  hasRdpPassword: false,
  hasKey: false,
  fileTransfer: "auto",
  lastUsed: null,
});

function edit(c: Connection | null) {
  const isNew = !c;
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
  showPwHint("vnc-pw", editing.hasVncPassword, "Leer = Passwort oben.");
  showPwHint("rdp-pw", editing.hasRdpPassword, "Leer = Passwort oben.");
  ($("extra-creds") as HTMLDetailsElement).open = editing.hasVncPassword || editing.hasRdpPassword;
  syncDesktopRow();
  showPwHint("pw", editing.hasPassword, "");
  f.sharedClipboard.checked = editing.sharedClipboard;
  f.playAudio.checked = editing.playAudio;
  f.sharedFolder.value = editing.sharedFolder;
  f.skipCertCheck.checked = editing.skipCertCheck;
  f.fileTransfer.value = editing.fileTransfer ?? "auto";
  fileTransferMenu.sync();
  ($("advanced") as HTMLDetailsElement).open = editing.skipCertCheck || f.fileTransfer.value !== "auto";
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

function showPwHint(prefix: "pw" | "vnc-pw" | "rdp-pw", stored: boolean, emptyText: string) {
  const field = ({ pw: "password", "vnc-pw": "vncPassword", "rdp-pw": "rdpPassword" } as const)[prefix];
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
    fileTransfer: (s("fileTransfer") || "auto") as Connection["fileTransfer"],
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
    const saved = await api.save(c, { password: secret("password"), vncPassword: secret("vncPassword"), rdpPassword: secret("rdpPassword"), sshKey });
    selected = saved.id;
    dlg.close();
    reload();
  } catch (e) {
    $("editor-error").textContent = String(e);
  }
};

$("new").onclick = () => edit(null);
for (const b of document.querySelectorAll<HTMLElement>("[data-help]")) b.onclick = () => openHelp(b.dataset.help!);
listen<string>("help", (e) => openHelp(e.payload));
$("search").addEventListener("input", render);
reload();
