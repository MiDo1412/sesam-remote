/** App-Einstellungen: Erscheinungsbild, Shell, Sicherheit, Über. Gespeichert lokal (keine Geheimnisse). */
import { isMac, isMobile } from "./titlebar";
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
    return { appearance: "auto", systemKeys: "remote", ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { appearance: "auto", systemKeys: "remote" };
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
let trashMenu: ReturnType<typeof enhanceSelect> | undefined;
let trashMaxMenu: ReturnType<typeof enhanceSelect> | undefined;
let keysMenu: { sync(): void } | undefined;
let sizeWatch: ResizeObserver | null = null;

/** Dieses Fenster ist das eigene Einstellungsfenster (macOS, lib.rs open_settings_window). */
export const settingsWindow: string = (window as unknown as { __SESAM_SETTINGS__?: string }).__SESAM_SETTINGS__ ?? "";

export async function openSettings(tab = "general") {
  // macOS: Einstellungen in einem eigenen Fenster (HIG), sonst als Sheet über der Bibliothek.
  if (isMac && !settingsWindow) return void invoke("open_settings_window", { tab }).catch(() => {});
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
  // Fokus wandert mit der Auswahl (WebKit fokussiert Knöpfe beim Klick nicht), sonst stehen Ring und Auswahl auf zwei Zeilen.
  for (const b of dlg.querySelectorAll<HTMLButtonElement>("[data-pane]")) b.onclick = () => (show(b.dataset.pane!), b.focus({ preventScroll: true }));

  // Allgemein
  const trash = $<HTMLSelectElement>("settings-trash-days");
  trash.value = localStorage.getItem("sesam.trashDays") ?? "7";
  trash.onchange = () => localStorage.setItem("sesam.trashDays", trash.value);
  trashMenu ??= enhanceSelect(trash);
  trashMenu.sync();
  const trashMax = $<HTMLSelectElement>("settings-trash-max");
  trashMax.value = localStorage.getItem("sesam.trashMaxMb") ?? "500";
  trashMax.onchange = () => localStorage.setItem("sesam.trashMaxMb", trashMax.value);
  trashMaxMenu ??= enhanceSelect(trashMax);
  trashMaxMenu.sync();
  const keys = dlg.querySelector<HTMLSelectElement>("[name=systemKeys]")!;
  keys.value = appSettings().systemKeys;
  keysMenu ??= enhanceSelect(keys);
  keysMenu.sync();
  keys.onchange = () => localStorage.setItem(KEY, JSON.stringify({ ...appSettings(), systemKeys: keys.value }));
  // Windows und macOS fangen die Kombinationen ab (kbhook.rs), Linux noch nicht.
  const row = $("system-keys-row");
  const note = row.nextElementSibling as HTMLElement;
  row.hidden = note.hidden = isMobile || !/Windows|Mac/.test(navigator.userAgent);
  if (isMac) {
    row.querySelector("span")!.textContent = "Mac-Tastenkürzel an den Remotecomputer";
    note.textContent =
      "Gilt für ⌘Tab, ⌘`, ⌘Leertaste, ⌃Leertaste, ⌃Pfeiltasten und ⌘⇧3/4/5; ⌘ kommt drüben als Windows-Taste an. ⌘Q, ⌘H, ⌘W und ⌘⌥Esc bleiben beim Mac. «Aktiv» heisst: Sesam ist vorne und das Remote-Bild hat den Fokus oder die Maus ist darüber. Dafür braucht Sesam einmal die Freigabe von macOS (Datenschutz & Sicherheit).";
  }
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
  $("settings-version").textContent = __VERSION_LABEL__ || (await getVersion().catch(() => ""));
  void renderUpdate();
  show(tab);
  if (!dlg.open) dlg.showModal();
  // Fokus auf den gewählten Bereich (nicht immer «Allgemein»), damit Tastatur und Anzeige übereinstimmen.
  dlg.querySelector<HTMLButtonElement>(`[data-pane="${tab}"]`)?.focus({ preventScroll: true });
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

// ---------- Automatische Updates (src-tauri/src/update.rs) ----------
interface UpdateStatus {
  current: string;
  auto: boolean;
  locked: boolean;
  checking: boolean;
  ready: string | null;
  install: "onQuit" | "manual" | "never" | null;
  error: string | null;
}

function updateLine(s: UpdateStatus): string {
  if (s.checking) return "Suche nach Updates …";
  if (s.ready) return s.install === "manual" ? `Update ${s.ready} ist verfügbar.` : `Update ${s.ready} wird beim nächsten Start installiert.`;
  if (s.error) return `Update-Prüfung nicht möglich: ${s.error}`;
  if (!s.auto) return s.locked ? "Updates verteilt die IT (Richtlinie dieses Computers)." : "Automatisch aktualisieren ist aus.";
  return `Sesam ist aktuell (${s.current}).`;
}

async function renderUpdate(s?: UpdateStatus) {
  const box = $("update-box");
  try {
    s ??= await invoke<UpdateStatus>("update_status");
  } catch {
    return;
  }
  if (s.install === "never") return void (box.hidden = true);
  box.hidden = false;
  $("update-line").textContent = updateLine(s);
  const auto = $<HTMLInputElement>("update-auto");
  auto.checked = s.auto;
  auto.disabled = s.locked;
  auto.onchange = async () => {
    await invoke("update_set_auto", { auto: auto.checked }).catch(() => {});
    void renderUpdate();
  };
  const check = $<HTMLButtonElement>("update-check");
  check.disabled = s.checking || (s.locked && !s.auto);
  check.onclick = async () => {
    $("update-line").textContent = "Suche nach Updates …";
    check.disabled = true;
    void renderUpdate(await invoke<UpdateStatus>("update_check").catch((e) => ({ ...s!, checking: false, error: String(e) })));
  };
  const install = $<HTMLButtonElement>("update-install");
  install.hidden = !(s.ready && s.install === "manual");
  install.onclick = () => void invoke("update_install").catch((e) => ($("update-line").textContent = `Installation nicht möglich: ${e}`));
}
