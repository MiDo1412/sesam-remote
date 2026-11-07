/** Eigene Titelleiste: unter Windows ohne Fensterrahmen (Logo-Menü links, Fenstersteuerung rechts),
 *  unter macOS bleibt die Menüleiste nativ und die Titelleiste ist transparent (Ampel links). */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openMenu, type MenuItem } from "./menu";
import { topics } from "./help";

export const isMac = navigator.userAgent.includes("Mac");

export function initTitlebar(actions: { command(c: string): void; tool(t: string): void; help(id: string): void }) {
  const bars = document.querySelectorAll<HTMLElement>("header.toolbar, header.session-bar");
  for (const h of bars) h.setAttribute("data-tauri-drag-region", "");
  if (isMac) return void document.body.classList.add("mac");
  document.body.classList.add("custom-titlebar");
  const win = getCurrentWindow();

  const items: MenuItem[] = [
    { value: "cmd:new", label: "Neue Verbindung …", shortcut: "Strg+N", section: "Ablage" },
    { value: "cmd:quick", label: "Schnelle Verbindung …", shortcut: "Strg+K" },
    { value: "cmd:settings", label: "Einstellungen …", shortcut: "Strg+," },
    { value: "tool:Bibliothek", label: "Bibliothek anzeigen", section: "Sitzung", separator: true },
    { value: "tool:Abkoppeln", label: "In eigenem Fenster öffnen" },
    { value: "tool:Andocken", label: "Ins Hauptfenster zurück" },
    { value: "tool:Minimieren", label: "Minimieren" },
    { value: "tool:Trennen", label: "Trennen" },
    ...topics.map((t, i) => ({ value: `help:${t.id}`, label: t.title, ...(i === 0 ? { section: "Hilfe", separator: true } : {}) })),
  ];
  const menu = async (anchor: HTMLElement) => {
    const v = await openMenu(anchor, items);
    if (!v) return;
    const [kind, arg] = [v.slice(0, v.indexOf(":")), v.slice(v.indexOf(":") + 1)];
    if (kind === "cmd") actions.command(arg);
    else if (kind === "tool") actions.tool(arg);
    else actions.help(arg);
  };

  for (const h of bars) {
    // Links: Sesam-Logo als Menüknopf (in der Bibliothek ersetzt es das Logo).
    const brand = h.querySelector<HTMLElement>(".brand");
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "app-menu";
    btn.title = "Menü";
    btn.setAttribute("aria-label", "Menü");
    btn.innerHTML = `<img src="/sesam-icon.svg" alt="" width="20" height="20" />${brand ? "<span>Sesam</span>" : ""}`;
    btn.onclick = () => menu(btn);
    if (brand) brand.replaceWith(btn);
    else h.prepend(btn);

    // Rechts: Minimieren, Maximieren/Wiederherstellen, Schliessen (Glyphen wie Windows 11).
    const ctl = document.createElement("div");
    ctl.className = "win-controls";
    const mk = (glyph: string, label: string, onClick: () => void, cls = "") => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = `win-btn ${cls}`;
      b.textContent = glyph;
      b.title = label;
      b.setAttribute("aria-label", label);
      b.onclick = onClick;
      return b;
    };
    const max = mk("", "Maximieren", () => win.toggleMaximize());
    ctl.append(mk("", "Minimieren", () => win.minimize()), max, mk("", "Schliessen", () => win.close(), "close"));
    h.append(ctl);
    const sync = async () => {
      const on = await win.isMaximized();
      max.textContent = on ? "" : "";
      max.title = on ? "Wiederherstellen" : "Maximieren";
      max.setAttribute("aria-label", max.title);
    };
    void sync();
    void win.onResized(sync);
  }
}
