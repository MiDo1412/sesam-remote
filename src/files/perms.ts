// Dialog «Berechtigungen» der Serverseite: Rechte als Häkchen oder Zahl, Besitzer, rekursiv.
import type { Entry } from "../api";

/** Fragt neue Rechte ab; `null` bei Abbrechen. `start` sind die bisherigen Rechte. */
export async function askPerms(path: string, e: Entry, owner: string) {
  const dlg = document.createElement("dialog");
  dlg.className = "perms-dlg";
  dlg.setAttribute("aria-labelledby", "perms-title");
  const who = ["Besitzer", "Gruppe", "Andere"];
  dlg.innerHTML = `<form class="sheet" method="dialog">
      <h2 id="perms-title">Berechtigungen</h2>
      <p class="note perms-path mono"></p>
      <table class="perms-grid"><thead><tr><th></th><th>Lesen</th><th>Schreiben</th><th>Ausführen</th></tr></thead><tbody>${who
        .map((w, i) => `<tr><th scope="row">${w}</th>${[4, 2, 1].map((b) => `<td><input type="checkbox" data-bit="${b << (3 * (2 - i))}" aria-label="${w}: ${b === 4 ? "Lesen" : b === 2 ? "Schreiben" : "Ausführen"}" /></td>`).join("")}</tr>`)
        .join("")}</tbody></table>
      <label class="row">Als Zahl<input name="octal" class="mono" inputmode="numeric" maxlength="4" size="5" /></label>
      <label class="row">Besitzer:Gruppe<input name="owner" class="mono" autocomplete="off" spellcheck="false" placeholder="benutzer:gruppe" /></label>
      <label class="row check"><input type="checkbox" name="recursive" />Auch für alles darin (rekursiv)</label>
      <p class="note">Wo dein Benutzer keine Rechte hat, fragt Sesam, ob es mit Administratorrechten (sudo) wiederholen soll.</p>
      <div class="row actions"><span></span><button type="button" class="perms-cancel">Abbrechen</button><button type="submit" class="primary">Übernehmen</button></div>
    </form>`;
  document.body.append(dlg);
  dlg.querySelector(".perms-path")!.textContent = path;
  const f = dlg.querySelector("form")!;
  const bits = [...dlg.querySelectorAll<HTMLInputElement>("[data-bit]")];
  const octal = f.elements.namedItem("octal") as HTMLInputElement;
  const own = f.elements.namedItem("owner") as HTMLInputElement;
  const rec = f.elements.namedItem("recursive") as HTMLInputElement;
  rec.closest("label")!.hidden = !e.dir;
  const start = e.perm ?? (e.dir ? 0o755 : 0o644);
  const special = start & 0o7000;
  const fromBits = () => bits.reduce((m, b) => m | (b.checked ? +b.dataset.bit! : 0), special);
  const show = (m: number) => {
    bits.forEach((b) => (b.checked = (m & +b.dataset.bit!) !== 0));
    octal.value = m.toString(8).padStart(3, "0");
  };
  show(start);
  own.value = owner;
  bits.forEach((b) => (b.onchange = () => (octal.value = fromBits().toString(8).padStart(3, "0"))));
  octal.oninput = () => /^[0-7]{3,4}$/.test(octal.value) && bits.forEach((b) => (b.checked = (parseInt(octal.value, 8) & +b.dataset.bit!) !== 0));
  const ok = await new Promise<boolean>((resolve) => {
    dlg.querySelector<HTMLButtonElement>(".perms-cancel")!.onclick = () => dlg.close();
    f.onsubmit = (ev) => {
      ev.preventDefault();
      if (!/^[0-7]{3,4}$/.test(octal.value)) return octal.setCustomValidity("Drei oder vier Ziffern von 0 bis 7, z. B. 644"), f.reportValidity(), octal.setCustomValidity("");
      resolve(true);
      dlg.close();
    };
    dlg.onclose = () => resolve(false);
    dlg.showModal();
  });
  dlg.remove();
  if (!ok) return null;
  const mode = parseInt(octal.value, 8);
  const newOwner = own.value.trim() && own.value.trim() !== owner ? own.value.trim() : null;
  const recursive = e.dir && rec.checked;
  return { mode, start, newOwner, recursive };
}
