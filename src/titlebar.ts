/** Eigene Titelleiste: unter Windows ohne Fensterrahmen (Logo links, Menü «⋯» und Fenstersteuerung rechts),
 *  unter macOS bleibt die Menüleiste nativ und die Titelleiste ist transparent (Ampel links). */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openMenu, type MenuItem } from "./menu";
import { fl } from "./icons";
import minusSvg from "@fluentui/svg-icons/icons/subtract_20_regular.svg?raw";
import squareSvg from "@fluentui/svg-icons/icons/square_20_regular.svg?raw";
import restoreSvg from "@fluentui/svg-icons/icons/square_multiple_20_regular.svg?raw";
import closeSvg from "@fluentui/svg-icons/icons/dismiss_20_regular.svg?raw";
import menuSvg from "@fluentui/svg-icons/icons/navigation_20_regular.svg?raw";

// Eigene Symbole statt der Windows-Symbolschrift (die nicht überall verfügbar ist und dann bunt ersetzt wird).
const glyph = (svg: string) => fl(svg, 14);

export const isMac = navigator.userAgent.includes("Mac");

export function initTitlebar(actions: { command(c: string): void; tool(t: string): void; help(id: string): void }) {
  const bars = document.querySelectorAll<HTMLElement>("header.toolbar, header.session-bar");
  for (const h of bars) h.setAttribute("data-tauri-drag-region", "");
  if (isMac) return void document.body.classList.add("mac");
  document.body.classList.add("custom-titlebar");
  const win = getCurrentWindow();

  // Kurzes App-Menü (☰) oben rechts (wie Edge/Teams), höchstens 7 Einträge. Alles Weitere über die Befehlspalette,
  // Kontextmenüs, Kürzel und die Verbindungsleiste.
  const items: MenuItem[] = [
    { value: "cmd:new", label: "Neue Verbindung …", shortcut: "Strg+N" },
    { value: "cmd:quick", label: "Schnelle Verbindung …", shortcut: "Strg+K" },
    { value: "cmd:terminal", label: "Neues Terminal", shortcut: "Strg+T" },
    { value: "cmd:palette", label: "Befehlspalette", shortcut: "Strg+Umschalt+P", separator: true },
    { value: "cmd:settings", label: "Einstellungen", shortcut: "Strg+," },
    { value: "help:", label: "Hilfe" },
    { value: "cmd:about", label: "Über Sesam Remote" },
  ];
  const menu = async (anchor: HTMLElement) => {
    const v = await openMenu(anchor, items);
    if (!v) return;
    const [kind, arg] = [v.slice(0, v.indexOf(":")), v.slice(v.indexOf(":") + 1)];
    if (kind === "cmd") actions.command(arg);
    else actions.help(arg);
  };

  for (const h of bars) {
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
    const more = mk(fl(menuSvg, 16), "Menü (Alt)", () => void menu(more), "more");
    more.setAttribute("aria-haspopup", "menu");
    more.setAttribute("aria-keyshortcuts", "Alt");
    const sep = Object.assign(document.createElement("span"), { className: "win-sep" });
    sep.setAttribute("aria-hidden", "true");
    ctl.append(more, sep, mk(glyph(minusSvg), "Minimieren", () => win.minimize()), max, mk(glyph(closeSvg), "Schliessen", () => win.close(), "close"));
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

  // Alt allein öffnet das Menü wie in Windows-Apps. Nicht, solange das Remote-Bild den Fokus hat (Alt gehört dann nach drüben).
  let altAlone = false;
  addEventListener("keydown", (e) => (altAlone = e.key === "Alt" && !e.repeat && !(document.activeElement instanceof HTMLCanvasElement)), true);
  addEventListener("pointerdown", () => (altAlone = false), true);
  addEventListener("keyup", (e) => {
    if (e.key !== "Alt" || !altAlone) return;
    altAlone = false;
    const btn = [...document.querySelectorAll<HTMLElement>(".win-btn.more")].find((b) => b.offsetParent !== null);
    if (btn && !document.querySelector("dialog[open]")) {
      e.preventDefault();
      void menu(btn);
    }
  }, true);
}

/** Kürzel so anzeigen, wie das System sie schreibt: macOS ⌘ ⇧ ⌥, sonst Strg/Umschalt/Alt. */
export function keyLabel(s: string) {
  return isMac ? s.replace(/Strg\+/g, "⌘").replace(/Umschalt\+/g, "⇧").replace(/Alt\+/g, "⌥").replace(/Entf/g, "⌫") : s;
}
