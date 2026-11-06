/** Gespeicherte Zugangsdaten: Liste in den Einstellungen, Bearbeiten-Dialog, Auswahl im Verbindungs-Sheet. */
import { api, type Cred } from "./api";
import { ask, tell } from "./ui";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export async function renderCredList() {
  const box = $("settings-cred-list");
  const list = await api.credsList().catch(() => []);
  if (!list.length) return void (box.textContent = "Noch keine gespeicherten Zugangsdaten.");
  box.replaceChildren(
    ...list.map(([c, uses]) => {
      const row = document.createElement("div");
      row.className = "cert-row";
      const left = document.createElement("div");
      left.append(
        Object.assign(document.createElement("div"), { className: "cert-host", textContent: c.name }),
        Object.assign(document.createElement("code"), {
          textContent: `${c.user || "ohne Benutzername"} · ${[c.hasPassword && "Passwort", c.hasKey && "SSH-Schlüssel"].filter(Boolean).join(", ") || "ohne Geheimnis"} · ${uses} Verbindung(en)`,
        }),
      );
      const edit = Object.assign(document.createElement("button"), { type: "button", className: "link", textContent: "Bearbeiten" });
      edit.onclick = () => openCred(c);
      const del = Object.assign(document.createElement("button"), { type: "button", className: "link", textContent: "Löschen" });
      del.onclick = async () => {
        if (!(await ask("Zugangsdaten löschen?", uses ? `${uses} Verbindung(en) nutzen sie; diese fallen auf «Eigene» zurück (dann ohne Passwort).` : "Benutzername und Geheimnisse werden entfernt.", "Löschen"))) return;
        await api.credDelete(c.id);
        renderCredList();
        dispatchEvent(new CustomEvent("sesam:creds-changed"));
      };
      const actions = document.createElement("div");
      actions.className = "row-actions";
      actions.append(edit, del);
      row.append(left, actions);
      return row;
    }),
  );
}

/** Dialog zum Anlegen/Ändern. Gibt die gespeicherten Daten zurück oder null. */
export function openCred(existing: Cred | null): Promise<Cred | null> {
  const dlg = $<HTMLDialogElement>("cred-dlg");
  const form = $<HTMLFormElement>("cred-form");
  const f = form.elements as unknown as Record<string, HTMLInputElement>;
  const cred: Cred = existing ?? { id: crypto.randomUUID(), name: "", user: "", hasPassword: false, hasKey: false };
  f.name.value = cred.name;
  f.user.value = cred.user;
  f.password.value = "";
  f.key.value = "";
  $("cred-pw-hint").textContent = cred.hasPassword ? "Gespeichert. Leer lassen, um es zu behalten." : "";
  $("cred-error").textContent = "";
  dlg.showModal();
  return new Promise((resolve) => {
    $("cred-cancel").onclick = () => (dlg.close(), resolve(null));
    form.onsubmit = async (ev) => {
      ev.preventDefault();
      try {
        const key = f.key.value.trim();
        const saved = await api.credSave({ ...cred, name: f.name.value.trim(), user: f.user.value.trim() }, f.password.value || undefined, key ? key + "\n" : undefined);
        dlg.close();
        renderCredList();
        dispatchEvent(new CustomEvent("sesam:creds-changed"));
        resolve(saved);
      } catch (e) {
        $("cred-error").textContent = String(e);
      }
    };
  });
}

export function initCreds() {
  $("cred-new").onclick = () => openCred(null);
}

export async function adoptFromConnection(connId: string, suggested: string) {
  const name = suggested || "Zugangsdaten";
  try {
    return await api.credFromConnection(connId, crypto.randomUUID(), name);
  } catch (e) {
    tell("Nicht übernommen", String(e));
    return null;
  }
}
