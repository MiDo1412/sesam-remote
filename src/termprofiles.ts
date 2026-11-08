/** Einstellungen › Terminal: Standardprofil wählen, eigene Profile anlegen/ändern/löschen. */
import { allProfiles, customProfiles, defaultProfileId, profileThemeChoice, saveCustomProfiles, setDefaultProfile, setProfileTheme, systemTheme, themeChoices, type TermProfile } from "./terminal";
import { TERM_THEMES } from "./termthemes";
import { enhanceSelect } from "./menu";
import { ask } from "./ui";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export async function renderProfiles() {
  const box = $("settings-profiles");
  const all = await allProfiles();
  const def = defaultProfileId() || all[0]?.id;
  box.replaceChildren(
    ...all.map((p) => {
      const row = document.createElement("label");
      row.className = "cert-row profile-row";
      const radio = Object.assign(document.createElement("input"), { type: "radio", name: "termDefault", value: p.id, checked: p.id === def });
      radio.onchange = () => setDefaultProfile(p.id);
      const text = document.createElement("div");
      text.append(
        Object.assign(document.createElement("div"), { className: "cert-host", textContent: p.name + (p.custom ? "" : " (erkannt)") }),
        Object.assign(document.createElement("div"), { className: "cmd", textContent: [p.command, ...p.args].join(" ") }),
      );
      // Design je Profil: Systemvorbild (z. B. PowerShell blau), wie SSH (Einstellungen › Shell) oder ein bestimmtes.
      const design = document.createElement("select");
      design.setAttribute("aria-label", `Design für ${p.name}`);
      design.append(
        new Option(`Systemvorbild (${TERM_THEMES[systemTheme(p)]?.label ?? "Standard"})`, ""),
        new Option("Wie unter «Shell» eingestellt", "global"),
        ...Object.entries(TERM_THEMES).map(([v, t]) => new Option(t.label, v)),
      );
      design.value = profileThemeChoice(p);
      design.onchange = () => setProfileTheme(p.id, design.value);
      const designWrap = document.createElement("div");
      designWrap.className = "profile-design";
      designWrap.onclick = (e) => e.preventDefault(); // Klick im Label wählt sonst das Standardprofil
      designWrap.append(design);
      row.append(radio, text, designWrap);
      enhanceSelect(design);
      if (p.custom) {
        const edit = Object.assign(document.createElement("button"), { type: "button", className: "link", textContent: "Bearbeiten" });
        edit.onclick = (e) => (e.preventDefault(), void editProfile(p));
        const del = Object.assign(document.createElement("button"), { type: "button", className: "link", textContent: "Löschen" });
        del.onclick = async (e) => {
          e.preventDefault();
          if (!(await ask("Profil löschen?", `«${p.name}» wird entfernt.`, "Löschen"))) return;
          saveCustomProfiles(customProfiles().filter((x) => x.id !== p.id));
          void renderProfiles();
        };
        const actions = document.createElement("div");
        actions.className = "row-actions";
        actions.append(edit, del);
        row.append(actions);
      }
      return row;
    }),
  );
}

/** Dialog für ein eigenes Profil. Umgebungsvariablen als Zeilen NAME=Wert. */
function editProfile(existing: TermProfile | null): Promise<void> {
  const dlg = document.createElement("dialog");
  dlg.innerHTML = `
    <form class="sheet" method="dialog" style="width:min(560px,calc(100vw - 48px))">
      <h2>Terminal-Profil</h2>
      <label class="span">Name<input name="name" required /></label>
      <label class="span">Programm<input name="command" required class="mono" spellcheck="false" placeholder="z. B. C:\\\\Program Files\\\\PowerShell\\\\7\\\\pwsh.exe oder /bin/zsh" /></label>
      <label class="span">Argumente<input name="args" class="mono" spellcheck="false" placeholder="durch Leerzeichen getrennt, z. B. -NoLogo" /></label>
      <label class="span">Startordner<input name="cwd" class="mono" spellcheck="false" placeholder="leer = persönlicher Ordner" /></label>
      <label class="span">Umgebungsvariablen<textarea name="env" rows="3" class="mono" spellcheck="false" placeholder="NAME=Wert, eine pro Zeile"></textarea></label>
      <label>Farbschema<select name="theme"></select></label>
      <label>Schrift<input name="font" placeholder="wie in den Einstellungen" /></label>
      <div class="row actions span"><span></span><button type="button" data-cancel>Abbrechen</button><button type="submit" class="primary">Sichern</button></div>
    </form>`;
  document.body.append(dlg);
  const form = dlg.querySelector("form")!;
  const f = form.elements as unknown as Record<string, HTMLInputElement>;
  const theme = form.elements.namedItem("theme") as HTMLSelectElement;
  theme.replaceChildren(...themeChoices().map(([v, l]) => new Option(l, v)));
  const p = existing;
  f.name.value = p?.name ?? "";
  f.command.value = p?.command ?? "";
  f.args.value = p?.args.join(" ") ?? "";
  f.cwd.value = p?.cwd ?? "";
  f.env.value = Object.entries(p?.env ?? {}).map(([k, v]) => `${k}=${v}`).join("\n");
  theme.value = p?.theme ?? "";
  f.font.value = p?.font ?? "";
  dlg.showModal();
  return new Promise((resolve) => {
    const done = () => (dlg.close(), dlg.remove(), resolve());
    dlg.querySelector<HTMLButtonElement>("[data-cancel]")!.onclick = done;
    form.onsubmit = (ev) => {
      ev.preventDefault();
      const env = Object.fromEntries(
        f.env.value
          .split("\n")
          .map((l) => l.trim())
          .filter((l) => l.includes("="))
          .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1)]),
      );
      const next: TermProfile = {
        id: p?.id ?? `custom-${crypto.randomUUID().slice(0, 8)}`,
        name: f.name.value.trim(),
        command: f.command.value.trim(),
        args: f.args.value.trim() ? f.args.value.trim().split(/\s+/) : [],
        icon: "shell",
        cwd: f.cwd.value.trim() || undefined,
        env,
        theme: theme.value || undefined,
        font: f.font.value.trim() || undefined,
        custom: true,
      };
      saveCustomProfiles([...customProfiles().filter((x) => x.id !== next.id), next]);
      done();
      void renderProfiles();
    };
  });
}

export function initProfileSettings() {
  const b = document.getElementById("profile-new");
  if (b) b.onclick = () => void editProfile(null);
}
