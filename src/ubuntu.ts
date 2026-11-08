/** Ubuntu/GNOME: Ports per SSH lesen, Ablehnungen klar zuordnen, Assistent «Ubuntu-PC hinzufügen». */
import { api, HOSTKEY_UNKNOWN, type Connection } from "./api";
import { askHostKey, tell } from "./ui";

/** Ports aus /etc/sesam-rdp/ports lesen; fragt bei unbekanntem SSH-Server nach. */
export async function fetchPorts(host: string, port: number, user: string, password?: string, id?: string): Promise<[string, number][]> {
  try {
    return await api.rdpPorts(host, port, user, password, id, null);
  } catch (e) {
    const msg = String(e);
    if (!msg.startsWith(HOSTKEY_UNKNOWN)) throw e;
    const trust = await askHostKey(host, msg);
    if (!trust) throw new Error("Abgebrochen: SSH-Server nicht bestätigt.");
    return api.rdpPorts(host, port, user, password, id, trust);
  }
}

/** «Anmeldung abgelehnt» je nach Ziel klar benennen. */
export function explainRdpError(msg: string, c: Pick<Connection, "rdpTarget" | "rdpSessionUser" | "os">) {
  // Server bricht schon die Aushandlung ab (Windows 10054/10053, Unix ECONNRESET): er lehnt ab, bevor ein Passwort geprüft wird.
  if (/os error (10054|10053|104)|connection reset|vom Remotehost geschlossen/i.test(msg)) {
    const linux = c.rdpTarget === "session" || c.os === "ubuntu" || c.os === "linux" || c.os === "raspberry";
    const why = linux
      ? "Bei der GNOME-Bildschirmfreigabe heisst das meist: Für diesen Benutzer sind keine Zugangsdaten gesetzt, oder sein Schlüsselbund ist gesperrt, weil er sich automatisch anmeldet (dann kann GNOME die Zugangsdaten nicht lesen). Abhilfe: Zugangsdaten unter Einstellungen › System › Remote-Desktop › Desktopfreigabe setzen und bei automatischer Anmeldung den Schlüsselbund ohne Passwort speichern (Setup-Script Schritt 4 oder «Passwörter und Schlüssel»)."
      : c.os === "windows"
        ? "Bei Windows oft: Remotedesktop ist aus, die Firewall blockiert, oder der Server verlangt eine andere Sicherheitsstufe."
        : "Häufige Gründe: Der Dienst ist nicht eingerichtet (keine Zugangsdaten), eine Firewall blockiert, oder der Server verlangt eine andere Sicherheitsstufe.";
    return `Der Server hat die Verbindung sofort beendet, noch vor der Anmeldung. ${why}`;
  }
  if (!msg.includes("Anmeldung abgelehnt")) return msg;
  if (c.rdpTarget === "login") return "Der Anmeldebildschirm hat die Zugangsdaten abgelehnt. Gefragt sind die Zugangsdaten der Systemfreigabe von GNOME Remote Desktop (Sesam-Setup-Script oder «sudo grdctl --system»), nicht unbedingt die des Ubuntu-Kontos.";
  if (c.rdpTarget === "session") return `Die Freigabe von ${c.rdpSessionUser || "diesem Benutzer"} hat die Zugangsdaten abgelehnt (Ubuntu: Einstellungen › System › Remote-Desktop › Desktopfreigabe).`;
  const win = "Bei Windows mit Microsoft-Konto: die E-Mail-Adresse und das Microsoft-Kennwort eingeben (nicht die Windows-Hello-PIN). Bei einem lokalen Konto: den Kontonamen, nicht den angezeigten vollen Namen.";
  const linux = "Bei GNOME-Remote-Desktop (Ubuntu) gelten die Zugangsdaten aus Einstellungen › System › Remote-Desktop, nicht unbedingt das Konto-Passwort; bei xrdp das Passwort des Linux-Kontos.";
  if (c.os === "windows") return `Benutzer oder Passwort falsch. ${win}`;
  if (c.os === "ubuntu" || c.os === "linux" || c.os === "raspberry") return `Benutzer oder Passwort falsch. ${linux}`;
  return "Benutzer oder Passwort falsch.";
}

