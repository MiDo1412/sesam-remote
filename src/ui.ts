import { HOSTKEY_UNKNOWN } from "./api";

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** Rückfrage mit drittem Knopf und Festbreiten-Detail; Ergebnis "ok", "alt" oder null. */
export function choose(title: string, text: string, detail: string, ok: string, alt: string, cancel = "Abbrechen"): Promise<"ok" | "alt" | null> {
  const d = byId("ask-detail");
  const a = byId("ask-alt");
  d.hidden = false;
  d.textContent = detail;
  a.hidden = false;
  a.textContent = alt;
  const dlg = byId<HTMLDialogElement>("ask");
  const done = ask(title, text, ok, cancel).then(() => (dlg.returnValue === "ok" ? "ok" : dlg.returnValue === "alt" ? "alt" : null));
  return done.finally(() => {
    d.hidden = true;
    a.hidden = true;
  });
}

/** Wie `choose`, mit Kästchen (z. B. «Für alle übernehmen»); `all` = angekreuzt. */
export async function chooseAll(title: string, text: string, detail: string, ok: string, alt: string, cancel: string, check: string) {
  const box = byId("ask-check");
  const input = box.querySelector("input")!;
  box.hidden = false;
  input.checked = false;
  box.querySelector("span")!.textContent = check;
  const v = await choose(title, text, detail, ok, alt, cancel);
  box.hidden = true;
  return { v, all: input.checked };
}

/** Eigener Dialog statt alert/confirm: die nativen fehlen in WKWebView (macOS/iPad). */
export function ask(title: string, text: string, ok = "OK", cancel: string | null = "Abbrechen"): Promise<boolean> {
  const dlg = byId<HTMLDialogElement>("ask");
  byId("ask-title").textContent = title;
  byId("ask-text").textContent = text;
  byId("ask-ok").textContent = ok;
  const c = byId("ask-cancel");
  c.hidden = cancel === null;
  c.textContent = cancel ?? "";
  dlg.returnValue = "";
  dlg.showModal();
  const input = byId<HTMLInputElement>("ask-input");
  if (!input.hidden) input.focus();
  return new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue === "ok"), { once: true }));
}

export const tell = (title: string, text: string) => ask(title, text, "OK", null).then(() => {});

/** Rückfrage mit Eingabefeld; `null` bei Abbruch. */
export async function askText(title: string, text: string, ok: string, value = ""): Promise<string | null> {
  const input = byId<HTMLInputElement>("ask-input");
  input.hidden = false;
  input.value = value;
  // Enter bestätigt (der erste Knopf im Formular wäre sonst «Abbrechen»).
  input.onkeydown = (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      byId<HTMLDialogElement>("ask").close("ok");
    }
  };
  const confirmed = await ask(title, text, ok);
  input.hidden = true;
  return confirmed ? input.value.trim() : null;
}

/** Passwort abfragen (verdeckt); wird nirgends gespeichert. */
export async function askPassword(title: string, text: string, ok: string): Promise<string | null> {
  const input = byId<HTMLInputElement>("ask-input");
  input.type = "password";
  input.hidden = false;
  input.value = "";
  input.onkeydown = (e) => e.key === "Enter" && (e.preventDefault(), byId<HTMLDialogElement>("ask").close("ok"));
  try {
    return (await ask(title, text, ok)) && input.value ? input.value : null; // nicht kürzen: Leerzeichen gehören zum Passwort
  } finally {
    input.hidden = true;
    input.type = "text";
    input.value = "";
  }
}

/** Fingerabdruck eines unbekannten SSH-Servers bestätigen lassen. `msg` beginnt mit HOSTKEY_UNKNOWN. */
export async function askHostKey(address: string, msg: string): Promise<string | null> {
  const [alg, fp] = msg.slice(HOSTKEY_UNKNOWN.length).split("|");
  const ok = await ask(
    "Unbekannter Server",
    `${address} ist noch nicht bekannt. Vergleiche den Fingerabdruck mit dem Server (ssh-keygen -lf /etc/ssh/ssh_host_*_key.pub), bevor du ihm vertraust.\n\n${alg}\n${fp}`,
    "Vertrauen und verbinden",
  );
  return ok ? fp : null;
}
