/** Touch-Bedienung des entfernten Desktops: Touchpad oder direkt tippen, Gesten, lokaler Zoom, Zeiger-Lupe, Bildschirmtastatur. */
import "./touch.css";
import { ICON } from "./icons";
import { openMenu } from "./menu";
import { toolButton, toolSep } from "./tabs";
import { createKeyboard, isTouch, pressVia, type Key } from "./osk";

type Mode = "touchpad" | "direct";

const TAP_MS = 300, LONG_MS = 500, SLOP = 8, SPEED = 1.6, WHEEL_STEP = 30, MAX_ZOOM = 6, LUPE = 132, LUPE_ZOOM = 2.5;
const ARROW = `<svg width="18" height="24" viewBox="0 0 18 24" aria-hidden="true"><path d="M1 1v19l5-5 4 8 3-1.5-4-8h7z" fill="#fff" stroke="#000" stroke-width="1.2" stroke-linejoin="round"/></svg>`;
const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/**
 * Hängt sich mit Capture-Listenern vor die Maus-Handler des Desktops: Touch-Zeiger werden hier
 * behandelt und gestoppt, Maus und Stift laufen unverändert weiter.
 */
export function desktopTouch(o: {
  pane: HTMLElement;
  wrap: HTMLElement;
  canvas: HTMLCanvasElement;
  /** Entfernte Koordinaten, Browser-Tastenmaske (1 links, 2 rechts). */
  pointer: (x: number, y: number, buttons: number) => void;
  send: (isDown: boolean, k: Key) => void;
  /** Gespeichert bei der Verbindung. */
  mode?: Mode;
}) {
  const { wrap, canvas } = o;
  wrap.classList.add("touch");
  let mode: Mode = o.mode ?? "touchpad";
  let lupeOn = false;
  let cx = 0, cy = 0, buttons = 0; // entfernter Zeiger
  let touched = false; // Overlay erst nach Touch, Maus blendet es wieder aus

  // ---------- Koordinaten (getBoundingClientRect enthält Zoom und Verschiebung) ----------
  const toClient = (x: number, y: number) => {
    const b = canvas.getBoundingClientRect();
    return [b.left + ((x + 0.5) * b.width) / canvas.width, b.top + ((y + 0.5) * b.height) / canvas.height];
  };
  const fromClient = (X: number, Y: number) => {
    const b = canvas.getBoundingClientRect();
    return [clamp(Math.floor(((X - b.left) * canvas.width) / b.width), 0, canvas.width - 1), clamp(Math.floor(((Y - b.top) * canvas.height) / b.height), 0, canvas.height - 1)];
  };
  let queued = false;
  const sendPos = (now = false) => {
    if (now) return o.pointer(cx, cy, buttons);
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => ((queued = false), o.pointer(cx, cy, buttons)));
  };
  const click = (mask: number) => {
    o.pointer(cx, cy, mask);
    o.pointer(cx, cy, 0);
  };
  const wheel = (deltaX: number, deltaY: number) => {
    const [clientX, clientY] = toClient(cx, cy);
    canvas.dispatchEvent(new WheelEvent("wheel", { deltaX, deltaY, clientX, clientY, cancelable: true }));
  };

  // ---------- Zoom ----------
  let s = 1, tx = 0, ty = 0;
  const applyZoom = () => {
    const w = canvas.offsetWidth, h = canvas.offsetHeight;
    if (s < 1.02) (s = 1), (tx = ty = 0);
    tx = clamp(tx, w - w * s, 0);
    ty = clamp(ty, h - h * s, 0);
    canvas.style.transform = s === 1 ? "" : `translate(${tx}px, ${ty}px) scale(${s})`;
    drawOverlay();
  };
  /** Im Touchpad-Modus folgt der Ausschnitt dem Zeiger. */
  const follow = () => {
    if (s === 1) return;
    const [X, Y] = toClient(cx, cy), r = wrap.getBoundingClientRect(), m = 24;
    tx += Math.max(0, r.left + m - X) - Math.max(0, X - (r.right - m));
    ty += Math.max(0, r.top + m - Y) - Math.max(0, Y - (r.bottom - m));
    applyZoom();
  };

  // ---------- Zeiger-Overlay und Lupe ----------
  const cursor = Object.assign(document.createElement("div"), { className: "touch-cursor", hidden: true });
  const lupe = Object.assign(document.createElement("canvas"), { className: "touch-lupe", hidden: true });
  lupe.width = lupe.height = LUPE * devicePixelRatio;
  lupe.setAttribute("aria-hidden", "true");
  wrap.append(cursor, lupe);
  let shape = "";
  const updateShape = () => {
    const css = canvas.style.cursor;
    if (css === shape) return;
    shape = css;
    const m = /url\("?(.*?)"?\)\s+(\d+)\s+(\d+)/.exec(css);
    if (m) {
      const img = Object.assign(new Image(), { src: m[1], alt: "" });
      img.style.translate = `-${m[2]}px -${m[3]}px`;
      cursor.replaceChildren(img);
    } else cursor.innerHTML = ARROW;
  };
  new MutationObserver(updateShape).observe(canvas, { attributes: true, attributeFilter: ["style"] });
  updateShape();
  let finger = false;
  const drawOverlay = () => {
    const [X, Y] = toClient(cx, cy), r = wrap.getBoundingClientRect();
    const left = X - r.left + wrap.scrollLeft, top = Y - r.top + wrap.scrollTop;
    // Zeiger nur, wenn der Server ihn nicht ausblendet («none» = er steckt im Bild).
    cursor.hidden = !touched || mode !== "touchpad" || shape === "none" || !canvas.width;
    if (!cursor.hidden) {
      cursor.style.transform = `translate(${left}px, ${top}px) scale(${shape.startsWith("url") ? canvas.getBoundingClientRect().width / canvas.width : 1})`;
      cursor.classList.toggle("pressed", buttons !== 0);
    }
    lupe.hidden = !(lupeOn && finger && canvas.width);
    if (lupe.hidden) return;
    const b = canvas.getBoundingClientRect(), src = (LUPE / LUPE_ZOOM) * (canvas.width / b.width);
    const g = lupe.getContext("2d")!, n = lupe.width;
    g.fillStyle = "#000";
    g.fillRect(0, 0, n, n);
    g.drawImage(canvas, cx - src / 2, cy - src / 2, src, src, 0, 0, n, n);
    g.strokeStyle = "#e5484d";
    g.lineWidth = devicePixelRatio;
    g.beginPath();
    g.moveTo(n / 2, n / 2 - 10 * devicePixelRatio), g.lineTo(n / 2, n / 2 + 10 * devicePixelRatio);
    g.moveTo(n / 2 - 10 * devicePixelRatio, n / 2), g.lineTo(n / 2 + 10 * devicePixelRatio, n / 2);
    g.stroke();
    const above = top - LUPE - 56;
    lupe.style.transform = `translate(${left - LUPE / 2}px, ${above >= wrap.scrollTop ? above : top + 56}px)`;
  };
  const moved = () => {
    sendPos();
    if (mode === "touchpad") follow();
    drawOverlay();
  };

  // ---------- Gesten ----------
  type One = { kind: "one"; t0: number; x0: number; y0: number; moved: boolean; drag: boolean; timer: number };
  type Two = { kind: "two"; t0: number; d0: number; m0: number[]; last: number[]; sub: "" | "pinch" | "scroll"; ax: number; ay: number; s0: number; tx0: number; ty0: number };
  let g: One | Two | { kind: "done" } | null = null;
  const pts = new Map<number, number[]>();
  let lastTap = { t: 0, X: 0, Y: 0, x: 0, y: 0 };
  const two = () => {
    const [a, b] = [...pts.values()];
    return { mid: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], d: Math.hypot(a[0] - b[0], a[1] - b[1]) };
  };
  const mine = (e: PointerEvent) => {
    if (e.pointerType !== "touch") {
      if (touched && e.type === "pointermove") (touched = false), drawOverlay();
      return false;
    }
    e.stopPropagation(); // Maus-Handler des Desktops nicht auslösen
    e.preventDefault();
    return true;
  };

  const onDown = (e: PointerEvent) => {
    if (!mine(e) || !canvas.width) return;
    touched = true;
    canvas.setPointerCapture(e.pointerId);
    if (kb.on) kb.focus();
    else canvas.focus({ preventScroll: true });
    pts.set(e.pointerId, [e.clientX, e.clientY]);
    const now = performance.now();
    if (pts.size === 1) {
      if (mode === "direct") {
        // Doppeltippen: zweites Tippen auf dieselbe Stelle, damit der Server einen Doppelklick erkennt.
        const near = now - lastTap.t < 400 && Math.hypot(e.clientX - lastTap.X, e.clientY - lastTap.Y) < 24;
        [cx, cy] = near ? [lastTap.x, lastTap.y] : fromClient(e.clientX, e.clientY);
        sendPos(true);
      }
      finger = true;
      const one: One = { kind: "one", t0: now, x0: e.clientX, y0: e.clientY, moved: false, drag: false, timer: 0 };
      // Langes Drücken: Taste halten, bis der Finger loslässt (Ziehen).
      one.timer = window.setTimeout(() => {
        if (g !== one || one.moved) return;
        one.drag = true;
        buttons = 1;
        sendPos(true);
        navigator.vibrate?.(10);
        drawOverlay();
      }, LONG_MS);
      g = one;
      drawOverlay();
    } else if (pts.size === 2 && g?.kind === "one" && !g.drag) {
      clearTimeout(g.timer);
      finger = false;
      const { mid, d } = two();
      g = { kind: "two", t0: now, d0: Math.max(d, 1), m0: mid, last: mid, sub: "", ax: 0, ay: 0, s0: s, tx0: tx, ty0: ty };
      drawOverlay();
    }
  };

  const onMove = (e: PointerEvent) => {
    const prev = pts.get(e.pointerId);
    if (!mine(e) || !prev || !g) return;
    pts.set(e.pointerId, [e.clientX, e.clientY]);
    if (g.kind === "one") {
      if (!g.moved && Math.hypot(e.clientX - g.x0, e.clientY - g.y0) > SLOP) {
        g.moved = true;
        if (!g.drag) clearTimeout(g.timer);
      }
      if (!g.moved) return;
      if (mode === "direct") [cx, cy] = fromClient(e.clientX, e.clientY);
      else {
        const b = canvas.getBoundingClientRect();
        cx = clamp(Math.round(cx + ((e.clientX - prev[0]) * SPEED * canvas.width) / b.width), 0, canvas.width - 1);
        cy = clamp(Math.round(cy + ((e.clientY - prev[1]) * SPEED * canvas.height) / b.height), 0, canvas.height - 1);
      }
      return moved();
    }
    if (g.kind !== "two" || pts.size < 2) return;
    const { mid, d } = two();
    if (!g.sub) {
      if (Math.abs(d - g.d0) > 24) g.sub = "pinch";
      else if (Math.hypot(mid[0] - g.m0[0], mid[1] - g.m0[1]) > 12) g.sub = "scroll";
    }
    if (g.sub === "pinch") {
      // Punkt unter der Anfangsmitte bleibt unter der aktuellen Mitte (Zoom und Verschieben in einem).
      const b = canvas.getBoundingClientRect(), ox = b.left - tx, oy = b.top - ty;
      const px = (g.m0[0] - ox - g.tx0) / g.s0, py = (g.m0[1] - oy - g.ty0) / g.s0;
      s = clamp((g.s0 * d) / g.d0, 1, MAX_ZOOM);
      tx = mid[0] - ox - px * s;
      ty = mid[1] - oy - py * s;
      applyZoom();
    } else if (g.sub === "scroll") {
      if (mode === "direct") [cx, cy] = fromClient(mid[0], mid[1]);
      g.ax += mid[0] - g.last[0];
      g.ay += mid[1] - g.last[1];
      // Natürliches Scrollen: Finger nach oben schiebt den Inhalt nach oben.
      for (; Math.abs(g.ay) >= WHEEL_STEP; g.ay -= Math.sign(g.ay) * WHEEL_STEP) wheel(0, -Math.sign(g.ay) * 100);
      for (; Math.abs(g.ax) >= WHEEL_STEP; g.ax -= Math.sign(g.ax) * WHEEL_STEP) wheel(-Math.sign(g.ax) * 100, 0);
    }
    g.last = mid;
  };

  const onUp = (e: PointerEvent) => {
    if (!mine(e) || !pts.has(e.pointerId)) return;
    const now = performance.now();
    if (g?.kind === "one") {
      clearTimeout(g.timer);
      if (g.drag) {
        buttons = 0;
        sendPos(true);
      } else if (!g.moved && now - g.t0 < TAP_MS && e.type === "pointerup") {
        click(1);
        const [X, Y] = toClient(cx, cy);
        lastTap = { t: now, X: mode === "direct" ? e.clientX : X, Y: mode === "direct" ? e.clientY : Y, x: cx, y: cy };
        shift();
      }
      g = { kind: "done" };
    } else if (g?.kind === "two") {
      if (!g.sub && now - g.t0 < TAP_MS && e.type === "pointerup") {
        if (mode === "direct") [cx, cy] = fromClient(...(two().mid as [number, number]));
        click(2);
      }
      g = { kind: "done" };
    }
    pts.delete(e.pointerId);
    if (!pts.size) g = null;
    finger = false;
    drawOverlay();
  };

  for (const t of ["pointerdown", "pointermove", "pointerup", "pointercancel"] as const)
    wrap.addEventListener(t, t === "pointerdown" ? onDown : t === "pointermove" ? onMove : onUp, true);

  // ---------- Bildschirmtastatur: Bild so verschieben, dass der Eingabepunkt sichtbar bleibt ----------
  let bottom: number | null = null, lift = 0;
  const shift = () => {
    const Y = toClient(cx, cy)[1] + lift;
    lift = bottom === null ? 0 : clamp(Y + 32 - bottom, 0, wrap.clientHeight * 0.8);
    wrap.style.translate = lift ? `0 ${-lift}px` : "";
  };
  const kb = createKeyboard({ pane: o.pane, press: pressVia(o.send), desktop: true, avoid: (b) => ((bottom = b), shift()) });

  // ---------- Werkzeugleiste ----------
  const modeLabel = { touchpad: "Touchpad", direct: "Direkt tippen" };
  /** Menü Touchpad / Direkt tippen / Zoom, an `anchor` (Werkzeugleiste oder Vollbild-Leiste). */
  const pickMode = async (anchor: HTMLElement) => {
    const v = await openMenu(
      anchor,
      [
        { value: "touchpad", label: "Touchpad", section: "Touch-Bedienung" },
        { value: "direct", label: "Direkt tippen" },
        { value: "zoom", label: "Zoom zurücksetzen", separator: true, disabled: s === 1 },
      ],
      { checked: mode },
    );
    if (v === "zoom") (s = 1), applyZoom();
    else if (v) {
      mode = v as Mode;
      modeBtn.title = `Touch-Bedienung: ${modeLabel[mode]}`;
      drawOverlay();
    }
  };
  const modeBtn: HTMLButtonElement = toolButton(ICON.handTap, "Touch-Bedienung", () => void pickMode(modeBtn), { compact: true });
  modeBtn.title = `Touch-Bedienung: ${modeLabel[mode]}`;
  const lupeBtn = toolButton(ICON.magnify, "Zeiger-Lupe", () => {
    lupeOn = !lupeOn;
    lupeBtn.classList.toggle("on", lupeOn);
    lupeBtn.setAttribute("aria-pressed", String(lupeOn));
  }, { compact: true });
  lupeBtn.setAttribute("aria-pressed", "false");

  return {
    /** Nur auf Geräten mit Touch, sonst leer. */
    buttons: isTouch() ? [modeBtn, lupeBtn, kb.button, toolSep()] : [],
    pickMode,
    hide: () => kb.hide(),
    dispose: () => kb.dispose(),
  };
}