/** Assistent: Adresse und SSH-Login, Sesam liest die Ports und schlägt Einträge vor. */
export function initUbuntu(blank: () => Connection, reload: () => void) {
  const dlg = document.createElement("dialog");
  dlg.setAttribute("aria-labelledby", "ubuntu-title");
  dlg.innerHTML = `
    <form class="sheet" method="dialog" style="width:min(560px,calc(100vw - 48px))">
      <h2 id="ubuntu-title">Ubuntu-PC hinzufügen</h2>
      <p class="note span">Sesam liest per SSH, welche Zugänge das Einrichtungs-Script auf dem PC angelegt hat, und legt passende Einträge an. Nur lesend, ohne sudo.</p>
      <label>Name<input name="name" placeholder="z. B. PC Büro" /></label>
      <label>Hostname oder IP-Adresse<input name="address" required class="mono" autocomplete="off" spellcheck="false" /></label>
      <label>Ubuntu-Benutzername<input name="user" required autocomplete="off" spellcheck="false" /></label>
      <label>Passwort<input name="password" type="password" autocomplete="off" /></label>
      <p class="note span">Tipp: Für RDP dieselben Angaben nehmen wie für das Ubuntu-Konto, dann reicht dieser eine Eintrag.</p>
      <div class="span" id="ubuntu-list"></div>
      <p class="error span" id="ubuntu-error" role="alert"></p>
      <div class="row actions span"><span></span><button type="button" id="ubuntu-cancel">Abbrechen</button><button type="submit" class="primary" id="ubuntu-go">Zugänge lesen</button></div>
    </form>`;
  document.body.append(dlg);
  const form = dlg.querySelector("form")!;
  const f = form.elements as unknown as Record<string, HTMLInputElement>;
  const list = dlg.querySelector<HTMLElement>("#ubuntu-list")!;
  const err = dlg.querySelector<HTMLElement>("#ubuntu-error")!;
  const go = dlg.querySelector<HTMLButtonElement>("#ubuntu-go")!;
  let found: { name: string; port: number; target: "login" | "session"; user: string }[] | null = null;
  dlg.querySelector<HTMLButtonElement>("#ubuntu-cancel")!.onclick = () => dlg.close();

  const showList = () => {
    list.replaceChildren(
      ...found!.map((e, i) => {
        const l = document.createElement("label");
        l.className = "option";
        l.innerHTML = `<span><b></b><small></small></span><input type="checkbox" checked data-i="${i}" />`;
        l.querySelector("b")!.textContent = e.name;
        l.querySelector("small")!.textContent = e.target === "login" ? `Port ${e.port} · als beliebiger Benutzer anmelden` : `Port ${e.port} · sehen und steuern, was bei ${e.user} läuft`;
        return l;
      }),
    );
    go.textContent = "Einträge anlegen";
  };

  form.onsubmit = async (ev) => {
    ev.preventDefault();
    err.textContent = "";
    const host = f.address.value.trim();
    const pc = f.name.value.trim() || host;
    if (!found) {
      go.disabled = true;
      try {
        const ports = await fetchPorts(host, 22, f.user.value.trim(), f.password.value || undefined);
        const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
        found = [
          { name: `${pc} – Anmeldebildschirm`, port: 3389, target: "login", user: "" },
          ...ports.map(([u, p]) => ({ name: `${cap(u)} – Sitzung übernehmen`, port: p, target: "session" as const, user: u })),
        ];
        showList();
      } catch (e) {
        err.textContent = `${e} Ohne SSH: Einträge von Hand anlegen (Neue Verbindung › Dienste › RDP › «Was möchtest du öffnen?»).`;
      } finally {
        go.disabled = false;
      }
      return;
    }
    const chosen = [...list.querySelectorAll<HTMLInputElement>("input[type=checkbox]:checked")].map((b) => found![Number(b.dataset.i)]);
    try {
      for (const e of chosen) {
        const c: Connection = {
          ...blank(),
          id: crypto.randomUUID(),
          name: e.name,
          group: pc,
          address: host,
          os: "ubuntu",
          user: f.user.value.trim(),
          sshPort: 22,
          rdpPort: e.port,
          vncPort: null,
          desktopProtocol: "rdp",
          defaultView: "desktop",
          rdpTarget: e.target,
          rdpSessionUser: e.user,
        };
        await api.save(c, { password: f.password.value || undefined });
      }
      dlg.close();
      reload();
      tell("Einträge angelegt", `${chosen.length} Einträge in der Gruppe «${pc}». Für RDP gelten Benutzer und Passwort des Ubuntu-Kontos; andere Angaben unter Bearbeiten › Dienste › RDP.`);
    } catch (e) {
      err.textContent = String(e);
    }
  };

  return {
    open() {
      form.reset();
      found = null;
      list.replaceChildren();
      err.textContent = "";
      go.textContent = "Zugänge lesen";
      dlg.showModal();
    },
  };
}
