/** Lokales Terminal wie Windows Terminal: erkannte und eigene Profile, Standardprofil, «+»-Knopf, Strg+T.
 *  Ein lokales Terminal ist ein Shell-Tab mit der Pseudo-Verbindung `local:<Profil>` (gleiche Oberfläche wie SSH). */
import { isMobile } from "./titlebar";
import { invoke } from "@tauri-apps/api/core";
import type { Connection } from "./api";
import { ICON_CHEVRON, openMenu } from "./menu";
import { openTab } from "./tabs";
import { tell } from "./ui";
import { TERM_THEMES } from "./termthemes";
import { ICON } from "./icons";
import { initProfileSettings } from "./termprofiles";

export interface TermProfile {
  id: string;
  name: string;
  command: string;
  args: string[];
  /** Symbolname (siehe profiles.rs), bei eigenen Profilen frei. */
  icon: string;
  cwd?: string;
  env?: Record<string, string>;
  /** Farbschema (TERM_THEMES-ID) und Schrift nur für dieses Profil. */
  theme?: string;
  font?: string;
  custom?: boolean;
}

const KEY = "sesam.terminalProfiles";
const DEFAULT_KEY = "sesam.terminalDefault";
let detected: TermProfile[] | null = null;
/** Profil je geöffnetem Terminal-Tab (Verbindungs-ID → Profil inkl. Startordner). */
const byConn = new Map<string, TermProfile>();

export function customProfiles(): TermProfile[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "[]");
  } catch {
    return [];
  }
}
export function saveCustomProfiles(list: TermProfile[]) {
  localStorage.setItem(KEY, JSON.stringify(list.map((p) => ({ ...p, custom: true }))));
}
export async function allProfiles(): Promise<TermProfile[]> {
  detected ??= await invoke<TermProfile[]>("term_profiles").catch(() => []);
  return [...detected, ...customProfiles()];
}
export const defaultProfileId = () => localStorage.getItem(DEFAULT_KEY) ?? "";
export const setDefaultProfile = (id: string) => localStorage.setItem(DEFAULT_KEY, id);

/** Profil eines Terminal-Tabs (undefined = keine lokale Shell, sondern SSH). Steckt in der Pseudo-Verbindung,
 *  damit es auch in «Weitere Sitzung», abgekoppelten Fenstern und nach dem Wiederherstellen da ist. */
export const localProfile = (c: Connection): TermProfile | undefined =>
  c.id.startsWith("local:") ? ((c as Connection & { profile?: TermProfile }).profile ?? byConn.get(c.id)) : undefined;

/** Design wie das Vorbild des Systems: PowerShell blau, cmd schwarz, Ubuntu aubergine, macOS Terminal wie das System … */
export function systemTheme(p: TermProfile): string {
  const id = p.id.toLowerCase();
  const name = p.name.toLowerCase();
  if (id === "powershell" || id === "vs-pwsh" && name.includes("windows")) return "powershell";
  if (id === "cmd" || id === "vs-cmd") return "cmd";
  if (id.startsWith("wsl-")) return name.includes("ubuntu") ? "ubuntu" : "linux-console";
  if (navigator.userAgent.includes("Mac")) return "mac-basic";
  if (navigator.userAgent.includes("Linux")) return navigator.userAgent.includes("Ubuntu") ? "ubuntu" : "gnome";
  return "windows-terminal"; // pwsh 7, Git Bash und Interpreter wie im Windows Terminal
}

const THEMES_KEY = "sesam.terminalThemes";
/** Gewähltes Design je Profil: "" = Systemvorbild, "global" = wie in den Einstellungen (wie SSH), sonst TERM_THEMES-ID. */
export function profileThemeChoice(p: TermProfile): string {
  try {
    return JSON.parse(localStorage.getItem(THEMES_KEY) ?? "{}")[p.id] ?? (p.theme || "");
  } catch {
    return p.theme || "";
  }
}
export function setProfileTheme(id: string, theme: string) {
  let all: Record<string, string> = {};
  try {
    all = JSON.parse(localStorage.getItem(THEMES_KEY) ?? "{}");
  } catch {}
  if (theme) all[id] = theme;
  else delete all[id];
  localStorage.setItem(THEMES_KEY, JSON.stringify(all));
  dispatchEvent(new CustomEvent("sesam:terminal-settings"));
}
/** Wirksames Farbschema eines Profils; `global` = Schema aus den Einstellungen. */
export const profileTheme = (p: TermProfile, global: string) => {
  const v = profileThemeChoice(p);
  return v === "global" ? global : v || systemTheme(p);
};

