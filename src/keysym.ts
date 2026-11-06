/** KeyboardEvent → X11-Keysym (RFB KeyEvent). */
const special: Record<string, number> = {
  Backspace: 0xff08, Tab: 0xff09, Enter: 0xff0d, Escape: 0xff1b, Delete: 0xffff,
  Home: 0xff50, ArrowLeft: 0xff51, ArrowUp: 0xff52, ArrowRight: 0xff53, ArrowDown: 0xff54,
  PageUp: 0xff55, PageDown: 0xff56, End: 0xff57, Insert: 0xff63, ContextMenu: 0xff67,
  CapsLock: 0xffe5, NumLock: 0xff7f, ScrollLock: 0xff14, Pause: 0xff13, PrintScreen: 0xff61,
  AltGraph: 0xfe03,
};
const sided: Record<string, [number, number]> = {
  Shift: [0xffe1, 0xffe2], Control: [0xffe3, 0xffe4], Alt: [0xffe9, 0xffea], Meta: [0xffeb, 0xffec],
};

export function keysym(e: KeyboardEvent): number | undefined {
  if (e.key in sided) return sided[e.key][e.code.endsWith("Right") ? 1 : 0];
  if (e.key in special) return special[e.key];
  const f = /^F(\d{1,2})$/.exec(e.key);
  if (f) return 0xffbe + Number(f[1]) - 1;
  const cps = [...e.key];
  if (cps.length === 1) {
    const cp = cps[0].codePointAt(0)!;
    return cp < 0x100 ? cp : 0x01000000 | cp; // Latin-1 direkt, sonst Unicode-Keysym
  }
  return undefined; // "Dead", "Unidentified", …
}
