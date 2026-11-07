/** Lokales Terminal wie Windows Terminal: erkannte und eigene Profile, Standardprofil, «+»-Knopf, Strg+T.
 *  Ein lokales Terminal ist ein Shell-Tab mit der Pseudo-Verbindung `local:<Profil>` (gleiche Oberfläche wie SSH). */
import { invoke } from "@tauri-apps/api/core";
import type { Connection } from "./api";
import { openMenu } from "./menu";
import { openTab } from "./tabs";
import { tell } from "./ui";
import { TERM_THEMES } from "./termthemes";
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

/** Profil eines Terminal-Tabs (undefined = keine lokale Shell, sondern SSH). */
export const localProfile = (connId: string) => (connId.startsWith("local:") ? byConn.get(connId) : undefined);

/** Neues lokales Terminal öffnen; ohne `id` mit dem Standardprofil. */
export async function openTerminal(id?: string, cwd?: string) {
  const all = await allProfiles();
  const p = all.find((x) => x.id === (id || defaultProfileId())) ?? all[0];
  if (!p) return tell("Kein Terminal gefunden", "Auf diesem Computer wurde keine Shell erkannt. Unter Einstellungen › Terminal lässt sich ein eigenes Profil anlegen.");
  const connId = `local:${p.id}`;
  byConn.set(connId, cwd ? { ...p, cwd } : p);
  const conn = { id: connId, name: p.name, address: "Dieser Computer", user: "", group: "", sshPort: null, vncPort: null, rdpPort: null } as unknown as Connection;
  openTab(conn, "shell", true);
}

/** Auswahlmenü «+ ▾» mit allen Profilen. */
export async function profileMenu(anchor: HTMLElement) {
  const all = await allProfiles();
  const def = defaultProfileId() || all[0]?.id;
  const v = await openMenu(
    anchor,
    all.map((p, i) => ({ value: p.id, label: p.name, shortcut: p.id === def ? "Standard" : undefined, section: i === 0 ? "Neues Terminal" : p.custom && !all[i - 1]?.custom ? "Eigene Profile" : undefined })),
  );
  if (v) void openTerminal(v);
}

export function initTerminal() {
  initProfileSettings();
  for (const id of ["term-new", "lib-term-new"]) {
    const b = document.getElementById(id);
    if (b) b.onclick = () => void openTerminal();
  }
  for (const id of ["term-pick", "lib-term-pick"]) {
    const b = document.getElementById(id);
    if (b) b.onclick = () => void profileMenu(b);
  }
  // Strg+T / ⌘T: neues Terminal – nicht, solange ein entfernter Desktop die Tastatur hat.
  addEventListener("keydown", (e) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== "t") return;
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
  console.assert(localProfile("local:x")?.id === "x" && localProfile("abc") === undefined, "localProfile");
  byConn.delete("local:x");
}
