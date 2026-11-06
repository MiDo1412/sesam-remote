/** Setup-Script für GNOME Remote Desktop: herunterladen oder per SSH auf dem Zielrechner starten. */
import { invoke } from "@tauri-apps/api/core";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { HOSTKEY_UNKNOWN, type Connection } from "./api";
import { openTab } from "./tabs";
import { ask, askHostKey, tell } from "./ui";

export async function downloadScript() {
  const [bytes, sha] = await invoke<[number[], string]>("setup_script");
  const path = await saveDialog({ title: "Setup-Script speichern", defaultPath: "sesam-rdp-setup.sh" });
  if (!path) return;
  await invoke("save_bytes", { path, data: bytes });
  tell("Script gespeichert", `${path}\n\nSHA-256 ${sha}\n\nAuf dem Zielrechner starten mit: sudo bash sesam-rdp-setup.sh`);
}

/** Script hochladen und in einem Shell-Tab starten; sudo-Passwort und alle Eingaben tippt der Benutzer selbst. */
export async function runOnTarget(c: Connection) {
  const [, sha] = await invoke<[number[], string]>("setup_script");
  const ok = await ask(
    "RDP auf dem Zielrechner einrichten?",
    `Sesam lädt das Setup-Script (Stand ${sha.slice(0, 12)}) per SSH nach ~/sesam-rdp-setup.sh auf ${c.address} und startet es in einer Shell mit sudo. Das Script fragt jeden Schritt nach; Passwörter tippst du selbst ein, Sesam speichert und protokolliert sie nicht.`,
    "Hochladen und starten",
  );
  if (!ok) return;
  let trust: string | null = null;
  for (;;) {
    try {
      await invoke<string>("setup_upload", { id: c.id, trust });
      break;
    } catch (e) {
      const msg = String(e);
      if (trust === null && msg.startsWith(HOSTKEY_UNKNOWN)) {
        trust = await askHostKey(c.address, msg);
        if (trust) continue;
        return;
      }
      return tell("Hochladen fehlgeschlagen", msg);
    }
  }
  openTab(c, "shell", true, { command: "sudo bash ~/sesam-rdp-setup.sh\n" });
}
