/** App-Einstellungen: Erscheinungsbild, Shell, Sicherheit, Über. Gespeichert lokal (keine Geheimnisse). */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { resolveTheme, saveTermSettings, TERM_THEMES, termSettings } from "./termthemes";
import { ask } from "./ui";
import { renderCredList } from "./creds";
import { renderProfiles } from "./termprofiles";
import { renderGroupSettings } from "./groups";

export type Appearance = "auto" | "light" | "dark";
const KEY = "sesam.app";

export function appSettings(): { appearance: Appearance } {
  try {
    return { appearance: "auto", ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { appearance: "auto" };
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

export async function openSettings(tab = "general") {
  const dlg = $<HTMLDialogElement>("settings");
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
  list.replaceChildren(
    ...[["auto", "Automatisch (wie System)"], ...Object.entries(TERM_THEMES).map(([id, x]) => [id, x.label])].map(([id, label]) => {
      const th = resolveTheme(id);
      const l = document.createElement("label");
      l.className = "scheme";
      const swatch = document.createElement("span");
      swatch.className = "scheme-preview";
      swatch.style.background = th.background ?? "";
      swatch.style.color = th.foreground ?? "";
      swatch.innerHTML = `<b style="color:${th.green}">mike@host</b>:<span style="color:${th.blue}">~</span>$ ls`;
      const input = Object.assign(document.createElement("input"), { type: "radio", name: "termTheme", value: id, checked: t.theme === id });
      input.onchange = () => saveTermSettings({ ...termSettings(), theme: id });
      l.append(input, swatch, document.createTextNode(label));
      return l;
    }),
  );
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
