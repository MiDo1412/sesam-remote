/** Gespeicherte Zugangsdaten: Liste in den Einstellungen, Bearbeiten-Dialog, Auswahl im Verbindungs-Sheet. */
import { api, HOSTKEY_UNKNOWN, type Cred } from "./api";
import { ask, askHostKey, tell } from "./ui";
import { openMenu } from "./menu";
import { ICON } from "./icons";

const byId = (id: string) => document.getElementById(id)!;

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export async function renderCredList() {
  const box = $("settings-cred-list");
  const list = await api.credsList().catch(() => []);
  if (!list.length) return void (box.textContent = "Noch keine gespeicherten Zugangsdaten.");
  box.replaceChildren(
    ...list.map(([c, users]) => {
      const row = document.createElement("div");
      row.className = "cert-row";
      const left = document.createElement("div");
      left.append(
        Object.assign(document.createElement("div"), { className: "cert-host", textContent: c.name }),
        Object.assign(document.createElement("code"), {
          textContent: `${c.user || "ohne Benutzername"} · ${[c.hasPassword && "Passwort", c.hasKey && "SSH-Schlüssel"].filter(Boolean).join(", ") || "ohne Geheimnis"}`,
        }),
        Object.assign(document.createElement("small"), { className: "cred-users", textContent: users.length ? `Verwendet von: ${users.join(", ")}` : "Von keiner Verbindung verwendet" }),
      );
      const edit = Object.assign(document.createElement("button"), { type: "button", className: "link", textContent: "Bearbeiten" });
      edit.onclick = () => openCred(c);
      const more = Object.assign(document.createElement("button"), { type: "button", className: "link", textContent: "Weitere …" });
      more.setAttribute("aria-haspopup", "menu");
      more.onclick = () => void credMenu(more, c, users);
      const actions = document.createElement("div");
      actions.className = "row-actions";
      actions.append(edit, more);
      row.append(left, actions);
      return row;
    }),
  );
}

const changed = () => (renderCredList(), dispatchEvent(new CustomEvent("sesam:creds-changed")));

/** Weitere Aktionen: Schlüssel, Passwort zeigen, duplizieren, löschen. */
async function credMenu(anchor: HTMLElement, c: Cred, users: string[]) {
  const mac = navigator.userAgent.includes("Mac");
  const v = await openMenu(anchor, [
    { value: "gen", label: c.hasKey ? "Neuen SSH-Schlüssel erzeugen …" : "SSH-Schlüssel erzeugen (Ed25519)", icon: ICON.plus },
    { value: "pub", label: "Öffentlichen Schlüssel kopieren", icon: ICON.copy, disabled: !c.hasKey },
    { value: "install", label: "Schlüssel auf einem Server eintragen …", icon: ICON.upload, disabled: !c.hasKey },
    ...(mac ? [{ value: "reveal", label: "Passwort anzeigen …", disabled: !c.hasPassword, separator: true }] : []),
    { value: "dup", label: "Duplizieren", separator: !mac },
    { value: "del", label: "Löschen …", icon: ICON.trash, separator: true },
  ]);
  try {
    if (v === "gen") {
      if (c.hasKey && !(await ask("Neuen Schlüssel erzeugen?", "Der bisherige private Schlüssel wird ersetzt. Server, auf denen nur der alte eingetragen ist, nehmen dann keine Anmeldung mit diesen Zugangsdaten mehr an.", "Ersetzen"))) return;
      const pub = await api.credGenerateKey(c.id);
      await api.clipWrite(pub);
      changed();
      tell("SSH-Schlüssel erzeugt", `Der öffentliche Schlüssel liegt in der Zwischenablage. Über «Schlüssel auf einem Server eintragen …» bringst du ihn auf einen Server.\n\n${pub}`);
    } else if (v === "pub") {
      await api.clipWrite(await api.credPublicKey(c.id));
      tell("Kopiert", "Der öffentliche Schlüssel liegt in der Zwischenablage (eine Zeile für ~/.ssh/authorized_keys).");
    } else if (v === "install") await installKey(c);
    else if (v === "reveal") {
      const pw = await api.credReveal(c.id);
      if (await ask(`Passwort von «${c.name}»`, pw, "Kopieren", "Schliessen")) await api.clipWrite(pw);
    } else if (v === "dup") {
      await api.credDuplicate(c.id, crypto.randomUUID(), `${c.name} (Kopie)`);
      changed();
    } else if (v === "del") {
      if (!(await ask("Zugangsdaten löschen?", users.length ? `Verwendet von: ${users.join(", ")}. Diese Verbindungen fallen auf «Eigene» zurück (dann ohne Passwort).` : "Benutzername und Geheimnisse werden entfernt.", "Löschen"))) return;
      await api.credDelete(c.id);
      changed();
    }
  } catch (e) {
    if (String(e) !== "Nicht bestätigt") tell("Nicht ausgeführt", String(e));
  }
}

/** Wie ssh-copy-id: Verbindung wählen, mit ihrer bisherigen Anmeldung verbinden und den Schlüssel eintragen. */
async function installKey(c: Cred) {
  const conns = (await api.list()).filter((x) => x.sshPort);
  if (!conns.length) return tell("Keine SSH-Verbindung", "In der Bibliothek gibt es keine Verbindung mit SSH.");
  const sel = document.createElement("select");
  sel.append(...conns.map((x) => new Option(`${x.name} (${x.user ? `${x.user}@` : ""}${x.address})`, x.id)));
  const host = byId("ask-text");
  host.after(sel);
  try {
    if (!(await ask("Schlüssel eintragen", `Sesam meldet sich mit der bisherigen Anmeldung der Verbindung an und ergänzt ~/.ssh/authorized_keys um den öffentlichen Schlüssel von «${c.name}». Danach kann die Verbindung diese Zugangsdaten nutzen.`, "Eintragen"))) return;
  } finally {
    sel.remove();
  }
  const id = sel.value;
  const name = conns.find((x) => x.id === id)?.name ?? "";
  let trust: string | null = null;
  for (;;) {
    try {
      return tell("Erledigt", await api.credInstallKey(c.id, id, trust));
    } catch (e) {
      if (trust === null && String(e).startsWith(HOSTKEY_UNKNOWN) && (trust = await askHostKey(name, String(e)))) continue;
      throw e;
    }
  }
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
