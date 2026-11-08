import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import "@xterm/xterm/css/xterm.css";
import { api, HOSTKEY_UNKNOWN } from "./api";
import { askHostKey, askText } from "./ui";
import { ICON } from "./icons";
import { openMenu } from "./menu";
import { handoffDone, toolButton, toolSep, type TabCtx, type TabHandle } from "./tabs";
import { createKeyboard, isTouch, termSequence } from "./osk";
import { resolveTheme, saveTermSettings, TERM_THEMES, termSettings, themeFont } from "./termthemes";
import { localProfile } from "./terminal";

const FONT = "'JetBrains Mono Variable', 'Cascadia Mono', 'SF Mono', Menlo, Consolas, monospace";

/** SSH-Shell als Tab. */
export function shellTab(ctx: TabCtx): TabHandle {
  const c = ctx.conn;
  // Lokales Terminal (Pseudo-Verbindung «local:…») oder SSH.
  const local = localProfile(c.id);
  ctx.setSub(local ? `Dieser Computer · ${local.command}` : `SSH · ${c.user}@${c.address}:${c.sshPort}`);
  const status = ctx.status;
  status.textContent = "Verbinde …";

  const host = Object.assign(document.createElement("div"), { className: "terminal" });
  ctx.pane.prepend(host);
  const s = termSettings();
  const term = new Terminal({
    cursorBlink: true,
    fontFamily: s.fontFamily || FONT,
    fontSize: s.fontSize,
    theme: resolveTheme(s.theme),
    allowProposedApi: true,
  });
  const fit = new FitAddon();
  const search = new SearchAddon();
  term.loadAddon(fit);
  term.loadAddon(search);
  // Emoji und breite Zeichen richtig breit (Unicode 11), Links mit Strg+Klick im Browser öffnen.
  term.loadAddon(new Unicode11Addon());
  term.unicode.activeVersion = "11";
  term.loadAddon(new WebLinksAddon((ev, uri) => (ev.ctrlKey || ev.metaKey) && /^https?:\/\//.test(uri) && void openUrl(uri)));
  term.open(host);

  let sid: string | null = null;
  const enc = new TextEncoder();
  const send = (text: string) => sid && api.shellWrite(sid, enc.encode(text)).catch((e) => (status.textContent = String(e)));
  const refit = () => {
    if (!host.clientWidth) return; // versteckter Tab
    fit.fit();
    if (sid) api.shellResize(sid, term.cols, term.rows).catch(() => {});
  };
  const ro = new ResizeObserver(refit);
  ro.observe(host);
  term.onData(send);

  // Einstellungen wirken sofort auf offene Shells.
  const applySettings = () => {
    const t = termSettings();
    const theme = local?.theme || t.theme; // Profil darf ein eigenes Farbschema haben
    term.options.theme = resolveTheme(theme);
    term.options.fontSize = t.fontSize;
    term.options.fontFamily = local?.font || t.fontFamily || (themeFont(theme) ? `${themeFont(theme)}, ${FONT}` : FONT);
    // Ganze Fläche (Rand, Statuszeile, Bildlaufleiste) in der Farbe des Schemas, kein Schwarz.
    const bg = term.options.theme?.background ?? "";
    host.style.background = ctx.pane.style.background = bg;
    ctx.pane.style.setProperty("--term-fg", term.options.theme?.foreground ?? "");
    refit();
  };
  applySettings();
  // Gebündelte Monospace-Schrift lädt erst bei Bedarf: danach Zeichenbreite neu messen.
  void document.fonts.load("13px 'JetBrains Mono Variable'").then(applySettings);
  addEventListener("sesam:terminal-settings", applySettings);
  const scheme = matchMedia("(prefers-color-scheme: dark)");
  scheme.addEventListener("change", applySettings);

  // Ordner/Dateien aus Explorer/Finder aufs Terminal ziehen: Pfad einfügen (wie Windows Terminal).
  const quote = (p: string) => (/[\s'"&()]/.test(p) ? (local?.command.toLowerCase().includes("cmd") ? `"${p}"` : `'${p.replace(/'/g, "'\\''")}'`) : p);
  const unlistenDrop = getCurrentWebview().onDragDropEvent((ev) => {
    if (ev.payload.type !== "drop" || !ctx.isActive() || !sid) return;
    send(ev.payload.paths.map(quote).join(" "));
    term.focus();
  });

  // ---------- Werkzeugleiste ----------
  const copy = () => {
    const sel = term.getSelection();
    if (sel) api.clipWrite(sel);
    term.focus();
  };
  const paste = async () => {
    const t = await api.clipRead().catch(() => "");
    if (t) term.paste(t);
    term.focus();
  };
  const font = (d: number) => {
    const t = termSettings();
    saveTermSettings({ ...t, fontSize: Math.max(8, Math.min(32, t.fontSize + d)) });
  };
  const find = async () => {
    const q = await askText("Suchen", "Text im Verlauf der Shell", "Suchen");
    if (q) search.findPrevious(q, { caseSensitive: false });
    term.focus();
  };
  const schemeBtn: HTMLButtonElement = toolButton(ICON.font, "Darstellung", async () => {
    const t = termSettings();
    const v = await openMenu(
      schemeBtn,
      [
        { value: "auto", label: "Automatisch (wie System)", section: "Farbschema" },
        ...Object.entries(TERM_THEMES).map(([value, x]) => ({ value, label: x.label })),
        { value: "+", label: "Schrift grösser", shortcut: "Strg++", separator: true },
        { value: "-", label: "Schrift kleiner", shortcut: "Strg+-" },
      ],
      { checked: t.theme },
    );
    if (v === "+" || v === "-") font(v === "+" ? 1 : -1);
    else if (v) saveTermSettings({ ...t, theme: v });
    term.focus();
  });
  // Bildschirmtastatur: xterm-Eingabefeld plus Zusatztasten (Esc, Strg, Pfeile, F-Tasten).
  const kb = createKeyboard({ pane: ctx.pane, input: term.textarea!, press: (k, m) => term.input(termSequence(k, m, term.modes.applicationCursorKeysMode)) });
  ctx.toolbar.append(
    toolButton(ICON.plus, "Neue Shell", () => ctx.openTab("shell", true), { shortcut: "Strg+Umschalt+T" }),
    toolSep(),
    toolButton(ICON.copy, "Kopieren", copy, { shortcut: "Strg+Umschalt+C" }),
    toolButton(ICON.typeClipboard, "Einfügen", paste, { shortcut: "Strg+Umschalt+V" }),
    toolButton(ICON.search, "Suchen", find, { shortcut: "Strg+Umschalt+F" }),
    schemeBtn,
    ...(isTouch() ? [kb.button] : []),
    toolSep(),
    toolButton(ICON.disconnect, "Trennen", () => ctx.closeSelf(), { text: true }),
  );
  // Tastenkürzel wie in Terminal-Apps (Strg+C ohne Umschalt bleibt beim Server).
  term.attachCustomKeyEventHandler((e) => {
    if (e.type !== "keydown" || !e.ctrlKey) return true;
    const k = e.key.toLowerCase();
    if (e.shiftKey && k === "c") return copy(), false;
    if (e.shiftKey && k === "v") return void paste(), false;
    if (e.shiftKey && k === "f") return void find(), false;
    if (e.shiftKey && k === "t") return ctx.openTab("shell", true), false;
    if (!e.shiftKey && (k === "+" || k === "=")) return font(1), false;
    if (!e.shiftKey && k === "-") return font(-1), false;
    return true;
  });

  // ---------- Verbinden ----------
  void (async () => {
    refit();
    const onEvent = (e: Parameters<Parameters<typeof api.shellOpen>[4]>[0]) => {
      if (e.kind === "data") term.write(new Uint8Array(e.data));
      else {
        status.textContent = e.reason;
        ctx.setState("closed");
      }
    };
    let trust: string | null = null;
    for (;;) {
      try {
        const at = ctx.extra?.attach;
        if (at) {
          // Laufende Shell aus einem anderen Fenster: bisherigen Text zeigen, dann weiter.
          if (typeof at.state === "string") term.write(at.state);
          await api.shellAttach(at.sid, onEvent);
          sid = at.sid;
          handoffDone(at.token);
          api.shellResize(sid, term.cols, term.rows).catch(() => {});
        } else if (local) sid = await api.termOpen(local, term.cols || 80, term.rows || 24, onEvent);
        else sid = await api.shellOpen(c.id, term.cols || 80, term.rows || 24, trust, onEvent);
        status.textContent = "";
        // Vorgegebener Befehl (z. B. Setup-Script): getippt, der Benutzer sieht und bestätigt alles im Terminal.
        if (ctx.extra?.command && !at) setTimeout(() => send(ctx.extra!.command!), 400);
        ctx.setState("connected");
        if (ctx.isActive()) term.focus();
      } catch (e) {
        const msg = String(e);
        if (trust === null && msg.startsWith(HOSTKEY_UNKNOWN)) {
          trust = await askHostKey(c.address, msg);
          if (trust) continue;
          status.textContent = "Verbindung abgebrochen: Server nicht bestätigt.";
        } else status.textContent = `Keine Verbindung: ${msg}`;
        ctx.setState("closed");
      }
      break;
    }
  })();

  let handedOff = false;
  /** Sichtbarer Text samt Verlauf (letzte 2000 Zeilen) für die Übergabe; Farben gehen dabei verloren. */
  const bufferText = () => {
    const b = term.buffer.active;
    const lines: string[] = [];
    for (let i = Math.max(0, b.length - 2000); i < b.length; i++) lines.push(b.getLine(i)?.translateToString(true) ?? "");
    while (lines.length && !lines[lines.length - 1]) lines.pop();
    return lines.join("\r\n");
  };
  return {
    handoff: () => (sid ? { sid, state: bufferText() } : null),
    release() {
      handedOff = true;
      this.close();
    },
    close() {
      ro.disconnect();
      kb.dispose();
      removeEventListener("sesam:terminal-settings", applySettings);
      unlistenDrop.then((u) => u());
      scheme.removeEventListener("change", applySettings);
      if (sid && !handedOff) api.shellClose(sid);
      sid = null;
      term.dispose();
    },
    onHide: () => kb.hide(),
    onShow() {
      requestAnimationFrame(() => {
        refit();
        term.focus();
      });
    },
  };
}
