/** Schnelle Verbindung: nichts speichern, Passwort nur im Arbeitsspeicher, zuletzt genutzte Ziele ohne Zugangsdaten. */
import { api, type Connection } from "./api";
import { ICON } from "./icons";
import { openTab, type Kind } from "./tabs";

type QuickKind = "rdp" | "vnc" | "shell" | "files";
interface Recent { kind: QuickKind; address: string; port: number; user: string }

const PORT: Record<QuickKind, number> = { rdp: 3389, vnc: 5900, shell: 22, files: 22 };
const LABEL: Record<QuickKind, string> = { rdp: "RDP", vnc: "VNC", shell: "SSH", files: "SFTP" };
const KEY = "sesam.quick";
const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** «ssh mike@host:22», «rdp://host:3389», «mike@host», «host:port» → Art, Benutzer, Adresse, Port. */
export function parseTarget(text: string): Partial<Recent> {
  let t = text.trim();
  const out: Partial<Recent> = {};
  const scheme = /^(ssh|sftp|rdp|vnc)(:\/\/|\s+)/i.exec(t);
  if (scheme) {
    const s = scheme[1].toLowerCase();
    out.kind = s === "ssh" ? "shell" : s === "sftp" ? "files" : (s as QuickKind);
    t = t.slice(scheme[0].length);
  }
  const at = t.lastIndexOf("@");
  if (at > 0) {
    out.user = t.slice(0, at);
    t = t.slice(at + 1);
  }
  const v6 = /^\[([^\]]+)\](?::(\d+))?$/.exec(t);
  const hp = v6 ? [v6[1], v6[2]] : t.split(":").length === 2 ? t.split(":") : [t, undefined];
  out.address = (hp[0] ?? "").replace(/\/+$/, "");
  if (hp[1] && /^\d+$/.test(hp[1])) out.port = Number(hp[1]);
  return out;
}

// Selbstprüfung im Entwicklungsbuild
if (import.meta.env.DEV) {
  const a = parseTarget("ssh mike@192.168.1.4:2222");
  console.assert(a.kind === "shell" && a.user === "mike" && a.address === "192.168.1.4" && a.port === 2222, "parseTarget ssh");
  const b = parseTarget("rdp://host.local:3390");
  console.assert(b.kind === "rdp" && b.address === "host.local" && b.port === 3390, "parseTarget rdp");
}

const recents = (): Recent[] => {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "[]");
  } catch {
    return [];
  }
};
const saveRecents = (r: Recent[]) => localStorage.setItem(KEY, JSON.stringify(r.slice(0, 8)));

export function initQuick(onSaveAsConnection: (c: Connection) => void) {
  $("quick").querySelector(".quick-icon")!.innerHTML = ICON.quick;
  const dlg = $<HTMLDialogElement>("quick-dlg");
  const form = $<HTMLFormElement>("quick-form");
  const f = form.elements as unknown as Record<string, HTMLInputElement>;
  const kind = () => (form.querySelector<HTMLInputElement>("[name=kind]:checked")?.value ?? "rdp") as QuickKind;
  let portTouched = false;
  const setKind = (k: QuickKind) => {
    form.querySelector<HTMLInputElement>(`[name=kind][value=${k}]`)!.checked = true;
    if (!portTouched) f.port.value = String(PORT[k]);
  };
  for (const r of form.querySelectorAll<HTMLInputElement>("[name=kind]")) r.onchange = () => setKind(kind());
  f.port.oninput = () => (portTouched = true);
  f.address.oninput = () => {
    const p = parseTarget(f.address.value);
    if (p.kind) setKind(p.kind);
    if (p.port) ((f.port.value = String(p.port)), (portTouched = true));
    if (p.user && !f.user.value) f.user.value = p.user;
  };

  const renderRecent = () => {
    const box = $("quick-recent");
    const list = recents();
    box.hidden = !list.length;
    box.replaceChildren(
      Object.assign(document.createElement("div"), { className: "recent-title", textContent: "Zuletzt" }),
      ...list.map((r, i) => {
        const chip = document.createElement("span");
        chip.className = "chip";
        const go = Object.assign(document.createElement("button"), { type: "button", className: "chip-main", textContent: `${LABEL[r.kind]} · ${r.user ? r.user + "@" : ""}${r.address}:${r.port}` });
        go.onclick = () => {
          setKind(r.kind);
          f.address.value = r.address;
          f.port.value = String(r.port);
          portTouched = true;
          f.user.value = r.user;
          f.password.focus();
        };
        const del = Object.assign(document.createElement("button"), { type: "button", className: "chip-x", title: "Aus der Liste entfernen" });
        del.innerHTML = ICON.close;
        del.setAttribute("aria-label", "Aus der Liste entfernen");
        del.onclick = () => (saveRecents(recents().filter((_, j) => j !== i)), renderRecent());
        chip.append(go, del);
        return chip;
      }),
    );
  };

  const open = () => {
    form.reset();
    portTouched = false;
    setKind("rdp");
    renderRecent();
    dlg.showModal();
    f.address.focus();
  };
  $("quick").onclick = open;
  $("empty-quick").onclick = open;
  $("quick-cancel").onclick = () => dlg.close();

  form.onsubmit = async (ev) => {
    ev.preventDefault();
    const p = parseTarget(f.address.value);
    const k = kind();
    const port = Number(f.port.value) || PORT[k];
    const user = f.user.value.trim() || p.user || "";
    const address = p.address ?? "";
    const c: Connection = {
      id: "quick", name: `${address} (schnell)`, group: "", address,
      vncPort: k === "vnc" ? port : null, sshPort: k === "shell" || k === "files" ? port : null, rdpPort: k === "rdp" ? port : null,
      rdpDomain: "", desktopProtocol: k === "vnc" ? "vnc" : "rdp", user,
      defaultView: k === "shell" ? "shell" : k === "files" ? "files" : "desktop", orientation: "landscape",
      sharedClipboard: true, fitToWindow: true, playAudio: true, sharedFolder: "", skipCertCheck: false, os: "", customThumb: false, credentialId: null, rdpNoNtlm: false, rdpKdc: "", rdpGateway: "", rdpGatewayUser: "", hasGatewayPassword: false,
      hasPassword: false, hasVncPassword: false, hasRdpPassword: false, hasKey: false, lastUsed: null,
    };
    try {
      c.id = await api.quickRegister(c, f.password.value || undefined);
      saveRecents([{ kind: k, address, port, user }, ...recents().filter((r) => !(r.kind === k && r.address === address && r.port === port))]);
      dlg.close();
      openTab(c, c.defaultView as Kind);
    } catch (e) {
      f.address.setCustomValidity(String(e));
      form.reportValidity();
      f.address.setCustomValidity("");
    }
  };

  addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "k" && !document.querySelector("dialog[open]")) {
      e.preventDefault();
      open();
    }
  });
  return { open, saveAs: onSaveAsConnection };
}
