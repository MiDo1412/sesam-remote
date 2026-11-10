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

/** Hinweis mit Überschrift und Schritten (statt einer langen Zeile), z. B. für eine sofort abgelehnte Verbindung. */
export interface Hint { title: string; text: string; steps: string[]; help?: string }

/** Server bricht schon die Aushandlung ab (Windows 10054/10053, Unix ECONNRESET): er lehnt ab, bevor ein Passwort geprüft wird. */
export function rejectedHint(msg: string, c: Pick<Connection, "rdpTarget" | "rdpSessionUser" | "os">): Hint | null {
  if (!/os error (10054|10053|104)\b|connection reset|vom Remotehost geschlossen/i.test(msg)) return null;
  const gnome = c.rdpTarget === "session" || c.os === "ubuntu" || c.os === "linux" || c.os === "raspberry";
  if (gnome)
    return {
      title: "Der Rechner hat die Verbindung abgelehnt",
      text: "Die Bildschirmfreigabe dieses Benutzers hat keine Zugangsdaten, oder sie kann sie nicht lesen.",
      steps: [
        "Auf dem Rechner als dieser Benutzer anmelden.",
        "Einstellungen › System › Remote-Desktop › Desktopfreigabe: Benutzername und Passwort setzen.",
        "Meldet sich der Benutzer automatisch an: den Schlüsselbund ohne Passwort speichern, sonst kann GNOME die Zugangsdaten nach einem Neustart nicht lesen.",
      ],
      help: "rdp-gnome",
    };
  return {
    title: "Der Rechner hat die Verbindung abgelehnt",
    text: "Er hat abgebrochen, bevor ein Passwort geprüft wurde.",
    steps: c.os === "windows"
      ? ["Auf dem Rechner Remotedesktop einschalten (Einstellungen › System › Remotedesktop).", "Prüfen, ob eine Firewall den Port sperrt.", "Ist Netzwerkebenen-Authentifizierung vorgeschrieben, Benutzer und Passwort in Sesam eintragen."]
      : ["Prüfen, ob der Remote-Desktop-Dienst auf dem Rechner eingerichtet ist und Zugangsdaten hat.", "Prüfen, ob eine Firewall den Port sperrt."],
    help: c.os === "windows" ? "rdp-windows" : undefined,
  };
}

/** Adresse im eigenen Netz: private oder link-lokale IP, `.local` oder ein Name ohne Punkt (z. B. «nas»). */
export function isLocalAddress(host: string) {
  const h = host.trim().toLowerCase().replace(/^\[|\]$/g, "");
  const m = /^(\d+)\.(\d+)\.\d+\.\d+$/.exec(h);
  if (m) {
    const [a, b] = [+m[1], +m[2]];
    return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 169 && b === 254);
  }
  if (h.includes(":")) return /^(fe[89ab]|f[cd])/.test(h);
  return h.endsWith(".local") || !h.includes(".");
}

/** macOS sperrt Apps ohne Freigabe «Lokales Netzwerk» mit EHOSTUNREACH (os error 65), obwohl der Rechner erreichbar ist. */
export function localNetworkHint(msg: string, host: string, mac = navigator.userAgent.includes("Mac")): Hint | null {
  if (!mac || !/os error 65\b/.test(msg) || !isLocalAddress(host)) return null;
  return {
    title: "macOS blockiert den Zugriff aufs lokale Netzwerk",
    text: `${host} ist im eigenen Netz; macOS lässt Sesam Remote nur mit Freigabe dorthin verbinden.`,
    steps: [
      "Systemeinstellungen › Datenschutz & Sicherheit › Lokales Netzwerk öffnen.",
      "«Sesam Remote» einschalten.",
      "Erneut verbinden.",
    ],
  };
}

/** VNC-Server, der nur RealVNC RA2/RA2ne anbietet (nicht offengelegt): die zwei Auswege nennen. */
export function vncAuthHint(msg: string): Hint | null {
  if (!msg.includes("VNC_RA2:")) return null;
  return {
    title: "Anmeldeverfahren von RealVNC wird nicht unterstützt",
    text: "Der Server meldet sich nur mit RA2 an, einem nicht offengelegten Verfahren von RealVNC. Sesam kann VNC-Passwort, VeNCrypt mit Zertifikat und die Anmeldung von macOS.",
    steps: [
      "In RealVNC Server: Optionen › Sicherheit › Authentifizierung auf «VNC-Passwort» stellen und dieses Passwort in Sesam hinterlegen.",
      "Oder auf aktuellem Raspberry Pi OS den Dienst wayvnc verwenden (raspi-config › Interface Options › VNC); er meldet sich mit VeNCrypt an.",
    ],
    help: "vnc",
  };
}

/** Derselbe Hinweis als eine Zeile für Statuszeilen (Shell, Dateien). */
export function localNetworkText(msg: string, host: string) {
  const h = localNetworkHint(msg, host);
  return h && `${h.title}. ${h.steps.join(" ")}`;
}

/** «Anmeldung abgelehnt» je nach Ziel klar benennen. */
export function explainRdpError(msg: string, c: Pick<Connection, "rdpTarget" | "rdpSessionUser" | "os" | "address">) {
  const h = localNetworkHint(msg, c.address) ?? rejectedHint(msg, c);
  if (h) return `${h.title}. ${h.text}`;
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

/** Selbstprüfung: lokales Netz erkennen und den macOS-Hinweis nur bei os error 65 zeigen. */
if (import.meta.env.DEV) {
  for (const h of ["172.16.1.31", "10.0.0.5", "192.168.1.153", "169.254.3.4", "nas", "doerig-02", "mac.local", "[fe80::1]", "fd00::2"]) console.assert(isLocalAddress(h), "lokal", h);
  for (const h of ["172.32.0.1", "8.8.8.8", "example.com", "2a02::1"]) console.assert(!isLocalAddress(h), "nicht lokal", h);
  const err = "No route to host (os error 65)";
  console.assert(localNetworkHint(err, "172.16.1.31", true)?.steps[0].includes("Lokales Netzwerk"), "Hinweis");
  console.assert(localNetworkHint(err, "example.com", true) === null && localNetworkHint(err, "10.0.0.1", false) === null, "kein Hinweis");
  console.assert(localNetworkHint("Connection refused (os error 61)", "10.0.0.1", true) === null, "anderer Fehler");
}
