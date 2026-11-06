/** Bildschirmtastatur: verstecktes Eingabefeld für die Touch-Tastatur (Windows/iPadOS) und eigene Zusatztastenleiste. */
import "./touch.css";
import { ICON } from "./icons";
import { toolButton } from "./tabs";

export type Key = Pick<KeyboardEvent, "code" | "key">;
/** Taste mit gehaltenen Modifikatoren drücken und loslassen. */
export type Press = (k: Key, mods: Key[]) => void;

export const isTouch = () => navigator.maxTouchPoints > 0;
const apple = /Mac|iPhone|iPad/.test(navigator.userAgent);

const MODS: [Key, string][] = [
  [{ code: "ControlLeft", key: "Control" }, "Strg"],
  [{ code: "AltLeft", key: "Alt" }, "Alt"],
  [{ code: "MetaLeft", key: "Meta" }, apple ? "Cmd" : "Win"],
];
const NAV: [string, string, string][] = [
  ["ArrowLeft", "←", "Pfeil links"], ["ArrowUp", "↑", "Pfeil hoch"], ["ArrowDown", "↓", "Pfeil runter"], ["ArrowRight", "→", "Pfeil rechts"],
];
/** Tasten, die das Eingabefeld nicht selbst als Text liefert. */
const SPECIAL = /^(Backspace|Enter|Tab|Escape|Delete|Insert|Home|End|PageUp|PageDown|Arrow\w+|F\d{1,2})$/;

/** Buchstaben/Ziffern mit Strg/Alt/Win als echte Taste senden (Unicode-Ereignisse lösen keine Kürzel aus). */
export const charKey = (ch: string, withMods: boolean): Key =>
  withMods && /^[a-z]$/i.test(ch) ? { code: `Key${ch.toUpperCase()}`, key: ch.toLowerCase() }
  : withMods && /^\d$/.test(ch) ? { code: `Digit${ch}`, key: ch }
  : ch === "\n" ? { code: "Enter", key: "Enter" } : { code: "", key: ch };

/** Press für Desktops aus dem vorhandenen send(isDown, key). */
export const pressVia = (send: (isDown: boolean, k: Key) => void): Press => (k, mods) => {
  for (const m of mods) send(true, m);
  send(true, k);
  send(false, k);
  for (const m of [...mods].reverse()) send(false, m);
};

export interface Keyboard {
  button: HTMLButtonElement;
  readonly on: boolean;
  show(): void;
  hide(): void;
  /** Eingabefeld (wieder) fokussieren, wenn die Tastatur an ist. */
  focus(): void;
  dispose(): void;
}

/**
 * `input`: vorhandenes Eingabefeld (xterm), das Text selbst verarbeitet; dann greifen wir nur bei
 * gesetzten Modifikatoren ein. Ohne `input` übersetzt ein eigenes Feld alles in `press`.
 * `avoid(bottom)`: sichtbare Unterkante (clientY) über Tastatur und Leiste, null = frei.
 */
