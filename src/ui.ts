import { HOSTKEY_UNKNOWN } from "./api";

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

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

/** Genau eine Ansicht zeigen: Bibliothek (`null`) oder eine Sitzungsansicht mit ihrer Werkzeugleiste. */
export function showView(view: "terminal" | "desktop" | "files" | null) {
  byId("library").hidden = view !== null;
  byId("session").hidden = view === null;
  for (const v of ["terminal", "desktop", "files"]) byId(v).hidden = view !== v;
  byId("desktop-tools").hidden = view !== "desktop";
  byId("files-tools").hidden = view !== "files";
}
