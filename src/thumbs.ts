/** Vorschaubilder: letzter Desktop (beim Trennen gespeichert), eigenes Bild oder Symbol des Betriebssystems. */
import { invoke } from "@tauri-apps/api/core";
import type { Connection } from "./api";
import { ICON } from "./icons";

const OS: Record<string, { icon: string; color: string; label: string }> = {
  ubuntu: { icon: ICON.osLinux, color: "#8a3b5e", label: "Ubuntu" },
  linux: { icon: ICON.osLinux, color: "#3c4a57", label: "Linux" },
  macos: { icon: ICON.osMac, color: "#3d6a8c", label: "macOS" },
  windows: { icon: ICON.osWindows, color: "#245a8f", label: "Windows" },
  raspberry: { icon: ICON.osPi, color: "#7a2f3a", label: "Raspberry Pi" },
  "": { icon: ICON.osGeneric, color: "#4a5560", label: "Unbekannt" },
};
export const OS_CHOICES = Object.entries(OS).map(([value, x]) => ({ value, label: x.label }));

const cache = new Map<string, string | null>();

/** Füllt das Vorschau-Element (Hintergrundbild oder Symbol). */
export async function renderThumb(el: HTMLElement, c: Connection) {
  const os = OS[c.os ?? ""] ?? OS[""];
  el.style.background = os.color;
  const icon = document.createElement("span");
  icon.className = "thumb-os";
  icon.innerHTML = os.icon;
  el.prepend(icon);
  let url = cache.get(c.id);
  if (url === undefined) {
    const bytes = await invoke<number[] | null>("thumb_get", { id: c.id }).catch(() => null);
    url = bytes ? URL.createObjectURL(new Blob([new Uint8Array(bytes)])) : null;
    cache.set(c.id, url);
  }
  if (url) {
    el.style.backgroundImage = `url(${url})`;
    el.classList.add("has-image");
  }
}

export function forgetThumb(id: string) {
  const u = cache.get(id);
  if (u) URL.revokeObjectURL(u);
  cache.delete(id);
}

/** Letztes Bild eines Desktops verkleinert speichern (nicht bei eigenem Bild). */
export async function saveDesktopThumb(c: Connection, canvas: HTMLCanvasElement) {
  if (c.customThumb || !canvas.width || c.id.startsWith("quick-")) return;
  const w = 480;
  const h = Math.round((canvas.height / canvas.width) * w);
  const small = Object.assign(document.createElement("canvas"), { width: w, height: h });
  small.getContext("2d")!.drawImage(canvas, 0, 0, w, h);
  const blob = await new Promise<Blob | null>((r) => small.toBlob(r, "image/jpeg", 0.75));
  if (!blob) return;
  await invoke("thumb_save", { id: c.id, data: Array.from(new Uint8Array(await blob.arrayBuffer())) }).catch(() => {});
  forgetThumb(c.id);
}
