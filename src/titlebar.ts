import { api } from "./api";
/** Eigene Titelleiste: unter Windows ohne Fensterrahmen (Logo-Menü links, Fenstersteuerung rechts),
 *  unter macOS bleibt die Menüleiste nativ und die Titelleiste ist transparent (Ampel links). */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openMenu, type MenuItem } from "./menu";
import { topics } from "./help";
import minusSvg from "@phosphor-icons/core/assets/regular/minus.svg?raw";
import squareSvg from "@phosphor-icons/core/assets/regular/square.svg?raw";
import restoreSvg from "@phosphor-icons/core/assets/regular/copy.svg?raw";
import closeSvg from "@phosphor-icons/core/assets/regular/x.svg?raw";

// Eigene Symbole statt der Windows-Symbolschrift (die nicht überall verfügbar ist und dann bunt ersetzt wird).
const glyph = (svg: string) => svg.replace("<svg ", '<svg width="12" height="12" aria-hidden="true" ');
import { invoke } from "@tauri-apps/api/core";

export const isMac = navigator.userAgent.includes("Mac");

export function initTitlebar(actions: { command(c: string): void; tool(t: string): void; help(id: string): void }) {
  const bars = document.querySelectorAll<HTMLElement>("header.toolbar, header.session-bar");
  for (const h of bars) h.setAttribute("data-tauri-drag-region", "");
  if (isMac) return void document.body.classList.add("mac");
  document.body.classList.add("custom-titlebar");
  const win = getCurrentWindow();

  // Windows-Begriffe: Datei / Bearbeiten / Ansicht / Sitzung / Hilfe (docs/beschriftungen.md).
  const items: MenuItem[] = [
    { value: "cmd:new", label: "Neue Verbindung …", shortcut: "Strg+N", section: "Datei" },
    { value: "cmd:quick", label: "Schnelle Verbindung …", shortcut: "Strg+K" },
    { value: "cmd:terminal", label: "Neues Terminal", shortcut: "Strg+T" },
    { value: "cmd:addubuntu", label: "Ubuntu-PC hinzufügen …" },
    { value: "cmd:settings", label: "Einstellungen", shortcut: "Strg+,", separator: true },
    { value: "app:quit", label: "Beenden", shortcut: "Alt+F4", separator: true },
    { value: "edit:cut", label: "Ausschneiden", shortcut: "Strg+X", section: "Bearbeiten", separator: true },
    { value: "edit:copy", label: "Kopieren", shortcut: "Strg+C" },
    { value: "edit:paste", label: "Einfügen", shortcut: "Strg+V" },
    { value: "edit:selectAll", label: "Alles auswählen", shortcut: "Strg+A" },
    { value: "tool:Bibliothek", label: "Bibliothek anzeigen", section: "Ansicht", separator: true },
    { value: "tool:Vorheriger Tab", label: "Vorheriger Tab" },
    { value: "tool:Nächster Tab", label: "Nächster Tab" },
    { value: "tool:Vollbild", label: "Vollbild", shortcut: "F11" },
    { value: "tool:Abkoppeln", label: "In eigenem Fenster öffnen", section: "Sitzung", separator: true },
    { value: "tool:Andocken", label: "Ins Hauptfenster zurück" },
    { value: "tool:Minimieren", label: "Minimieren" },
    { value: "tool:Trennen", label: "Trennen" },
    ...topics.map((t, i) => ({ value: `help:${t.id}`, label: t.title, ...(i === 0 ? { section: "Hilfe", separator: true } : {}) })),
    { value: "cmd:about", label: "Über Sesam Remote", separator: true },
  ];
  const menu = async (anchor: HTMLElement) => {
    const focused = document.activeElement as HTMLElement | null; // Bearbeiten wirkt auf das Feld davor
    const v = await openMenu(anchor, items);
    if (!v) return;
    const [kind, arg] = [v.slice(0, v.indexOf(":")), v.slice(v.indexOf(":") + 1)];
    if (kind === "cmd") actions.command(arg);
    else if (kind === "tool") actions.tool(arg);
    else if (kind === "app") void invoke("app_exit");
    else if (kind === "edit") void editCommand(arg, focused);
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
      b.innerHTML = glyph;
      b.title = label;
      b.setAttribute("aria-label", label);
      b.onclick = onClick;
      return b;
    };
    const max = mk(glyph(squareSvg), "Maximieren", () => win.toggleMaximize());
    ctl.append(mk(glyph(minusSvg), "Minimieren", () => win.minimize()), max, mk(glyph(closeSvg), "Schliessen", () => win.close(), "close"));
    h.append(ctl);
    const sync = async () => {
      const on = await win.isMaximized();
      max.innerHTML = glyph(on ? restoreSvg : squareSvg);
      max.title = on ? "Wiederherstellen" : "Maximieren (Andocklayouts: Win+Z)";
      max.setAttribute("aria-label", max.title);
    };
    void sync();
    void win.onResized(sync);
  }
}

/** Bearbeiten-Befehle für das zuletzt fokussierte Eingabefeld (Einfügen über die Zwischenablage-API). */
async function editCommand(cmd: string, el: HTMLElement | null) {
  el?.focus();
  if (cmd !== "paste") return void document.execCommand(cmd === "selectAll" ? "selectAll" : cmd);
  const text = await api.clipRead().catch(() => "");
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    el.setRangeText(text, el.selectionStart ?? el.value.length, el.selectionEnd ?? el.value.length, "end");
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }
}

/** Kürzel so anzeigen, wie das System sie schreibt: macOS ⌘ ⇧ ⌥, sonst Strg/Umschalt/Alt. */
export function keyLabel(s: string) {
  return isMac ? s.replace(/Strg\+/g, "⌘").replace(/Umschalt\+/g, "⇧").replace(/Alt\+/g, "⌥").replace(/Entf/g, "⌫") : s;
}