export function createKeyboard(o: { pane: HTMLElement; press: Press; input?: HTMLTextAreaElement; desktop?: boolean; avoid?: (bottom: number | null) => void }): Keyboard {
  const own = !o.input;
  const input = o.input ?? Object.assign(document.createElement("textarea"), { className: "osk-input", rows: 1, spellcheck: false });
  if (own) {
    input.setAttribute("inputmode", "text");
    input.setAttribute("enterkeyhint", "send");
    input.setAttribute("aria-label", "Eingabe für den entfernten Bildschirm");
    o.pane.append(input);
  }
  for (const [a, v] of [["autocapitalize", "off"], ["autocorrect", "off"], ["autocomplete", "off"]]) input.setAttribute(a, v);

  // ---------- Zusatztasten ----------
  const bar = Object.assign(document.createElement("div"), { className: "osk-bar", hidden: true });
  bar.setAttribute("role", "toolbar");
  bar.setAttribute("aria-label", "Zusatztasten");
  const state = new Map<Key, { n: 0 | 1 | 2; t: number; b: HTMLButtonElement }>(); // 0 aus, 1 einmal, 2 gesperrt
  const mods = () => [...state].filter(([, s]) => s.n).map(([k]) => k);
  const renderMod = (s: { n: number; b: HTMLButtonElement }) => {
    s.b.classList.toggle("on", s.n > 0);
    s.b.classList.toggle("locked", s.n === 2);
    s.b.setAttribute("aria-pressed", String(s.n > 0));
    s.b.title = s.n === 2 ? `${s.b.dataset.label} gesperrt (antippen löst)` : `${s.b.dataset.label} (doppelt tippen sperrt)`;
  };
  const press = (k: Key) => {
    o.press(k, mods());
    for (const s of state.values()) if (s.n === 1) (s.n = 0), renderMod(s);
  };
  const keyBtn = (label: string, aria: string, fn: () => void, cls = "") => {
    const b = Object.assign(document.createElement("button"), { type: "button", className: `osk-key ${cls}`, textContent: label });
    b.setAttribute("aria-label", aria);
    b.onpointerdown = (e) => e.preventDefault(); // Fokus (und damit die Touch-Tastatur) behalten
    b.onclick = fn;
    bar.append(b);
    return b;
  };
  keyBtn("Esc", "Escape", () => press({ code: "Escape", key: "Escape" }));
  keyBtn("Tab", "Tabulator", () => press({ code: "Tab", key: "Tab" }));
  for (const [k, label] of MODS) {
    if (k.key === "Meta" && !o.desktop) continue;
    const s = { n: 0 as 0 | 1 | 2, t: 0, b: keyBtn(label, label, () => {
      const now = performance.now();
      s.n = s.n === 0 ? 1 : s.n === 1 && now - s.t < 400 ? 2 : 0;
      s.t = now;
      renderMod(s);
    }, "mod") };
    s.b.dataset.label = label;
    renderMod(s);
    state.set(k, s);
  }
  for (const [code, label, aria] of NAV) keyBtn(label, aria, () => press({ code, key: code }));
  for (let i = 1; i <= 12; i++) keyBtn(`F${i}`, `F${i}`, () => press({ code: `F${i}`, key: `F${i}` }), "fn");
  if (o.desktop) keyBtn("Strg+Alt+Entf", "Strg+Alt+Entf", () => o.press({ code: "Delete", key: "Delete" }, [MODS[0][0], MODS[1][0]]), "wide");
  const closeBtn = keyBtn("", "Tastatur ausblenden", () => hide(), "close");
  closeBtn.innerHTML = ICON.close;
  o.pane.append(bar);

  // ---------- Eingabe ----------
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.isComposing || e.key === "F11" || (!own && !mods().length)) return; // xterm macht den Rest selbst
    const held: Key[] = [];
    const altGr = e.ctrlKey && e.altKey; // Windows meldet AltGr als Strg+Alt
    if (!altGr && e.ctrlKey && e.key !== "Control") held.push(MODS[0][0]);
    if (!altGr && e.altKey && e.key !== "Alt") held.push(MODS[1][0]);
    if (e.metaKey && e.key !== "Meta") held.push(MODS[2][0]);
    const single = [...e.key].length === 1;
    if (SPECIAL.test(e.key) || (single && (held.length || (mods().length && !altGr)))) {
      e.preventDefault();
      e.stopPropagation();
      const k = single ? charKey(e.key, true) : { code: e.code || e.key, key: e.key };
      if (held.length) o.press(k, [...held, ...mods()]);
      else press(k);
    }
  };
  const onBeforeInput = (e: InputEvent) => {
    if (e.isComposing || (!own && !mods().length)) return;
    if (e.inputType === "deleteContentBackward") press({ code: "Backspace", key: "Backspace" });
    else if (e.inputType === "insertLineBreak" || e.inputType === "insertParagraph") press({ code: "Enter", key: "Enter" });
    else if (e.inputType.startsWith("insert") && e.data) for (const ch of e.data) press(charKey(ch, mods().length > 0));
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  const onCompositionEnd = (e: CompositionEvent) => {
    if (!own) return;
    for (const ch of e.data ?? "") press(charKey(ch, false));
    input.value = "";
  };
  input.addEventListener("keydown", onKeyDown, true);
  input.addEventListener("beforeinput", onBeforeInput, true);
  input.addEventListener("compositionend", onCompositionEnd);
  if (own) input.addEventListener("input", (e) => !(e as InputEvent).isComposing && (input.value = ""));
  // Antippen des Bildes nimmt dem Feld den Fokus: zurückholen; sonst (Tastatur-Taste «ausblenden», anderes Feld) aus.
  const onBlur = (e: FocusEvent) => {
    if (!on) return;
    if (e.relatedTarget instanceof Node && o.pane.contains(e.relatedTarget)) return void requestAnimationFrame(() => input.focus({ preventScroll: true }));
    if (own) hide();
  };
  input.addEventListener("blur", onBlur);

  // ---------- Platz für die Tastatur ----------
  const vv = window.visualViewport;
  const vk = (navigator as any).virtualKeyboard as { overlaysContent: boolean; boundingRect: DOMRect; addEventListener: Window["addEventListener"]; removeEventListener: Window["removeEventListener"] } | undefined;
  const place = () => {
    if (!on) return o.avoid ? o.avoid(null) : void (o.pane.style.paddingBottom = "");
    const kb = Math.max(vv ? innerHeight - vv.height - vv.offsetTop : 0, vk?.boundingRect.height ?? 0, 0);
    bar.style.transform = kb ? `translateY(${-kb}px)` : "";
    const bottom = innerHeight - kb - bar.offsetHeight;
    if (o.avoid) o.avoid(bottom);
    else o.pane.style.paddingBottom = `${Math.max(0, o.pane.getBoundingClientRect().bottom - bottom)}px`;
  };
  vv?.addEventListener("resize", place);
  vv?.addEventListener("scroll", place);
  vk?.addEventListener("geometrychange", place);

  let on = false;
  const button = toolButton(ICON.keyboard, "Tastatur", () => (on ? hide() : show()));
  button.setAttribute("aria-pressed", "false");
  const set = (v: boolean) => {
    on = v;
    bar.hidden = !v;
    button.classList.toggle("on", v);
    button.setAttribute("aria-pressed", String(v));
    if (vk) vk.overlaysContent = v; // nur solange aktiv, damit Formulare der Bibliothek normal bleiben
    place();
  };
  const show = () => {
    set(true);
    input.focus({ preventScroll: true });
  };
  const hide = () => {
    if (!on) return;
    set(false);
    for (const s of state.values()) (s.n = 0), renderMod(s);
    if (own) input.blur();
  };

  return {
    button,
    get on() { return on; },
    show,
    hide,
    focus: () => on && input.focus({ preventScroll: true }),
    dispose() {
      hide();
      vv?.removeEventListener("resize", place);
      vv?.removeEventListener("scroll", place);
      vk?.removeEventListener("geometrychange", place);
      input.removeEventListener("keydown", onKeyDown, true);
      input.removeEventListener("beforeinput", onBeforeInput, true);
      input.removeEventListener("blur", onBlur);
      bar.remove();
      if (own) input.remove();
    },
  };
}

