/** Eigene Titelleiste: unter Windows ohne Fensterrahmen (Logo links, Menü «⋯» und Fenstersteuerung rechts),
 *  unter macOS bleibt die Menüleiste nativ und die Titelleiste ist transparent (Ampel links). */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { openMenu, type MenuItem } from "./menu";
import { fl } from "./icons";
import minusSvg from "@fluentui/svg-icons/icons/subtract_20_regular.svg?raw";
import squareSvg from "@fluentui/svg-icons/icons/square_20_regular.svg?raw";
import restoreSvg from "@fluentui/svg-icons/icons/square_multiple_20_regular.svg?raw";
import closeSvg from "@fluentui/svg-icons/icons/dismiss_20_regular.svg?raw";
import menuSvg from "@fluentui/svg-icons/icons/navigation_20_regular.svg?raw";

// Eigene Symbole statt der Windows-Symbolschrift (die nicht überall verfügbar ist und dann bunt ersetzt wird).
const glyph = (svg: string) => fl(svg, 14);

/** iPad (meldet sich in der WebView als Mac, hat aber Touch), iPhone, Android: keine eigenen Fenster, keine lokalen
 *  Programme, keine Desktop-Verknüpfungen; die Oberfläche blendet diese Funktionen aus (Klasse «mobile»). */
export const isMobile = /iPad|iPhone|Android/.test(navigator.userAgent) || (navigator.userAgent.includes("Mac") && navigator.maxTouchPoints > 1);
export const isMac = navigator.userAgent.includes("Mac") && !isMobile;
/** Grösse der Webansicht in Punkten. `screen` bleibt auf dem iPhone Duo beim Aufklappen auf dem Wert des Aussenbildschirms
 *  stehen und zeigt auf dem iPad in Split View den ganzen Bildschirm; outerWidth/-Height folgen dem Fenster. */
const viewPt = () => (outerWidth > 0 && outerHeight > 0 ? { w: outerWidth, h: outerHeight } : { w: screen.width, h: screen.height });
const phoneNow = () => isMobile && Math.min(viewPt().w, viewPt().h) < 600;
/** Schmales Fenster (iPhone, iPhone Duo zugeklappt, iPad in schmaler Split View): Navigationsstapel statt Spalten,
 *  eigene Seiten für Gruppen und Details (Klasse «phone»). Wechselt live beim Auf- und Zuklappen; dann kommt «sesam:phone». */
export let isPhone = phoneNow();
// Fokusring nur nach Tastaturbedienung (styles.css: body.kbd); Klick oder Tippen blendet ihn wieder aus.
addEventListener("keydown", (e) => (e.key === "Tab" || e.key.startsWith("Arrow")) && document.body.classList.add("kbd"), true);
addEventListener("pointerdown", () => document.body.classList.remove("kbd"), true);
/** Apple-Tastatur (Mac oder iPad mit Tastatur): Kürzel mit ⌘ anzeigen. */
const appleKeys = navigator.userAgent.includes("Mac") || /iPad|iPhone/.test(navigator.userAgent);

/** Fenstermaterial und Bedienungshilfen des Systems als Klassen am body (docs/design-konzept.md):
 *  material-vibrancy / material-mica / ohne = deckend, reduce-transparency, increase-contrast. */
function initMaterial() {
  invoke<{ material: string; reduceTransparency: boolean; increaseContrast: boolean }>("ui_material")
    .then((m) => {
      const b = document.body.classList;
      if (m.material !== "none") b.add(`material-${m.material}`, "material");
      b.toggle("reduce-transparency", m.reduceTransparency);
      b.toggle("increase-contrast", m.increaseContrast);
    })
    .catch(() => {});
}

