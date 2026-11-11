/** Befehlspalette (Strg+Umschalt+P, macOS ⌘⇧P): Verbindungen öffnen und Aktionen ausführen, per Tastatur filterbar. */
import type { Connection } from "./api";

export interface Command {
  label: string;
  /** Kleiner Zusatz rechts, z. B. Adresse oder Tastenkürzel. */
  hint?: string;
  /** Weitere Suchbegriffe. */
  keywords?: string;
  run: () => void;
}

/** Treffer-Bewertung: alle Wörter der Suche müssen vorkommen; Anfang des Labels zählt mehr. */
export function score(cmd: Command, query: string): number {
  const hay = `${cmd.label} ${cmd.hint ?? ""} ${cmd.keywords ?? ""}`.toLowerCase();
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return 1;
  let s = 0;
  for (const w of words) {
    const i = hay.indexOf(w);
    if (i < 0) return 0;
    s += i === 0 ? 3 : cmd.label.toLowerCase().includes(w) ? 2 : 1;
  }
  return s;
}

export function initPalette(commands: () => Command[]) {
  const dlg = document.createElement("dialog");
  dlg.className = "palette";
  dlg.setAttribute("aria-label", "Befehlspalette");
  dlg.innerHTML = `<input type="search" placeholder="Verbindung oder Befehl suchen …" aria-label="Suchen" aria-controls="palette-list" /><ul id="palette-list" role="listbox"></ul>`;
  document.body.append(dlg);
  const input = dlg.querySelector("input")!;
  const list = dlg.querySelector("ul")!;
  let shown: Command[] = [];
  let sel = 0;
  const draw = () => {
    const q = input.value.trim();
    shown = commands()
      .map((c) => [c, score(c, q)] as const)
      .filter(([, s]) => s > 0)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 50)
      .map(([c]) => c);
    sel = Math.min(sel, Math.max(0, shown.length - 1));
    list.replaceChildren(
      ...shown.map((c, i) => {
        const li = document.createElement("li");
        li.setAttribute("role", "option");
        li.setAttribute("aria-selected", String(i === sel));
        li.id = `palette-${i}`;
        const l = Object.assign(document.createElement("span"), { textContent: c.label });
        const h = Object.assign(document.createElement("small"), { textContent: c.hint ?? "" });
        li.append(l, h);
        li.onmousemove = () => i !== sel && ((sel = i), draw());
        li.onclick = () => run(i);
        return li;
      }),
    );
    if (!shown.length) list.append(Object.assign(document.createElement("li"), { className: "empty", textContent: "Nichts gefunden" }));
    input.setAttribute("aria-activedescendant", shown.length ? `palette-${sel}` : "");
    list.querySelector("[aria-selected=true]")?.scrollIntoView({ block: "nearest" });
  };
  const run = (i: number) => {
    const c = shown[i];
    dlg.close();
    c?.run();
  };
  input.oninput = () => ((sel = 0), draw());
  input.onkeydown = (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      sel = (sel + (e.key === "ArrowDown" ? 1 : -1) + shown.length) % Math.max(1, shown.length);
      draw();
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(sel);
    }
  };
  dlg.onclick = (e) => e.target === dlg && dlg.close(); // Klick neben die Palette schliesst
  const open = () => {
    if (document.querySelector("dialog[open]")) return;
    input.value = "";
    sel = 0;
    draw();
    dlg.showModal();
    input.focus();
  };
  addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && !e.altKey && e.key.toLowerCase() === "p") {
      if ((document.activeElement as HTMLElement | null)?.closest(".desktop")) return; // Desktop behält die Tastatur
      e.preventDefault();
      open();
    }
  });
  return { open };
}

/** Befehle für eine Verbindung: Desktop, Shell, Dateien – nur was sie anbietet. */
export function connectionCommands(c: Connection, open: (c: Connection, view: "desktop" | "shell" | "files") => void, has: { desktop: boolean; files: boolean }): Command[] {
  const out: Command[] = [];
  const hint = c.address;
  if (has.desktop) out.push({ label: `${c.name}: Desktop öffnen`, hint, keywords: `${c.group} rdp vnc`, run: () => open(c, "desktop") });
  if (c.sshPort) out.push({ label: `${c.name}: Shell öffnen`, hint, keywords: `${c.group} ssh terminal`, run: () => open(c, "shell") });
  if (has.files) out.push({ label: `${c.name}: Dateien öffnen`, hint, keywords: `${c.group} sftp ftp`, run: () => open(c, "files") });
  return out;
}

// Selbstprüfung im Entwicklungsbuild
if (import.meta.env?.DEV) {
  const c: Command = { label: "Werkstatt: Desktop öffnen", hint: "192.168.1.20", run: () => {} };
  console.assert(score(c, "") === 1 && score(c, "werkstatt desk") > 0 && score(c, "windows") === 0 && score(c, "werkstatt") > score(c, "desk"), "palette score");
}
