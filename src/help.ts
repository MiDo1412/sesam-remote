import { marked } from "marked";
import { downloadScript } from "./setup";
import vnc from "../docs/hilfe/vnc.md?raw";
import wayvnc from "../docs/hilfe/wayvnc.md?raw";
import macos from "../docs/hilfe/macos.md?raw";
import rdpGnome from "../docs/hilfe/rdp-gnome.md?raw";
import rdpWindows from "../docs/hilfe/rdp-windows.md?raw";
import ssh from "../docs/hilfe/ssh.md?raw";
import ftp from "../docs/hilfe/ftp.md?raw";
import creds from "../docs/hilfe/zugangsdaten.md?raw";

/** Anleitungen aus docs/hilfe; die IDs nutzt auch das Menü «Hilfe» (lib.rs). */
export const topics = [
  { id: "zugangsdaten", title: "Welche Zugangsdaten?", md: creds },
  { id: "rdp-gnome", title: "RDP mit GNOME (Ubuntu)", md: rdpGnome },
  { id: "rdp-windows", title: "RDP mit Windows", md: rdpWindows },
  { id: "vnc", title: "VNC allgemein", md: vnc },
  { id: "wayvnc", title: "wayvnc (wlroots)", md: wayvnc },
  { id: "macos", title: "macOS-Bildschirmfreigabe", md: macos },
  { id: "ssh", title: "SSH und SFTP", md: ssh },
  { id: "ftp", title: "FTP und FTPS", md: ftp },
] as const;

export type TopicId = (typeof topics)[number]["id"];

export function openHelp(id: string) {
  const dlg = document.getElementById("help") as HTMLDialogElement;
  const nav = document.getElementById("help-nav")!;
  const body = document.getElementById("help-body")!;
  const show = (tid: string) => {
    const t = topics.find((x) => x.id === tid) ?? topics[0];
    // Eigene, mitgelieferte Inhalte: als HTML einsetzen ist hier sicher.
    body.innerHTML = marked.parse(t.md, { async: false });
    for (const pre of body.querySelectorAll("pre")) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "copy";
      b.textContent = "Kopieren";
      b.onclick = async () => {
        await navigator.clipboard.writeText(pre.querySelector("code")?.textContent?.trimEnd() ?? "");
        b.textContent = "Kopiert";
        setTimeout(() => (b.textContent = "Kopieren"), 1500);
      };
      pre.append(b);
    }
    for (const a of nav.querySelectorAll<HTMLButtonElement>("button")) a.setAttribute("aria-current", String(a.dataset.id === t.id));
    if (t.id === "rdp-gnome") {
      // Setup-Script direkt aus der Hilfe
      const box = document.createElement("div");
      box.className = "help-actions";
      const dl = Object.assign(document.createElement("button"), { type: "button", className: "primary", textContent: "Setup-Script herunterladen …" });
      dl.onclick = () => downloadScript();
      const note = Object.assign(document.createElement("p"), {
        className: "note",
        textContent: "Oder: Verbindung mit SSH auswählen und in der Detailspalte «RDP auf Zielrechner einrichten …» wählen.",
      });
      box.append(dl, note);
      body.firstElementChild?.after(box);
    }
    body.scrollTop = 0;
  };
  nav.replaceChildren(
    ...topics.map((t) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "nav-item";
      b.dataset.id = t.id;
      b.textContent = t.title;
      b.onclick = () => show(t.id);
      return b;
    }),
  );
  show(id);
  if (!dlg.open) dlg.showModal();
}