// ---------- Shell: Tasten als Terminal-Sequenzen ----------
const CSI_FINAL: Record<string, string> = { ArrowUp: "A", ArrowDown: "B", ArrowRight: "C", ArrowLeft: "D", Home: "H", End: "F" };
const TILDE: Record<string, number> = { Insert: 2, Delete: 3, PageUp: 5, PageDown: 6, F5: 15, F6: 17, F7: 18, F8: 19, F9: 20, F10: 21, F11: 23, F12: 24 };
const SS3: Record<string, string> = { F1: "P", F2: "Q", F3: "R", F4: "S" };

/** Taste mit Modifikatoren → Bytes für das Terminal (xterm-Konventionen). */
export function termSequence(k: Key, mods: Key[], appCursor: boolean): string {
  const ctrl = mods.some((m) => m.key === "Control"), alt = mods.some((m) => m.key === "Alt");
  const m = 1 + (alt ? 2 : 0) + (ctrl ? 4 : 0);
  if (k.key in CSI_FINAL) return m > 1 ? `\x1b[1;${m}${CSI_FINAL[k.key]}` : `\x1b${appCursor ? "O" : "["}${CSI_FINAL[k.key]}`;
  if (k.key in SS3) return m > 1 ? `\x1b[1;${m}${SS3[k.key]}` : `\x1bO${SS3[k.key]}`;
  if (k.key in TILDE) return `\x1b[${TILDE[k.key]}${m > 1 ? `;${m}` : ""}~`;
  let s = { Escape: "\x1b", Tab: "\t", Enter: "\r", Backspace: "\x7f" }[k.key] ?? ([...k.key].length === 1 ? k.key : "");
  if (ctrl && s.length === 1) {
    const c = s.toUpperCase().charCodeAt(0);
    if (c >= 0x40 && c <= 0x5f) s = String.fromCharCode(c & 0x1f);
    else if (s === " ") s = "\x00";
  }
  return alt && s ? `\x1b${s}` : s;
}
