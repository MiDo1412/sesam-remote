/** «Desktop freischalten»: Gerät mit nur SSH erkennen (src-tauri/src/unlock.rs), die genauen Befehle zeigen und
 *  erst nach Klick auf «Ausführen» mit sudo ausführen. Ubuntu: das Setup-Script startet interaktiv in einer Shell. */
import { invoke } from "@tauri-apps/api/core";
import { HOSTKEY_UNKNOWN, type Connection } from "./api";
import { runOnTarget } from "./setup";
import { askHostKey, askPassword, tell } from "./ui";

interface Plan {
  system: "" | "ubuntu" | "raspberry" | "macos";
  label: string;
  protocol: "" | "rdp" | "vnc";
  port: number;
  commands: string[];
  note: string;
}

export async function unlockDesktop(c: Connection, done: (c: Connection, opened: boolean) => void) {
  const dlg = document.createElement("dialog");
  dlg.className = "unlock-desktop";
  dlg.setAttribute("aria-labelledby", "unlock-title");
  dlg.innerHTML = `<form class="sheet" method="dialog">
      <h2 id="unlock-title"></h2>
      <p class="note unlock-what">Sesam fragt das Gerät per SSH, welches System läuft …</p>
      <pre class="unlock-cmds" hidden></pre>
      <p class="note unlock-note" hidden></p>
      <div class="row actions"><span></span><button type="submit">Abbrechen</button><button type="button" class="primary run" hidden>Ausführen</button></div>
    </form>`;
  dlg.querySelector("h2")!.textContent = `Desktop freischalten · ${c.name}`;
  const what = dlg.querySelector<HTMLElement>(".unlock-what")!;
  const cmds = dlg.querySelector<HTMLElement>(".unlock-cmds")!;
  const note = dlg.querySelector<HTMLElement>(".unlock-note")!;
  const run = dlg.querySelector<HTMLButtonElement>(".run")!;
  document.body.append(dlg);
  dlg.onclose = () => dlg.remove();
  dlg.showModal();

  let trust: string | null = null;
  let plan: Plan;
  for (;;) {
    try {
      plan = await invoke<Plan>("desktop_detect", { id: c.id, trust });
      break;
    } catch (e) {
      const msg = String(e);
      if (trust === null && msg.startsWith(HOSTKEY_UNKNOWN) && (trust = await askHostKey(c.address, msg))) continue;
      what.textContent = `Nicht erkannt: ${msg}`;
      return;
    }
  }
  if (!dlg.open) return;
  const desk = plan.protocol === "rdp" ? `RDP (Port ${plan.port})` : `VNC (Port ${plan.port})`;
  what.textContent = plan.system ? `Erkannt: ${plan.label}. Sesam schaltet ${desk} ein und trägt es in der Verbindung ein. Mit sudo ausgeführt wird genau das:` : `Erkannt: ${plan.label}.`;
  cmds.textContent = plan.commands.join("\n");
  cmds.hidden = !plan.commands.length;
  note.textContent = plan.note;
  note.hidden = false;
  run.hidden = !plan.system;
  run.textContent = plan.system === "ubuntu" ? "Setup-Script starten" : "Mit sudo ausführen";
  run.onclick = async () => {
    run.disabled = true;
    try {
      if (plan.system === "ubuntu") {
        dlg.close();
        await runOnTarget(c);
        await invoke("desktop_enable", { id: c.id, trust, system: plan.system });
        return done(c, false);
      }
      let password: string | undefined;
      for (let i = 0; ; i++) {
        try {
          await invoke("desktop_enable", { id: c.id, trust, system: plan.system, password });
          break;
        } catch (e) {
          const msg = String(e);
          if (i > 1 || !(msg === "SUDO_PASSWORD" || msg.includes("stimmt nicht"))) throw e;
          const pw = await askPassword("Passwort für sudo", `sudo auf ${c.name} braucht das Passwort von ${c.user || "diesem Benutzer"}. Es wird nicht gespeichert.`, "Ausführen");
          if (!pw) return void (run.disabled = false);
          password = pw;
        }
      }
      dlg.close();
      await tell("Desktop freigeschaltet", `${desk} ist auf ${c.name} eingeschaltet und in der Verbindung eingetragen.`);
      done(c, true);
    } catch (e) {
      run.disabled = false;
      what.textContent = `Nicht freigeschaltet: ${e}`;
    }
  };
}