export function initTitlebar(actions: { command(c: string): void; tool(t: string): void; help(id: string): void }) {
  initMaterial();
  // Fenster an allen freien Flächen von Titel-, Tab- und Sitzungsleiste ziehen («deep»: auch auf Text und leeren
  // Containern; Knöpfe, Tabs und Eingabefelder bleiben bedienbar). Doppelklick: Zoomen bzw. Maximieren.
  const bars = document.querySelectorAll<HTMLElement>("header.toolbar, header.session-bar");
  for (const h of document.querySelectorAll<HTMLElement>("header.toolbar, header.session-bar, .session-head")) h.setAttribute("data-tauri-drag-region", "deep");
  if (isMobile) return void (document.body.classList.add("mobile", ...(isPhone ? ["phone"] : [])), touchScale(), longPressMenus());
  if (isMac) return void document.body.classList.add("mac");
  document.body.classList.add("custom-titlebar");
  const win = getCurrentWindow();

  // Kurzes App-Menü (☰) oben rechts (wie Edge/Teams), höchstens 7 Einträge. Alles Weitere über die Befehlspalette,
  // Kontextmenüs, Kürzel und die Verbindungsleiste.
  const items: MenuItem[] = [
    { value: "cmd:new", label: "Neue Verbindung …", shortcut: "Strg+N" },
    { value: "cmd:quick", label: "Schnelle Verbindung …", shortcut: "Strg+K" },
    { value: "cmd:terminal", label: "Neues Terminal", shortcut: "Strg+T" },
    { value: "cmd:palette", label: "Befehlspalette", shortcut: "Strg+Umschalt+P", separator: true },
    { value: "cmd:settings", label: "Einstellungen", shortcut: "Strg+," },
    { value: "help:", label: "Hilfe" },
    { value: "cmd:about", label: "Über Sesam Remote" },
  ];
  const menu = async (anchor: HTMLElement) => {
    const v = await openMenu(anchor, items);
    if (!v) return;
    const [kind, arg] = [v.slice(0, v.indexOf(":")), v.slice(v.indexOf(":") + 1)];
    if (kind === "cmd") actions.command(arg);
    else actions.help(arg);
  };

  const maxBtns: HTMLElement[] = [];
  for (const h of bars) {
    // Rechts: Minimieren, Maximieren/Wiederherstellen, Schliessen (Glyphen wie Windows 11).
    const ctl = document.createElement("div");
    ctl.className = "win-controls";
    const mk = (glyph: string, label: string, onClick: () => void, cls = "") => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = `win-btn ${cls}`;
      b.innerHTML = glyph;
      b.title = label;
      b.setAttribute("aria-label", label);
      b.onclick = onClick;
      return b;
    };
    const max = mk(glyph(squareSvg), "Maximieren", () => win.toggleMaximize());
    const more = mk(fl(menuSvg, 16), "Menü (Alt)", () => void menu(more), "more");
    more.setAttribute("aria-haspopup", "menu");
    more.setAttribute("aria-keyshortcuts", "Alt");
    const sep = Object.assign(document.createElement("span"), { className: "win-sep" });
    sep.setAttribute("aria-hidden", "true");
    ctl.append(more, sep, mk(glyph(minusSvg), "Minimieren", () => win.minimize()), max, mk(glyph(closeSvg), "Schliessen", () => win.close(), "close"));
    h.append(ctl);
    const sync = async () => {
      const on = await win.isMaximized();
      max.innerHTML = glyph(on ? restoreSvg : squareSvg);
      max.title = on ? "Wiederherstellen" : "Maximieren (Andocklayouts: Win+Z)";
      max.setAttribute("aria-label", max.title);
    };
    void sync();
    void win.onResized(sync);
    maxBtns.push(max);
  }

  // Windows 11: Andocklayouts beim Darüberfahren (src-tauri/src/snap.rs). Über dem sichtbaren Knopf liegt ein
  // natives Kindfenster; die Maus kommt dort nicht in der Webansicht an, Hover und Drücken meldet Rust.
  if (navigator.userAgent.includes("Windows")) {
    let last = "";
    const place = () => {
      const r = maxBtns.find((b) => b.offsetParent !== null)?.getBoundingClientRect();
      const d = devicePixelRatio;
      const [x, y] = r ? [Math.round(r.left * d), Math.round(r.top * d)] : [0, 0];
      const [w, h] = r && r.width ? [Math.round(r.right * d) - x, Math.round(r.bottom * d) - y] : [0, 0];
      const key = `${x},${y},${w},${h}`;
      if (key !== last) void invoke("snap_button", { x, y, w, h }).then(() => (last = key), () => {});
    };
    const ro = new ResizeObserver(place);
    for (const b of maxBtns) ro.observe(b);
    addEventListener("resize", place);
    void win.listen<string>("snap-button", (e) => {
      for (const b of maxBtns) for (const c of ["hover", "down"]) b.classList.toggle(c, e.payload === c);
    });
  }

  // Alt allein öffnet das Menü wie in Windows-Apps. Nicht, solange das Remote-Bild den Fokus hat (Alt gehört dann nach drüben).
  let altAlone = false;
  addEventListener("keydown", (e) => (altAlone = e.key === "Alt" && !e.repeat && !(document.activeElement instanceof HTMLCanvasElement)), true);
  addEventListener("pointerdown", () => (altAlone = false), true);
  addEventListener("keyup", (e) => {
    if (e.key !== "Alt" || !altAlone) return;
    altAlone = false;
    const btn = [...document.querySelectorAll<HTMLElement>(".win-btn.more")].find((b) => b.offsetParent !== null);
    if (btn && !document.querySelector("dialog[open]")) {
      e.preventDefault();
      void menu(btn);
    }
  }, true);
}

/** Kürzel so anzeigen, wie das System sie schreibt: macOS ⌘ ⇧ ⌥, sonst Strg/Umschalt/Alt. */
export function keyLabel(s: string) {
  return appleKeys ? s.replace(/Strg\+/g, "⌘").replace(/Umschalt\+/g, "⇧").replace(/Alt\+/g, "⌥").replace(/Entf/g, "⌫") : s;
}

/** iPad: WKWebView meldet langes Drücken nicht als «contextmenu». Nach 550 ms ohne Bewegung lösen wir es selbst aus,
 *  damit Kontextmenüs (Karten, Dateien, Tabs) wie auf dem Mac erreichbar sind. Terminal und Remote-Bild haben eigene Gesten. */
