import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, CERT_CHANGED, CERT_UNKNOWN, type Connection } from "./api";
import { ask } from "./ui";
import { keysym } from "./keysym";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

type Mode = "fit" | "scale" | "native";
let close: (() => void) | null = null;

export async function openDesktop(c: Connection, onBack: () => void) {
  close?.();
  $("library").hidden = true;
  $("session").hidden = false;
  $("terminal").hidden = true;
  $("desktop").hidden = false;
  $("desktop-tools").hidden = false;
  $("session-title").textContent = c.name;
  $("session-sub").textContent = `VNC · ${c.address}:${c.vncPort}`;
  const status = $("session-status");
  status.textContent = "Verbinde …";

  const wrap = $("desktop");
  // Frisches Canvas pro Sitzung, damit keine alten Ereignis-Handler übrig bleiben.
  const canvas = document.createElement("canvas");
  canvas.tabIndex = 0;
  canvas.setAttribute("aria-label", "Entfernter Bildschirm");
  wrap.replaceChildren(canvas);
  const ctx = canvas.getContext("2d", { alpha: false })!;
  let mode: Mode = c.fitToWindow ? "fit" : "scale";
  let sid: string | null = null;
  let resizable = true;

  /** Anzeigegrösse des Canvas für Skalieren/Anpassen, Seitenverhältnis bleibt. */
  const layout = () => {
    wrap.classList.toggle("native", mode === "native");
    if (mode === "native" || !canvas.width) {
      canvas.style.width = canvas.style.height = "";
      return;
    }
    const s = Math.min(wrap.clientWidth / canvas.width, wrap.clientHeight / canvas.height);
    canvas.style.width = `${Math.floor(canvas.width * s)}px`;
    canvas.style.height = `${Math.floor(canvas.height * s)}px`;
  };

  /** «An Fenster anpassen»: entfernte Auflösung = Fenstergrösse (bei Hochformat hochkant). */
  let sizeTimer = 0;
  const requestSize = () => {
    clearTimeout(sizeTimer);
    if (mode !== "fit" || !sid || !resizable) return;
    sizeTimer = window.setTimeout(async () => {
      let w = wrap.clientWidth & ~1, h = wrap.clientHeight & ~1;
      if (c.orientation === "portrait" && w > h) [w, h] = [h, w];
      if (w < 64 || h < 64 || (w === canvas.width && h === canvas.height)) return;
      resizable = await api.vncSetSize(sid!, w, h).catch(() => false);
      if (!resizable) status.textContent = "Der Server kann seine Auflösung nicht anpassen; die Anzeige wird skaliert.";
    }, 300);
  };

  const ro = new ResizeObserver(() => (layout(), requestSize()));
  ro.observe(wrap);

  for (const r of document.querySelectorAll<HTMLInputElement>("[name=viewMode]")) {
    r.checked = r.value === mode;
    r.onchange = () => {
      mode = r.value as Mode;
      layout();
      requestSize();
      canvas.focus();
    };
  }

  // ---------- Eingaben ----------
  const toRemote = (e: MouseEvent) => {
    const b = canvas.getBoundingClientRect();
    const x = Math.round(((e.clientX - b.left) * canvas.width) / b.width);
    const y = Math.round(((e.clientY - b.top) * canvas.height) / b.height);
    return [Math.max(0, Math.min(canvas.width - 1, x)), Math.max(0, Math.min(canvas.height - 1, y))];
  };
  // Browser: 1 links, 2 rechts, 4 Mitte  →  RFB: 1 links, 2 Mitte, 4 rechts
  const mask = (b: number) => (b & 1) | ((b & 4) >> 1) | ((b & 2) << 1);
  let pending: [number, number, number] | null = null;
  const flushPointer = () => {
    if (pending && sid) api.vncPointer(sid, ...pending).catch(() => {});
    pending = null;
  };
  const onPointer = (e: PointerEvent) => {
    e.preventDefault();
    const [x, y] = toRemote(e);
    const m = mask(e.buttons);
    // Bewegungen pro Bildschirmbild bündeln, Klicks sofort senden.
    const first = pending === null;
    pending = [m, x, y];
    if (e.type !== "pointermove") flushPointer();
    else if (first) requestAnimationFrame(flushPointer);
  };
  canvas.addEventListener("pointerdown", (e) => (canvas.focus(), canvas.setPointerCapture(e.pointerId), onPointer(e)));
  canvas.addEventListener("pointerup", onPointer);
  canvas.addEventListener("pointermove", onPointer);
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      if (!sid) return;
      const [x, y] = toRemote(e);
      const m = mask(e.buttons);
      const bit = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? (e.deltaY < 0 ? 8 : 16) : e.deltaX < 0 ? 32 : 64;
      api.vncPointer(sid, m | bit, x, y).then(() => api.vncPointer(sid!, m, x, y)).catch(() => {});
    },
    { passive: false },
  );

  const down = new Map<string, number>();
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "F11") return; // lokal: Vollbild
    e.preventDefault();
    if (!sid) return;
    const isDown = e.type === "keydown";
    const sym = isDown ? keysym(e) : down.get(e.code);
    if (sym === undefined) return;
    if (isDown) down.set(e.code, sym);
    else down.delete(e.code);
    api.vncKey(sid, isDown, sym).catch(() => {});
  };
  const releaseAll = () => {
    for (const sym of down.values()) if (sid) api.vncKey(sid, false, sym).catch(() => {});
    down.clear();
  };
  canvas.addEventListener("keydown", onKey);
  canvas.addEventListener("keyup", onKey);
  canvas.addEventListener("blur", releaseAll);

  // ---------- Vollbild und Zwischenablage ----------
  const win = getCurrentWindow();
  const toggleFullscreen = async () => {
    const on = !(await win.isFullscreen());
    await win.setFullscreen(on);
    $("session").classList.toggle("fullscreen", on);
    if (on) status.textContent = "F11 beendet das Vollbild.";
  };
  const onF11 = (e: KeyboardEvent) => {
    if (e.key === "F11") {
      e.preventDefault();
      toggleFullscreen();
    }
  };
  window.addEventListener("keydown", onF11);
  $("fullscreen").onclick = toggleFullscreen;
  const syncClipboard = () => sid && c.sharedClipboard && api.vncClipboardSync(sid).catch(() => {});
  window.addEventListener("focus", syncClipboard);
  canvas.addEventListener("focus", syncClipboard);

  close = () => {
    ro.disconnect();
    releaseAll();
    window.removeEventListener("keydown", onF11);
    window.removeEventListener("focus", syncClipboard);
    if (sid) api.vncClose(sid);
    sid = null;
    win.setFullscreen(false);
    $("session").classList.remove("fullscreen");
    close = null;
  };
  $("session-back").onclick = () => {
    close?.();
    $("session").hidden = true;
    $("desktop").hidden = true;
    $("desktop-tools").hidden = true;
    $("terminal").hidden = false;
    $("library").hidden = false;
    onBack();
  };

  // ---------- Bild vom Server ----------
  const onFrame = (buf: ArrayBuffer) => {
    const v = new DataView(buf);
    const text = () => new TextDecoder().decode(new Uint8Array(buf, 1));
    switch (v.getUint8(0)) {
      case 0: {
        const [x, y, w, h] = [v.getUint16(1), v.getUint16(3), v.getUint16(5), v.getUint16(7)];
        ctx.putImageData(new ImageData(new Uint8ClampedArray(buf, 9, w * h * 4), w, h), x, y);
        break;
      }
      case 1:
        canvas.width = v.getUint16(1);
        canvas.height = v.getUint16(3);
        layout();
        break;
      case 3:
        status.textContent = `Verbindung beendet: ${text()}`;
        sid = null;
        break;
      case 5:
        status.textContent = text();
        break;
      case 6: {
        const [hx, hy, w, h] = [v.getUint16(1), v.getUint16(3), v.getUint16(5), v.getUint16(7)];
        if (!w || !h) {
          canvas.style.cursor = "none";
          break;
        }
        const c2 = document.createElement("canvas");
        [c2.width, c2.height] = [w, h];
        c2.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(buf, 9, w * h * 4), w, h), 0, 0);
        canvas.style.cursor = `url(${c2.toDataURL()}) ${hx} ${hy}, default`;
        break;
      }
    }
  };

  let trust: string | null = null;
  for (;;) {
    try {
      sid = await api.vncOpen(c.id, trust, onFrame);
      status.textContent = "";
      canvas.focus();
      syncClipboard();
      requestSize();
    } catch (e) {
      const msg = String(e);
      const changed = msg.startsWith(CERT_CHANGED);
      if (trust === null && (changed || msg.startsWith(CERT_UNKNOWN))) {
        const fp = msg.slice((changed ? CERT_CHANGED : CERT_UNKNOWN).length);
        const ok = await ask(
          changed ? "Zertifikat hat sich geändert" : "Unbekanntes Zertifikat",
          (changed
            ? `Das TLS-Zertifikat von ${c.address} ist nicht mehr dasselbe wie beim letzten Mal. Das kann ein Angriff sein. Nur fortfahren, wenn du das Zertifikat selbst erneuert hast.`
            : `${c.address} verschlüsselt mit einem eigenen Zertifikat. Vergleiche den Fingerabdruck mit dem Server, bevor du ihm vertraust.`) +
            `\n\n${fp}`,
          changed ? "Neuem Zertifikat vertrauen" : "Vertrauen und verbinden",
        );
        if (ok) {
          trust = fp;
          continue;
        }
        status.textContent = "Verbindung abgebrochen: Zertifikat nicht bestätigt.";
      } else status.textContent = `Keine Verbindung: ${msg}`;
    }
    break;
  }
}
