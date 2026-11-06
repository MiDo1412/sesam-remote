import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import "@xterm/xterm/css/xterm.css";
import { api, HOSTKEY_UNKNOWN } from "./api";
import { askHostKey, askText } from "./ui";
import { ICON } from "./icons";
import { openMenu } from "./menu";
import { toolButton, toolSep, type TabCtx, type TabHandle } from "./tabs";
import { resolveTheme, saveTermSettings, TERM_THEMES, termSettings } from "./termthemes";

const FONT = "'Cascadia Mono', 'SF Mono', Menlo, Consolas, monospace";

/** SSH-Shell als Tab. */
export function shellTab(ctx: TabCtx): TabHandle {
  const c = ctx.conn;
  ctx.setSub(`SSH · ${c.user}@${c.address}:${c.sshPort}`);
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
    term.options.theme = resolveTheme(t.theme);
    term.options.fontSize = t.fontSize;
    term.options.fontFamily = t.fontFamily || FONT;
    host.style.background = term.options.theme?.background ?? "";
    refit();
  };
  applySettings();
  addEventListener("sesam:terminal-settings", applySettings);
  const scheme = matchMedia("(prefers-color-scheme: dark)");
  scheme.addEventListener("change", applySettings);

  // ---------- Werkzeugleiste ----------
  const copy = () => {
    const sel = term.getSelection();
    if (sel) navigator.clipboard.writeText(sel);
    term.focus();
  };
  const paste = async () => {
    const t = await navigator.clipboard.readText().catch(() => "");
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
  ctx.toolbar.append(
    toolButton(ICON.plus, "Neue Shell", () => ctx.openTab("shell", true), { shortcut: "Strg+Umschalt+T" }),
    toolSep(),
    toolButton(ICON.copy, "Kopieren", copy, { shortcut: "Strg+Umschalt+C" }),
    toolButton(ICON.typeClipboard, "Einfügen", paste, { shortcut: "Strg+Umschalt+V" }),
    toolButton(ICON.search, "Suchen", find, { shortcut: "Strg+Umschalt+F" }),
    schemeBtn,
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
        sid = await api.shellOpen(c.id, term.cols || 80, term.rows || 24, trust, onEvent);
        status.textContent = "";
        // Vorgegebener Befehl (z. B. Setup-Script): getippt, der Benutzer sieht und bestätigt alles im Terminal.
        if (ctx.extra?.command) setTimeout(() => send(ctx.extra!.command!), 400);
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

  return {
    close() {
      ro.disconnect();
      removeEventListener("sesam:terminal-settings", applySettings);
      scheme.removeEventListener("change", applySettings);
      if (sid) api.shellClose(sid);
      sid = null;
      term.dispose();
    },
    onShow() {
      requestAnimationFrame(() => {
        refit();
        term.focus();
      });
    },
  };
}