function longPressMenus() {
  let timer = 0, x = 0, y = 0, quiet = 0;
  const cancel = () => clearTimeout(timer);
  addEventListener("click", (c) => Date.now() < quiet && (c.preventDefault(), c.stopPropagation()), true);
  addEventListener("pointerdown", (e) => {
    cancel();
    const t = e.target as Element;
    if (e.pointerType !== "touch" || t.closest("input, textarea, select, .xterm, .desktop, [contenteditable]")) return;
    [x, y] = [e.clientX, e.clientY];
    timer = window.setTimeout(() => {
      t.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: x, clientY: y }));
      quiet = Date.now() + 700; // der «click» beim Loslassen soll nichts öffnen
    }, 550);
  }, true);
  addEventListener("pointermove", (e) => Math.hypot(e.clientX - x, e.clientY - y) > 10 && cancel(), true);
  addEventListener("pointerup", cancel, true);
  addEventListener("pointercancel", cancel, true);
}

/** Quer (iPhone, iPhone Duo aufgeklappt): WKWebView meldet in env(safe-area-inset-*) weiter die Werte vom Hochformat (oben die Insel, links/rechts 0),
 *  auch nach einem Neustart im Querformat. Dann liegt die Insel seitlich über dem Inhalt: oben 0, links und rechts den
 *  Abstand der Insel. Meldet WebKit seitliche Werte, gelten sie unverändert. */
function fixSafeArea(landscape: boolean) {
  const root = document.documentElement.style;
  for (const k of ["t", "r", "l"]) root.removeProperty(`--sa-${k}`);
  if (!landscape) return;
  const probe = document.createElement("div");
  probe.style.cssText = "position:fixed;visibility:hidden;padding:env(safe-area-inset-top) env(safe-area-inset-right) 0 env(safe-area-inset-left)";
  document.body.append(probe);
  const s = getComputedStyle(probe);
  const [top, right, left] = [s.paddingTop, s.paddingRight, s.paddingLeft].map(parseFloat);
  probe.remove();
  // iPhone Duo zugeklappt quer: iOS meldet nur die Seite der Statusleiste, um 180° gedreht liegt die Kamera aber
  // gegenüber. Beidseitig wie bei jedem iPhone quer.
  if (isPhone && !left !== !right) {
    root.setProperty("--sa-l", `${Math.max(left, right)}px`);
    root.setProperty("--sa-r", `${Math.max(left, right)}px`);
    return;
  }
  if (left || right || top < 40) return; // oben nur die Statusleiste (iPad): Werte stimmen
  root.setProperty("--sa-t", "0px");
  // iPhone quer: beidseitig wie iOS selbst. iPhone Duo aufgeklappt (breite Oberfläche): WebKit und UIKit melden die
  // Kamera als «oben» 82 pt, sie liegt aber quer immer rechts (Statusleiste rechts, auch um 180° gedreht).
  root.setProperty("--sa-l", isPhone ? `${top}px` : "0px");
  root.setProperty("--sa-r", `${top}px`);
}

/** iPad: die ganze Oberfläche ist für die Maus gebaut (13 px Text, 32 px Knöpfe). Einheitlich vergrössert ergibt das
 *  rund 17 pt Text und mit den Mindesthöhen in styles.css 44-pt-Touchziele (Apple HIG); Koordinaten bleiben stimmig. */
function touchScale() {
  const meta = document.querySelector('meta[name="viewport"]');
  // WKWebView rechnet «device-width» beim Vergrössern nicht neu: Breite selbst setzen, auch nach Drehen, Auf-/Zuklappen
  // und Split View. Ändert sich dabei schmal/breit (iPhone Duo), wechselt die Oberfläche ohne Neuladen.
  const apply = () => {
    const { w, h } = viewPt();
    const phone = phoneNow();
    if (phone !== isPhone) {
      isPhone = phone;
      document.body.classList.toggle("phone", phone);
      dispatchEvent(new Event("sesam:phone"));
    }
    // iPad mini: etwas weniger, sonst wird es eng; iPhone: 1,15 (Schrift 13 px ≈ 15 pt, Knöpfe 38 px ≈ 44 pt)
    // Schmale iPad-Oberfläche (Duo aufgeklappt hochkant, iPad in Split View): höchstens so stark vergrössern, dass
    // 640 px Breite bleiben, sonst ragen Einstellungen und Blätter über den Rand.
    const scale = isPhone ? 1.15 : Math.max(1, Math.min(Math.min(screen.width, screen.height) >= 800 ? 1.3 : 1.2, w / 640));
    const content = `width=${Math.round(w / scale)}, initial-scale=${scale}, minimum-scale=${scale}, maximum-scale=${scale}, user-scalable=no, viewport-fit=cover`;
    if (meta?.getAttribute("content") !== content) meta?.setAttribute("content", content);
    requestAnimationFrame(() => fixSafeArea(w > h));
    setTimeout(() => fixSafeArea(w > h), 500); // beim Start meldet WebKit die Ränder oft erst später, ohne resize
  };
  apply();
  addEventListener("resize", apply);
}
