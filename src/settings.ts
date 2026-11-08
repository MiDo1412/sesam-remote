/** App-Einstellungen: Erscheinungsbild, Shell, Sicherheit, Über. Gespeichert lokal (keine Geheimnisse). */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { saveTermSettings, termSettings } from "./termthemes";
import { schemeGallery } from "./schemegallery";
import { ask } from "./ui";
import { enhanceSelect } from "./menu";
import { renderCredList } from "./creds";
import { renderProfiles } from "./termprofiles";
import { renderGroupSettings } from "./groups";

export type Appearance = "auto" | "light" | "dark";
const KEY = "sesam.app";

/** Wo Windows-Tastenkombinationen (Win, Alt+Tab …) wirken, wie in mstsc. */
export type SystemKeys = "remote" | "fullscreen" | "local";

export function appSettings(): { appearance: Appearance; systemKeys: SystemKeys } {
  try {
    return { appearance: "auto", systemKeys: "fullscreen", ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { appearance: "auto", systemKeys: "fullscreen" };
  }
}

/** Erscheinungsbild anwenden (auch Titelleiste). Andere Fenster folgen über das storage-Ereignis. */
export function applyAppearance() {
  const a = appSettings().appearance;
  if (a === "auto") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = a;
  getCurrentWindow().setTheme(a === "auto" ? null : a).catch(() => {});
  dispatchEvent(new CustomEvent("sesam:terminal-settings")); // «Automatisch» bei der Shell folgt mit
}

addEventListener("storage", (e) => {
  if (e.key === KEY) applyAppearance();
  if (e.key === "sesam.terminal") dispatchEvent(new CustomEvent("sesam:terminal-settings"));
});

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** Grösse des Einstellungsfensters (vom Benutzer gezogen) merken. */
const SIZE_KEY = "sesam.settingsSize";
let keysMenu: { sync(): void } | undefined;
let sizeWatch: ResizeObserver | null = null;

export async function openSettings(tab = "general") {
  const dlg = $<HTMLDialogElement>("settings");
  const box = dlg.querySelector<HTMLElement>(".settings")!;
  try {
    const sz = JSON.parse(localStorage.getItem(SIZE_KEY) ?? "null");
    if (sz && sz.w > 400 && sz.h > 300) (box.style.width = `${sz.w}px`), (box.style.height = `${sz.h}px`);
  } catch {}
  sizeWatch ??= new ResizeObserver(() => {
    if (!dlg.open || !box.style.width) return;
    try {
      localStorage.setItem(SIZE_KEY, JSON.stringify({ w: box.offsetWidth, h: box.offsetHeight }));
    } catch {}
  });
  sizeWatch.observe(box);
  const show = (t: string) => {
    for (const b of dlg.querySelectorAll<HTMLButtonElement>("[data-pane]")) b.setAttribute("aria-selected", String(b.dataset.pane === t));
    for (const p of dlg.querySelectorAll<HTMLElement>(".settings-pane")) p.hidden = p.id !== `settings-${t}`;
    if (t === "security") renderCerts();
    if (t === "creds") renderCredList();
    if (t === "terminal") void renderProfiles();
    if (t === "groups") renderGroupSettings();
  };
  for (const b of dlg.querySelectorAll<HTMLButtonElement>("[data-pane]")) b.onclick = () => show(b.dataset.pane!);

  // Allgemein
  const keys = dlg.querySelector<HTMLSelectElement>("[name=systemKeys]")!;
  keys.value = appSettings().systemKeys;
  keysMenu ??= enhanceSelect(keys);
  keysMenu.sync();
  keys.onchange = () => localStorage.setItem(KEY, JSON.stringify({ ...appSettings(), systemKeys: keys.value }));
  // shortcut: nur Windows fängt die Kombinationen ab (kbhook.rs); auf macOS fehlt der Ereignis-Tap noch.
  const onWin = navigator.userAgent.includes("Windows");
  $("system-keys-row").hidden = !onWin;
  ($("system-keys-row").nextElementSibling as HTMLElement).hidden = !onWin;
  for (const r of dlg.querySelectorAll<HTMLInputElement>("[name=appearance]")) {
    r.checked = r.value === appSettings().appearance;
    r.onchange = () => {
      localStorage.setItem(KEY, JSON.stringify({ ...appSettings(), appearance: r.value }));
      applyAppearance();
    };
  }

  // Shell
  const t = termSettings();
  const list = $("settings-schemes");
  // Galerie mit Vorschau; die Auswahl wirkt sofort auf offene Shells.
  list.replaceChildren(schemeGallery(t.theme, (id) => saveTermSettings({ ...termSettings(), theme: id })));
  const size = $<HTMLInputElement>("settings-fontsize");
  size.value = String(t.fontSize);
  size.onchange = () => saveTermSettings({ ...termSettings(), fontSize: Math.max(8, Math.min(32, Number(size.value) || 13)) });
  const fam = $<HTMLInputElement>("settings-fontfamily");
  fam.value = t.fontFamily;
  fam.onchange = () => saveTermSettings({ ...termSettings(), fontFamily: fam.value.trim() });

  // Über
  $("settings-version").textContent = await getVersion().catch(() => "");
  show(tab);
  if (!dlg.open) dlg.showModal();
}

async function renderCerts() {
  const box = $("settings-certs");
  const certs = await invoke<[string, string][]>("certs_list").catch(() => []);
  if (!certs.length) {
    box.textContent = "Noch keine Zertifikate gemerkt.";
    return;
  }
  box.replaceChildren(
    ...certs.map(([host, fp]) => {
      const row = document.createElement("div");
      row.className = "cert-row";
      const name = Object.assign(document.createElement("div"), { className: "cert-host", textContent: host });
      const code = Object.assign(document.createElement("code"), { textContent: fp.replace(/^SHA256:/, "").toLowerCase() });
      const del = Object.assign(document.createElement("button"), { type: "button", className: "link", textContent: "Entfernen" });
      del.onclick = async () => {
        if (!(await ask("Zertifikat vergessen?", `Beim nächsten Verbinden mit ${host} fragt Sesam wieder nach.`, "Vergessen"))) return;
        await invoke("cert_remove", { host });
        renderCerts();
      };
      const left = document.createElement("div");
      left.append(name, code);
      row.append(left, del);
      return row;
    }),
  );
}