/** Neues lokales Terminal öffnen; ohne `id` mit dem Standardprofil. */
export async function openTerminal(id?: string, cwd?: string) {
  const all = await allProfiles();
  const p = all.find((x) => x.id === (id || defaultProfileId())) ?? all[0];
  if (!p) return tell("Kein Terminal gefunden", "Auf diesem Computer wurde keine Shell erkannt. Unter Einstellungen › Terminal lässt sich ein eigenes Profil anlegen.");
  const connId = `local:${p.id}`;
  byConn.set(connId, cwd ? { ...p, cwd } : p);
  const conn = { id: connId, name: p.name, address: "Dieser Computer", user: "", group: "", sshPort: null, vncPort: null, rdpPort: null, profile: cwd ? { ...p, cwd } : p } as unknown as Connection;
  openTab(conn, "shell", true);
}

/** Symbol je Profil im Menü (16 px wie alle Menüsymbole). */
const profileIcon = (p: TermProfile) =>
  p.icon === "linux" ? ICON.osLinux.replace(/width="\d+" height="\d+"/, 'width="16" height="16"') : p.icon === "windows" ? ICON.osWindows.replace(/width="\d+" height="\d+"/, 'width="16" height="16"') : ICON.shell;

/** Auswahlmenü «+ ▾» mit allen Profilen. */
export async function profileMenu(anchor: HTMLElement) {
  const all = await allProfiles();
  const def = defaultProfileId() || all[0]?.id;
  const v = await openMenu(
    anchor,
    all.map((p, i) => ({ value: p.id, label: p.name, icon: profileIcon(p), shortcut: p.id === def ? "Standard" : undefined, section: i === 0 ? "Neues Terminal" : p.custom && !all[i - 1]?.custom ? "Eigene Profile" : undefined })),
  );
  if (v) void openTerminal(v);
}

export function initTerminal() {
  initProfileSettings();
  for (const id of ["term-new", "lib-term-new"]) {
    const b = document.getElementById(id);
    if (b) b.onclick = () => void openTerminal();
  }
  // Split-Knopf wie im Windows Terminal: Plus | Chevron
  const plus = document.getElementById("term-new");
  if (plus) plus.innerHTML = ICON.plus;
  for (const id of ["term-pick", "lib-term-pick"]) {
    const b = document.getElementById(id);
    if (b) b.innerHTML = ICON_CHEVRON;
  }
  for (const id of ["term-pick", "lib-term-pick"]) {
    const b = document.getElementById(id);
    if (b) b.onclick = () => void profileMenu(b);
  }
  // Strg+T / ⌘T: neues Terminal – nicht, solange ein entfernter Desktop die Tastatur hat.
  addEventListener("keydown", (e) => {
    if (isMobile || !(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== "t") return;
    if ((document.activeElement as HTMLElement | null)?.closest(".desktop") || document.querySelector("dialog[open]")) return;
    e.preventDefault();
    void openTerminal();
  });
}

/** Farbschema-Namen für die Profil-Auswahl in den Einstellungen. */
export const themeChoices = () => [["", "Wie in den Einstellungen"], ...Object.entries(TERM_THEMES).map(([id, t]) => [id, t.label])];

/** Selbstprüfung: Profil-ID und Verbindungs-ID passen zusammen. */
if (import.meta.env.DEV) {
  byConn.set("local:x", { id: "x", name: "X", command: "x", args: [], icon: "shell" });
  console.assert(localProfile({ id: "local:x" } as Connection)?.id === "x" && localProfile({ id: "abc" } as Connection) === undefined, "localProfile");
  console.assert(systemTheme({ id: "cmd", name: "Eingabeaufforderung", command: "", args: [], icon: "" }) === "cmd", "systemTheme cmd");
  byConn.delete("local:x");
}
