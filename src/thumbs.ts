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
  el.style.backgroundColor = os.color; // nicht `background`: das würde background-size: cover überschreiben
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
  const g = small.getContext("2d")!;
  g.drawImage(canvas, 0, 0, w, h);
  if (isBlank(g.getImageData(0, 0, w, h).data)) return; // schwarzes/leeres Bild (kein Bild empfangen) nicht als Vorschau merken
  const blob = await new Promise<Blob | null>((r) => small.toBlob(r, "image/jpeg", 0.75));
  if (!blob) return;
  await invoke("thumb_save", { id: c.id, data: Array.from(new Uint8Array(await blob.arrayBuffer())) }).catch(() => {});
  forgetThumb(c.id);
}

/** Fast einfarbig (z. B. schwarz, weil noch kein Bild kam)? Prüft jeden 97. Bildpunkt. */
export function isBlank(px: Uint8ClampedArray) {
  let lo = 255, hi = 0;
  for (let i = 0; i < px.length; i += 4 * 97) {
    const v = (px[i] + px[i + 1] + px[i + 2]) / 3;
    lo = Math.min(lo, v);
    hi = Math.max(hi, v);
  }
  return hi - lo < 12;
}

// Selbstprüfung im Entwicklungsbuild
if (import.meta.env?.DEV) {
  const flat = new Uint8ClampedArray(4 * 1000).fill(0);
  const pic = flat.map((_, i) => (i * 37) % 256);
  console.assert(isBlank(flat) && !isBlank(pic), "isBlank");
}
