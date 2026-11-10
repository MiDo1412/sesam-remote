/** Erste Hilfe: eine Verbindung Schritt für Schritt prüfen (src-tauri/src/firstaid.rs, conn_diagnose) und je nach
 *  Befund einen Knopf anbieten: Aufwecken, Lokales Netzwerk freigeben, über SSH-Tunnel, Zugangsdaten, Verbinden. */
import { Channel, invoke } from "@tauri-apps/api/core";
import { api, desktopProtocol, filesProtocol, type Connection } from "./api";
import { openPermissions } from "./permissions";

interface Step {
  key: string;
  title: string;
  ok: boolean | null;
  detail: string;
  hint: "" | "wake" | "localnet" | "tunnel" | "creds" | "trust";
}

export type Service = "ssh" | "rdp" | "vnc" | "ftp";
const LABEL: Record<Service, string> = { ssh: "SSH (Shell)", rdp: "RDP (Desktop)", vnc: "VNC (Desktop)", ftp: "FTP (Dateien)" };

/** Dienst passend zur Ansicht (Desktop, Shell, Dateien). */
export function serviceOf(c: Connection, view: "desktop" | "shell" | "files"): Service | null {
  if (view === "desktop") return desktopProtocol(c);
  if (view === "files") return filesProtocol(c) === "ftp" ? "ftp" : c.sshPort ? "ssh" : null;
  return c.sshPort ? "ssh" : null;
}

export interface FirstAidActions {
  edit(c: Connection): void;
  wake(c: Connection): void;
  connect(c: Connection, service: Service): void;
}

export function openFirstAid(c: Connection, service: Service | null, act: FirstAidActions) {
  const offered = (["rdp", "vnc", "ssh", "ftp"] as Service[]).filter((s) => ({ ssh: c.sshPort, rdp: c.rdpPort, vnc: c.vncPort, ftp: c.ftpPort })[s]);
  let svc: Service = service && offered.includes(service) ? service : offered[0];
  const dlg = document.createElement("dialog");
  dlg.className = "first-aid";
  dlg.setAttribute("aria-labelledby", "first-aid-title");
  dlg.innerHTML = `<form class="sheet" method="dialog">
      <h2 id="first-aid-title"></h2>
      <p class="note">Sesam prüft der Reihe nach, wo die Verbindung hängt. Es ändert dabei nichts und speichert nichts.</p>
      <div class="segmented first-aid-svc" role="radiogroup" aria-label="Dienst"></div>
      <ol class="first-aid-steps" aria-live="polite"></ol>
      <div class="first-aid-fix"></div>
      <div class="row actions"><span></span><button type="button" class="again">Erneut prüfen</button><button type="submit" class="primary">Fertig</button></div>
    </form>`;
  dlg.querySelector("h2")!.textContent = `Erste Hilfe · ${c.name}`;
  const list = dlg.querySelector<HTMLOListElement>(".first-aid-steps")!;
  const fix = dlg.querySelector<HTMLElement>(".first-aid-fix")!;
  const seg = dlg.querySelector<HTMLElement>(".first-aid-svc")!;
  seg.hidden = offered.length < 2;
  for (const s of offered) {
    const l = document.createElement("label");
    const r = Object.assign(document.createElement("input"), { type: "radio", name: "svc", value: s, checked: s === svc });
    r.onchange = () => ((svc = s), void run());
    l.append(r, LABEL[s]);
    seg.append(l);
  }
  const button = (text: string, onclick: () => void, primary = false) => {
    const b = Object.assign(document.createElement("button"), { type: "button", textContent: text, className: primary ? "primary" : "" });
    b.onclick = onclick;
    return b;
  };
  let run_ = 0;
  async function run() {
    const me = ++run_;
    list.replaceChildren();
    fix.replaceChildren();
    const busy = Object.assign(document.createElement("li"), { className: "busy", textContent: "Prüfe …" });
    list.append(busy);
    const steps: Step[] = [];
    const ch = new Channel<Step>();
    ch.onmessage = (s) => {
      if (me !== run_) return;
      steps.push(s);
      const li = document.createElement("li");
      li.className = s.ok === true ? "ok" : s.ok === false ? "fail" : "info";
      const mark = Object.assign(document.createElement("span"), { className: "mark", textContent: s.ok === true ? "✓" : s.ok === false ? "✕" : "–" });
      mark.setAttribute("aria-label", s.ok === true ? "bestanden" : s.ok === false ? "Fehler" : "Hinweis");
      const text = document.createElement("div");
      text.append(Object.assign(document.createElement("b"), { textContent: s.title }), Object.assign(document.createElement("small"), { textContent: s.detail }));
      li.append(mark, text);
      list.insertBefore(li, busy);
    };
    try {
      await invoke("conn_diagnose", { id: c.id, service: svc, onStep: ch });
    } catch (e) {
      ch.onmessage({ key: "error", title: "Prüfung nicht möglich", ok: false, detail: String(e), hint: "" });
    }
    if (me !== run_) return;
    busy.remove();
    const failed = steps.some((s) => s.ok === false);
    const hints = new Set(steps.map((s) => s.hint).filter(Boolean));
    if (hints.has("wake") && c.mac) fix.append(button("Aufwecken", () => (dlg.close(), act.wake(c)), true));
    if (hints.has("localnet")) fix.append(button("Berechtigungen …", () => void openPermissions(), true));
    // Tunnel nur, wenn SSH auf dem Gerät antwortet.
    const sshOpen = hints.has("tunnel") && svc !== "ssh" && !!c.sshPort && !c.sshTunnel && (await invoke<number[]>("probe_services", { address: c.address, ports: [c.sshPort] }).catch((): number[] => [])).includes(c.sshPort!);
    if (me !== run_) return;
    if (sshOpen)
      fix.append(
        button("Über SSH-Tunnel verbinden", async () => {
          await api.save({ ...c, sshTunnel: true }, {});
          c.sshTunnel = true;
          void run();
        }),
      );
    if (hints.has("creds")) fix.append(button("Zugangsdaten …", () => (dlg.close(), act.edit(c)), true));
    if (hints.has("trust") || !failed) fix.append(button("Verbinden", () => (dlg.close(), act.connect(c, svc)), !failed));
    if (!failed && !hints.size) list.append(Object.assign(document.createElement("li"), { className: "info done", textContent: "Alles in Ordnung." }));
  }
  dlg.querySelector<HTMLButtonElement>(".again")!.onclick = () => void run();
  document.body.append(dlg);
  dlg.onclose = () => (run_++, dlg.remove());
  dlg.showModal();
  if (svc) void run();
  else list.append(Object.assign(document.createElement("li"), { className: "info", textContent: "Diese Verbindung hat keinen Dienst eingetragen." }));
}
