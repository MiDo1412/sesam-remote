import { availableMonitors, getCurrentWindow, PhysicalPosition, PhysicalSize, primaryMonitor } from "@tauri-apps/api/window";
import { api, CERT_CHANGED, CERT_UNKNOWN, desktopProtocol, type Connection } from "./api";
import { choose, showView } from "./ui";
import { keysym } from "./keysym";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

type Mode = "fit" | "scale" | "native";
type Key = Pick<KeyboardEvent, "code" | "key">;
let close: (() => void) | null = null;

/** Desktop über VNC oder RDP; Anzeige, Modi und Eingabe-Logik sind gemeinsam. */
export async function openDesktop(c: Connection, onBack: () => void) {
  close?.();
  showView("desktop");
  const rdp = desktopProtocol(c) === "rdp";
  $("session-title").textContent = c.name;
  $("session-sub").textContent = rdp ? `RDP · ${c.address}:${c.rdpPort}` : `VNC · ${c.address}:${c.vncPort}`;
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

  /** Gewünschte entfernte Auflösung: Fenstergrösse, bei Hochformat hochkant. */
  const wanted = () => {
    let w = wrap.clientWidth & ~1, h = wrap.clientHeight & ~1;
    if (c.orientation === "portrait" && w > h) [w, h] = [h, w];
    return [w, h];
  };

  /** «An Fenster anpassen»: entfernte Auflösung folgt der Fenstergrösse. */
  let sizeTimer = 0;
  const requestSize = () => {
    clearTimeout(sizeTimer);
    if (mode !== "fit" || !sid || !resizable) return;
    sizeTimer = window.setTimeout(async () => {
      const [w, h] = wanted();
      if (w < 200 || h < 200 || (w === canvas.width && h === canvas.height)) return;
      if (rdp) return void api.rdpSetSize(sid!, w, h).catch(() => {}); // Absage kommt als Hinweis
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

  // ---------- Maus ----------
  const toRemote = (e: MouseEvent) => {
    const b = canvas.getBoundingClientRect();
    const x = Math.round(((e.clientX - b.left) * canvas.width) / b.width);
    const y = Math.round(((e.clientY - b.top) * canvas.height) / b.height);
    return [Math.max(0, Math.min(canvas.width - 1, x)), Math.max(0, Math.min(canvas.height - 1, y))];
  };
  // Browser: 1 links, 2 rechts, 4 Mitte  →  RFB: 1 links, 2 Mitte, 4 rechts (RDP nimmt die Browser-Maske)
  const vncMask = (b: number) => (b & 1) | ((b & 4) >> 1) | ((b & 2) << 1);
  const pointer = (x: number, y: number, buttons: number) => {
    if (!sid) return;
    (rdp ? api.rdpPointer(sid, x, y, buttons) : api.vncPointer(sid, vncMask(buttons), x, y)).catch(() => {});
  };
  let pending: [number, number, number] | null = null;
  const flushPointer = () => {
    if (pending) pointer(...pending);
    pending = null;
  };
  const onPointer = (e: PointerEvent) => {
    e.preventDefault();
    const [x, y] = toRemote(e);
    // Bewegungen pro Bildschirmbild bündeln, Klicks sofort senden.
    const first = pending === null;
    pending = [x, y, e.buttons];
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
      const vertical = Math.abs(e.deltaY) >= Math.abs(e.deltaX);
      const delta = vertical ? e.deltaY : e.deltaX;
      if (rdp) return void api.rdpWheel(sid, vertical, delta < 0 ? 120 : -120).catch(() => {});
      const [x, y] = toRemote(e);
      const m = vncMask(e.buttons);
      const bit = vertical ? (delta < 0 ? 8 : 16) : delta < 0 ? 32 : 64;
      api.vncPointer(sid, m | bit, x, y).then(() => api.vncPointer(sid!, m, x, y)).catch(() => {});
    },
    { passive: false },
  );

  // ---------- Tastatur ----------
  /** Gedrückte Tasten (code → Keysym bei VNC), damit Loslassen und «alles loslassen» stimmen. */
  const down = new Map<string, number>();
  let altGr = false;
  const send = (isDown: boolean, k: Key) => {
    if (!sid) return;
    if (rdp) {
      if (isDown) down.set(k.code, 0);
      else down.delete(k.code);
      return void api.rdpKey(sid, k.code, k.key, isDown, altGr).catch(() => {});
    }
    const sym = isDown ? keysym(k) : down.get(k.code);
    if (sym === undefined) return;
    if (isDown) down.set(k.code, sym);
    else down.delete(k.code);
    api.vncKey(sid, isDown, sym).catch(() => {});
  };
  // Windows meldet AltGr als «Strg links» + «Alt rechts» kurz nacheinander. Strg darum kurz
  // zurückhalten und verwerfen, wenn AltGr folgt. AltGr selbst senden wir nicht: das erzeugte
  // Zeichen (z. B. «@») kommt als eigener Keysym (VNC) bzw. Unicode-Ereignis (RDP).
  const isWindows = navigator.userAgent.includes("Windows");
  let heldCtrl: { key: Key; timer: number } | null = null;
  let swallowedCtrl = false;
  const flushCtrl = () => {
    if (!heldCtrl) return;
    clearTimeout(heldCtrl.timer);
    send(true, heldCtrl.key);
    heldCtrl = null;
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "F11") return; // lokal: Vollbild
    e.preventDefault();
    if (!sid) return;
    const isDown = e.type === "keydown";
    if (e.key === "AltGraph") {
      altGr = isDown;
      if (isDown && heldCtrl) {
        clearTimeout(heldCtrl.timer);
        heldCtrl = null;
        swallowedCtrl = true;
      }
      return;
    }
    if (isWindows && e.code === "ControlLeft" && !down.has("ControlLeft")) {
      if (isDown) {
        if (!heldCtrl && !swallowedCtrl) heldCtrl = { key: { code: e.code, key: e.key }, timer: window.setTimeout(flushCtrl, 30) };
        return;
      }
      if (swallowedCtrl) {
        swallowedCtrl = false;
        return;
      }
      flushCtrl();
    }
    if (isDown) flushCtrl();
    send(isDown, e);
  };
  const releaseAll = () => {
    if (heldCtrl) clearTimeout(heldCtrl.timer);
    heldCtrl = null;
    swallowedCtrl = false;
    altGr = false;
    if (sid && rdp) api.rdpReleaseAll(sid).catch(() => {});
    else for (const sym of down.values()) if (sid) api.vncKey(sid, false, sym).catch(() => {});
    down.clear();
  };

  // Tastenkombinationen, die das lokale System selbst abfängt (physische Tasten).
  const combos: Record<string, string[]> = {
    cad: ["ControlLeft", "AltLeft", "Delete"],
    cse: ["ControlLeft", "ShiftLeft", "Escape"],
    alttab: ["AltLeft", "Tab"],
    altf4: ["AltLeft", "F4"],
    super: ["MetaLeft"],
    print: ["PrintScreen"],
  };
  // Keysyms derselben Tasten für VNC (keysym() braucht key, nicht code).
  const comboKey: Record<string, string> = {
    ControlLeft: "Control", AltLeft: "Alt", ShiftLeft: "Shift", MetaLeft: "Meta",
    Delete: "Delete", Escape: "Escape", Tab: "Tab", F4: "F4", PrintScreen: "PrintScreen",
  };
  const sendKeys = $<HTMLSelectElement>("send-keys");
  sendKeys.value = "";
  sendKeys.onchange = () => {
    const codes = combos[sendKeys.value] ?? [];
    for (const code of codes) send(true, { code, key: comboKey[code] });
    for (const code of [...codes].reverse()) send(false, { code, key: comboKey[code] });
    sendKeys.value = "";
    canvas.focus();
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
  // «Alle Monitore» (RDP): rahmenloses Fenster über alle Monitore, Server bekommt deren Anordnung.
  let spanned: { pos: PhysicalPosition; size: PhysicalSize; mode: Mode } | null = null;
  const toggleSpan = async () => {
    if (spanned) {
      const prev = spanned;
      spanned = null;
      await win.setAlwaysOnTop(false);
      await win.setDecorations(true);
      await win.setSize(prev.size);
      await win.setPosition(prev.pos);
      $("session").classList.remove("fullscreen");
      mode = prev.mode;
      layout();
      requestSize();
      return;
    }
    if (!sid || !rdp) return;
    const mons = await availableMonitors();
    const prim = (await primaryMonitor()) ?? mons[0];
    if (mons.length < 2) return void (status.textContent = "Es ist nur ein Monitor angeschlossen.");
    spanned = { pos: await win.outerPosition(), size: await win.innerSize(), mode };
    const x0 = Math.min(...mons.map((m) => m.position.x)), y0 = Math.min(...mons.map((m) => m.position.y));
    const x1 = Math.max(...mons.map((m) => m.position.x + m.size.width)), y1 = Math.max(...mons.map((m) => m.position.y + m.size.height));
    await win.setFullscreen(false);
    await win.setDecorations(false);
    await win.setPosition(new PhysicalPosition(x0, y0));
    await win.setSize(new PhysicalSize(x1 - x0, y1 - y0));
    await win.setAlwaysOnTop(true);
    $("session").classList.add("fullscreen");
    mode = "scale"; // entfernte Fläche = Vereinigung der Monitore, also deckungsgleich
    layout();
    api.rdpSetMonitors(sid, mons.map((m) => ({
      left: m.position.x,
      top: m.position.y,
      width: m.size.width,
      height: m.size.height,
      primary: m.name === prim.name && m.position.x === prim.position.x && m.position.y === prim.position.y,
    }))).catch((e) => (status.textContent = String(e)));
    status.textContent = "F11 beendet «Alle Monitore».";
  };
  $("span").hidden = !rdp;
  $("span").onclick = toggleSpan;
  const onF11 = (e: KeyboardEvent) => {
    if (e.key === "F11") {
      e.preventDefault();
      if (spanned) toggleSpan();
      else toggleFullscreen();
    }
  };
  window.addEventListener("keydown", onF11);
  $("fullscreen").onclick = toggleFullscreen;
  // RDP gleicht die Zwischenablage selbst ab (cliprdr); nur VNC braucht den Anstoss beim Fokussieren.
  const syncClipboard = () => sid && !rdp && c.sharedClipboard && api.vncClipboardSync(sid).catch(() => {});
  window.addEventListener("focus", syncClipboard);
  canvas.addEventListener("focus", syncClipboard);

  close = () => {
    ro.disconnect();
    releaseAll();
    window.removeEventListener("keydown", onF11);
    window.removeEventListener("focus", syncClipboard);
    if (sid) (rdp ? api.rdpClose(sid) : api.vncClose(sid)).catch(() => {});
    sid = null;
    if (spanned) toggleSpan();
    win.setFullscreen(false);
    $("session").classList.remove("fullscreen");
    close = null;
  };
  $("session-back").onclick = () => {
    close?.();
    showView(null);
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
          // 0×0: unsichtbar; mit hx = 1 der Standardzeiger (RDP).
          canvas.style.cursor = hx === 1 ? "default" : "none";
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

  let remember = true;
  const openSession = (trust: string | null) => {
    if (!rdp) return api.vncOpen(c.id, trust, remember, onFrame);
    const [w, h] = mode === "fit" ? wanted() : [1920, 1080];
    return api.rdpOpen(c.id, trust, remember, Math.max(w, 640), Math.max(h, 480), onFrame);
  };

  let trust: string | null = null;
  for (;;) {
    try {
      sid = await openSession(trust);
      status.textContent = "";
      canvas.focus();
      syncClipboard();
      requestSize();
    } catch (e) {
      const msg = String(e);
      const changed = msg.startsWith(CERT_CHANGED);
      if (trust === null && (changed || msg.startsWith(CERT_UNKNOWN))) {
        const fp = msg.slice((changed ? CERT_CHANGED : CERT_UNKNOWN).length);
        const port = rdp ? c.rdpPort : c.vncPort;
        const choice = await choose(
          changed ? "Zertifikat hat sich geändert" : "Unbekanntes Zertifikat",
          changed
            ? `Das Zertifikat von ${c.address}:${port} ist nicht mehr dasselbe wie beim letzten Mal. Das kann ein Angriff sein. Nur fortfahren, wenn das Zertifikat auf dem Server erneuert wurde.`
            : `${c.address}:${port} nutzt ein eigenes Zertifikat. Vergleiche den Fingerabdruck mit dem Server (z. B. Übersicht des Setup-Scripts oder grdctl), bevor du ihm vertraust.`,
          `SHA-256\n${fp.replace(/^SHA256:/, "").toLowerCase()}`,
          changed ? "Neuem Zertifikat vertrauen" : "Vertrauen und verbinden",
          "Nur dieses Mal verbinden",
        );
        if (choice) {
          trust = fp;
          remember = choice === "ok";
          continue;
        }
        status.textContent = "Verbindung abgebrochen: Zertifikat nicht bestätigt.";
      } else status.textContent = `Keine Verbindung: ${msg}`;
    }
    break;
  }
}
