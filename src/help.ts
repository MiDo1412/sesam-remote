import { api } from "./api";
import { marked } from "marked";
import { downloadScript } from "./setup";
import { isMobile } from "./titlebar";
import { LINKS, openLink } from "./settings";
import { visibleSession } from "./tabs";
import { desktopProtocol, filesProtocol, type Connection } from "./api";
import vnc from "../docs/hilfe/vnc.md?raw";
import wayvnc from "../docs/hilfe/wayvnc.md?raw";
import macos from "../docs/hilfe/macos.md?raw";
import rdpGnome from "../docs/hilfe/rdp-gnome.md?raw";
import rdpWindows from "../docs/hilfe/rdp-windows.md?raw";
import rdpXrdp from "../docs/hilfe/rdp-xrdp.md?raw";
import ssh from "../docs/hilfe/ssh.md?raw";
import ftp from "../docs/hilfe/ftp.md?raw";
import creds from "../docs/hilfe/zugangsdaten.md?raw";
import komfort from "../docs/hilfe/komfort.md?raw";

/** Anleitungen aus docs/hilfe; die IDs nutzt auch das Menü «Hilfe» (lib.rs). */
export const topics = [
  { id: "zugangsdaten", title: "Welche Zugangsdaten?", md: creds },
  { id: "rdp-gnome", title: "RDP mit GNOME (Ubuntu)", md: rdpGnome },
  { id: "rdp-windows", title: "RDP mit Windows", md: rdpWindows },
  { id: "rdp-xrdp", title: "RDP mit xrdp (Linux)", md: rdpXrdp },
  { id: "vnc", title: "VNC allgemein", md: vnc },
  { id: "wayvnc", title: "wayvnc (wlroots)", md: wayvnc },
  { id: "macos", title: "macOS-Bildschirmfreigabe", md: macos },
  { id: "ssh", title: "SSH und SFTP", md: ssh },
  { id: "ftp", title: "FTP und FTPS", md: ftp },
  { id: "komfort", title: "Komfortfunktionen", md: komfort },
] as const;

export type TopicId = (typeof topics)[number]["id"];

/** Gruppen im Hilfe-Fenster und im Menü «Hilfe» (lib.rs). */
const GROUPS: { title: string; ids: TopicId[] }[] = [
  { title: "Zielrechner einrichten", ids: ["rdp-windows", "rdp-gnome", "rdp-xrdp", "macos", "wayvnc"] },
  { title: "Verbinden", ids: ["zugangsdaten", "vnc", "ssh", "ftp"] },
  { title: "Sesam kennenlernen", ids: ["komfort"] },
];

/** Anleitung, die zur offenen Sitzung passt (Protokoll und System), sonst zum Einstieg. */
export function topicFor(c: Connection | null, kind?: string): TopicId {
  if (!c) return "zugangsdaten";
  if (kind === "shell") return "ssh";
  if (kind === "files") return filesProtocol(c) === "ftp" ? "ftp" : "ssh";
  if (desktopProtocol(c) === "rdp") return c.rdpTarget || c.os === "ubuntu" ? "rdp-gnome" : c.os === "linux" || c.os === "raspberry" ? "rdp-xrdp" : "rdp-windows";
  return c.os === "macos" ? "macos" : "vnc";
}

/** `id` leer: die Anleitung zur Lage (Menü «Hilfe zu dieser Ansicht», ☰ › Hilfe). */
export function openHelp(id: string) {
  const here = visibleSession();
  if (!id) id = topicFor(here?.conn ?? null, here?.kind);
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
        await api.clipWrite(pre.querySelector("code")?.textContent?.trimEnd() ?? "");
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
  const hereId = here ? topicFor(here.conn, here.kind) : null;
  nav.replaceChildren(
    ...GROUPS.flatMap((g) => [
      Object.assign(document.createElement("h3"), { className: "nav-header nav-head", textContent: g.title }),
      ...g.ids.map((tid) => {
        const t = topics.find((x) => x.id === tid)!;
        const b = document.createElement("button");
        b.type = "button";
        b.className = "nav-item";
        b.dataset.id = t.id;
        b.textContent = t.title;
        // Kleiner Hinweis, welche Anleitung zur offenen Sitzung gehört.
        if (t.id === hereId) b.append(Object.assign(document.createElement("small"), { className: "nav-here", textContent: here!.conn.name }));
        b.onclick = () => (show(t.id), b.focus({ preventScroll: true }));
        return b;
      }),
    ]),
  );
  // Suche: Themen ausblenden, deren Titel und Text das Gesuchte nicht enthalten.
  const search = Object.assign(document.createElement("input"), { type: "search", placeholder: "Hilfe durchsuchen", className: "help-search" });
  search.setAttribute("aria-label", "Hilfe durchsuchen");
  search.oninput = () => {
    const q = search.value.trim().toLowerCase();
    let first: string | null = null;
    for (const b of nav.querySelectorAll<HTMLButtonElement>("button.nav-item")) {
      const t = topics.find((x) => x.id === b.dataset.id)!;
      b.hidden = !!q && !`${t.title}\n${t.md}`.toLowerCase().includes(q);
      if (!b.hidden) first ??= t.id;
    }
    // Gruppentitel ohne sichtbare Anleitung ausblenden.
    for (const h of nav.querySelectorAll<HTMLElement>(".nav-head")) {
      let n = h.nextElementSibling;
      let any = false;
      while (n && !n.classList.contains("nav-head")) (any ||= !(n as HTMLElement).hidden), (n = n.nextElementSibling);
      h.hidden = !any;
    }
    if (q && first) show(first);
  };
  nav.prepend(search);
  document.getElementById("help-faq")!.onclick = () => openLink(LINKS.faq);
  document.getElementById("help-issue")!.onclick = () => openLink(LINKS.issue(`Hilfe: ${body.querySelector("h1")?.textContent ?? ""}`));
  show(id);
  if (!dlg.open) {
    dlg.showModal();
    // Touch: nicht gleich die Bildschirmtastatur für das Suchfeld zeigen, Fokus auf die gewählte Anleitung.
    if (isMobile) nav.querySelector<HTMLElement>("[aria-current=true]")?.focus({ preventScroll: true });
  }
}
