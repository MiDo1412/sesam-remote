/** «Berechtigungen einrichten …»: Freigaben des Systems mit Stand, je Zeile anfragen oder die Einstellung öffnen
 *  (src-tauri/src/perms.rs). Windows und Linux brauchen keine; dann sagt die Seite das. */
import { invoke } from "@tauri-apps/api/core";

interface Perm {
  id: string;
  label: string;
  purpose: string;
  state: "ok" | "missing" | "unknown";
}

const STATE = { ok: "Erteilt", missing: "Fehlt", unknown: "Wird beim ersten Mal gefragt" };

export async function openPermissions() {
  const dlg = document.createElement("dialog");
  dlg.className = "perms-setup";
  dlg.setAttribute("aria-labelledby", "perms-setup-title");
  dlg.innerHTML = `<form class="sheet" method="dialog">
      <h2 id="perms-setup-title">Berechtigungen einrichten</h2>
      <p class="note">Freigaben, die ${navigator.userAgent.includes("Mac") ? "macOS" : "das System"} für Sesam Remote verlangt. «Anfragen» zeigt den Dialog des Systems; wo der nicht mehr kommt, öffnet «Einstellungen» die richtige Stelle.</p>
      <ul class="perm-list"></ul>
      <div class="row actions"><span></span><button type="button" class="perms-all">Alle einrichten</button><button type="submit" class="primary">Fertig</button></div>
    </form>`;
  document.body.append(dlg);
  const list = dlg.querySelector<HTMLElement>(".perm-list")!;
  const all = dlg.querySelector<HTMLButtonElement>(".perms-all")!;
  const render = (perms: Perm[]) => {
    all.hidden = !perms.some((p) => p.state !== "ok");
    if (!perms.length) return void (list.innerHTML = `<li class="note">Auf diesem System braucht Sesam keine besonderen Freigaben: es verbindet nur nach aussen, dafür fragt die Firewall nicht.</li>`);
    list.replaceChildren(
      ...perms.map((p) => {
        const li = document.createElement("li");
        li.className = `perm ${p.state}`;
        const text = document.createElement("div");
        text.append(Object.assign(document.createElement("b"), { textContent: p.label }), Object.assign(document.createElement("small"), { textContent: p.purpose }));
        const st = Object.assign(document.createElement("span"), { className: "perm-state", textContent: STATE[p.state] });
        const ask = Object.assign(document.createElement("button"), { type: "button", textContent: "Anfragen", hidden: p.state === "ok" });
        ask.onclick = async () => render(await invoke<Perm[]>("perm_request", { id: p.id }));
        const open = Object.assign(document.createElement("button"), { type: "button", textContent: "Einstellungen" });
        open.onclick = () => void invoke("perm_open", { id: p.id }).catch(() => {});
        li.append(text, st, ask, open);
        return li;
      }),
    );
  };
  all.onclick = async () => {
    for (const p of await invoke<Perm[]>("perm_list")) if (p.state !== "ok") await invoke("perm_request", { id: p.id });
    render(await invoke<Perm[]>("perm_list"));
  };
  render(await invoke<Perm[]>("perm_list"));
  // Nach dem Umschalten in den Systemeinstellungen: beim Zurückkommen neu lesen.
  const refresh = () => void invoke<Perm[]>("perm_list").then(render);
  addEventListener("focus", refresh);
  dlg.onclose = () => (removeEventListener("focus", refresh), dlg.remove());
  dlg.showModal();
}
