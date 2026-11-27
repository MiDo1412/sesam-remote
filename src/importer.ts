// Verbindungen übernehmen: eigene SSH-Konfiguration, eine Datei (.rdp oder ssh config) oder Geräte im lokalen Netz.
// Die Vorschläge kommen aus src-tauri/src/import.rs; hier wählt man aus, gespeichert wird wie im Formular.
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, type Connection } from "./api";
import { openMenu } from "./menu";
import { tell } from "./ui";
import { isMobile } from "./titlebar";

interface Found { connection: Connection; jump: string; keyFile: string }
interface Seen { name: string; host: string; address: string; port: number; service: "ssh" | "vnc" | "rdp" }

const label = (f: Found) => {
  const c = f.connection;
  const port = c.sshPort ? `SSH ${c.sshPort}` : c.rdpPort ? `RDP ${c.rdpPort}` : c.vncPort ? `VNC ${c.vncPort}` : "";
  return `${c.user || c.rdpUser ? `${c.user || c.rdpUser}@` : ""}${c.address} · ${port}`;
};
const same = (a: Connection, b: Connection) =>
  a.address.toLowerCase() === b.address.toLowerCase() && ((!!b.sshPort && a.sshPort === b.sshPort) || (!!b.rdpPort && a.rdpPort === b.rdpPort) || (!!b.vncPort && a.vncPort === b.vncPort));

/** Auswahlliste mit Häkchen; was schon in der Bibliothek steht, ist abgewählt. */
function pick(title: string, found: Found[], existing: Connection[]): Promise<Found[] | null> {
  const dlg = document.createElement("dialog");
  dlg.className = "import-dlg";
  const form = Object.assign(document.createElement("form"), { className: "sheet", method: "dialog" });
  const h = Object.assign(document.createElement("h2"), { textContent: title });
  const list = Object.assign(document.createElement("div"), { className: "import-list" });
  const boxes = found.map((f) => {
    const known = existing.some((e) => same(e, f.connection));
    const row = Object.assign(document.createElement("label"), { className: "row check" });
    const box = Object.assign(document.createElement("input"), { type: "checkbox", checked: !known });
    const text = Object.assign(document.createElement("span"), { textContent: f.connection.name });
    const sub = Object.assign(document.createElement("span"), { className: "note", textContent: known ? `${label(f)} · schon in der Bibliothek` : label(f) });
    row.append(box, text, sub);
    list.append(row);
    return box;
  });
  const actions = Object.assign(document.createElement("div"), { className: "row actions" });
  const cancel = Object.assign(document.createElement("button"), { type: "button", textContent: "Abbrechen" });
  const ok = Object.assign(document.createElement("button"), { type: "submit", className: "primary", textContent: "Übernehmen" });
  actions.append(Object.assign(document.createElement("span")), cancel, ok);
  form.append(h, list, actions);
  dlg.append(form);
  document.body.append(dlg);
  return new Promise((resolve) => {
    cancel.onclick = () => dlg.close();
    form.onsubmit = (e) => {
      e.preventDefault();
      resolve(found.filter((_, i) => boxes[i].checked));
      dlg.close();
    };
    dlg.onclose = () => (resolve(null), dlg.remove());
    dlg.showModal();
  });
}

/** Ausgewählte speichern: neue IDs, Sprungserver auf die neue ID, Schlüsseldatei verschlüsselt wie eingefügt. */
async function save(chosen: Found[]) {
  const ids = new Map(chosen.map((f) => [f.connection.name, crypto.randomUUID()]));
  let noKey = 0;
  for (const f of chosen) {
    const c: Connection = { ...f.connection, id: ids.get(f.connection.name)!, jumpHost: ids.get(f.jump) ?? "" };
    let sshKey: string | undefined;
    if (f.keyFile) sshKey = await api.readKeyFile(f.keyFile).catch(() => (noKey++, undefined));
    await api.save(c, { sshKey });
  }
  dispatchEvent(new CustomEvent("sesam:library-changed"));
  tell(
    "Übernommen",
    `${chosen.length} Verbindung(en) in der Bibliothek.${noKey ? ` ${noKey} Schlüsseldatei(en) liessen sich nicht lesen (fehlt oder mit Passphrase); bitte im Formular wählen.` : ""}`,
  );
}

async function fromFile(file: string) {
  const found = await invoke<Found[]>("import_file", { file });
  const chosen = await pick("Verbindungen übernehmen", found, await api.list());
  if (chosen?.length) await save(chosen);
}

async function fromNetwork() {
  const seen = await invoke<Seen[]>("discover_network");
  if (!seen.length) return tell("Nichts gefunden", "Im lokalen Netz hat sich kein Gerät mit SSH, VNC oder RDP gemeldet (Bonjour/mDNS). Manche Geräte melden sich nicht von selbst; dann die Adresse von Hand eintragen.");
  const found: Found[] = seen.map((s) => ({
    connection: {
      ...({} as Connection),
      name: s.name,
      address: s.address,
      sshPort: s.service === "ssh" ? s.port : null,
      vncPort: s.service === "vnc" ? s.port : null,
      rdpPort: s.service === "rdp" ? s.port : null,
      ftpPort: null,
      desktopProtocol: s.service === "rdp" ? "rdp" : "vnc",
      defaultView: s.service === "ssh" ? "shell" : "desktop",
    },
    jump: "",
    keyFile: "",
  }));
  const chosen = await pick("Im Netzwerk gefunden", found, await api.list());
  if (chosen?.length) await save(chosen);
}

/** Menü «Importieren …» an `anchor` (ohne Anker: Mitte des Fensters). */
export async function openImport(anchor?: HTMLElement) {
  const config = await invoke<string | null>("ssh_config_path").catch(() => null);
  const at = anchor ?? Object.assign(document.createElement("span"), { style: `position:fixed;left:50%;top:30%;width:0;height:0` });
  if (!anchor) document.body.append(at);
  const v = await openMenu(at, [
    ...(config ? [{ value: "config", label: "Aus meiner SSH-Konfiguration" }] : []),
    { value: "file", label: "Datei wählen (.rdp oder SSH-Konfiguration) …" },
    ...(isMobile ? [] : [{ value: "net", label: "Im Netzwerk suchen" }]),
  ]).finally(() => anchor || at.remove());
  try {
    if (v === "config" && config) await fromFile(config);
    else if (v === "net") await fromNetwork();
    else if (v === "file") {
      const file = await openDialog({ multiple: false, directory: false });
      if (typeof file === "string") await fromFile(file);
    }
  } catch (e) {
    tell("Nicht übernommen", String(e));
  }
}
