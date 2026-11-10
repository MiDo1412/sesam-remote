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
import { openMenu } from "./menu";
import { ICON } from "./icons";
import { handoffDone, toolButton, toolSep, type TabCtx, type TabHandle } from "./tabs";
import { createKeyboard, isTouch, termSequence } from "./osk";
import { resolveTheme, saveTermSettings, termSettings, themeFont } from "./termthemes";
import { schemeGallery } from "./schemegallery";
import { localProfile, profileTheme, setProfileTheme } from "./terminal";
import { localNetworkText } from "./ubuntu";

const FONT = "'JetBrains Mono Variable', 'Cascadia Mono', 'SF Mono', Menlo, Consolas, monospace";

/** SSH-Shell als Tab. */
/** Alle offenen Shells in diesem Fenster (für «An alle Shells senden»). */
const allShells = new Set<{ send: (d: string) => unknown; name: string }>();

export function shellTab(ctx: TabCtx): TabHandle {
  const c = ctx.conn;
  // Lokales Terminal (Pseudo-Verbindung «local:…») oder SSH.
  const local = localProfile(c);
  ctx.setSub(local ? `Lokal · ${local.name} · ${local.cwd || "Benutzerordner"}` : `SSH · ${c.user}@${c.address}:${c.sshPort}`);
  if (local && !local.cwd) void api.localHome().then((h) => ctx.setSub(`Lokal · ${local.name} · ${h}`)).catch(() => {});
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
  // «An alle Shells senden»: Eingabe geht zusätzlich an alle anderen offenen Shells (wie Broadcast im Windows Terminal).
  const me = { send, name: c.name };
  allShells.add(me);
  term.onData((d) => {
    send(d);
    if (broadcast) for (const s of allShells) if (s !== me) s.send(d);
  });

  // Einstellungen wirken sofort auf offene Shells.
  const applySettings = () => {
    const t = termSettings();
    const theme = local ? profileTheme(local, t.theme) : t.theme; // lokal: Design je Profil (Standard: Systemvorbild)
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
  let broadcast = false;
  const broadcastBtn: HTMLButtonElement = toolButton(ICON.span, "An alle Shells senden", () => {
    broadcast = !broadcast;
    broadcastBtn.classList.toggle("on", broadcast);
    broadcastBtn.setAttribute("aria-pressed", String(broadcast));
    status.textContent = broadcast ? `Eingabe geht an alle offenen Shells (${allShells.size}).` : "";
    term.focus();
  }, { secondary: true });
  // Textbausteine je Verbindung (lokal gespeichert, keine Passwörter hinein).
  const snipKey = `sesam.snippets.${c.id.startsWith("local:") ? "local" : c.id}`;
  const snippets = async () => {
    let list: string[] = [];
    try {
      list = JSON.parse(localStorage.getItem(snipKey) ?? "[]");
    } catch {}
    const v = await openMenu(schemeBtn, [
      ...list.map((t, i) => ({ value: String(i), label: t.length > 60 ? `${t.slice(0, 57)}…` : t })),
      { value: "new", label: "Neuer Textbaustein …", separator: list.length > 0 },
      ...(list.length ? [{ value: "clear", label: "Alle Textbausteine löschen" }] : []),
    ]);
    if (v === "new") {
      const t = await askText("Neuer Textbaustein", "Text oder Befehl, der mit einem Klick in die Shell geht. «\\n» am Ende führt ihn aus. Keine Passwörter speichern.", "Sichern");
      if (t) localStorage.setItem(snipKey, JSON.stringify([...list, t]));
    } else if (v === "clear") localStorage.removeItem(snipKey);
    else if (v !== null) send(list[Number(v)].replace(/\\n$/, "\r"));
    term.focus();
  };

  // «Darstellung»: Galerie als Aufklappfeld; Darüberfahren zeigt das Schema sofort in dieser Shell.
  let pop: HTMLElement | null = null;
  const closePop = () => {
    if (!pop) return;
    pop.remove();
    pop = null;
    applySettings(); // Vorschau zurücknehmen
    removeEventListener("pointerdown", outside, true);
    term.focus();
  };
  const outside = (e: Event) => {
    if (pop && !pop.contains(e.target as Node) && !schemeBtn.contains(e.target as Node)) closePop();
  };
  const schemeBtn: HTMLButtonElement = toolButton(ICON.font, "Darstellung", () => {
    if (pop) return closePop();
    pop = document.createElement("div");
    pop.className = "scheme-pop";
    pop.setAttribute("role", "dialog");
    pop.setAttribute("aria-label", "Darstellung der Shell");
    const head = document.createElement("div");
    head.className = "scheme-pop-head";
    const title = document.createElement("h3");
    title.textContent = "Farbschema";
    const smaller = Object.assign(document.createElement("button"), { type: "button", textContent: "A−", title: "Schrift kleiner (Strg+-)" });
    const bigger = Object.assign(document.createElement("button"), { type: "button", textContent: "A+", title: "Schrift grösser (Strg++)" });
    smaller.onclick = () => font(-1);
    bigger.onclick = () => font(1);
    head.append(title, smaller, bigger);
    const gallery = schemeGallery(
      local ? profileTheme(local, termSettings().theme) : termSettings().theme,
      (id) => {
        // Lokal: Design dieses Profils; SSH: Design aus den Einstellungen.
        if (local) setProfileTheme(local.id, id);
        else saveTermSettings({ ...termSettings(), theme: id });
        closePop();
      },
      (id) => {
        if (!id) return applySettings();
        term.options.theme = resolveTheme(id);
        host.style.background = ctx.pane.style.background = term.options.theme.background ?? "";
      },
    );
    pop.append(head, gallery);
    pop.onkeydown = (e) => e.key === "Escape" && (e.stopPropagation(), closePop());
    document.body.append(pop);
    const r = schemeBtn.getBoundingClientRect();
    pop.style.top = `${r.bottom + 6}px`;
    pop.style.right = `${Math.max(8, innerWidth - r.right)}px`;
    addEventListener("pointerdown", outside, true);
    gallery.querySelector<HTMLElement>("[aria-checked=true]")?.focus();
  });
  // Bildschirmtastatur: xterm-Eingabefeld plus Zusatztasten (Esc, Strg, Pfeile, F-Tasten).
  const kb = createKeyboard({ pane: ctx.pane, input: term.textarea!, press: (k, m) => term.input(termSequence(k, m, term.modes.applicationCursorKeysMode)) });
  ctx.toolbar.append(
    toolButton(ICON.plus, "Neue Shell", () => ctx.openTab("shell", true), { shortcut: "Strg+Umschalt+T" }),
    toolSep(),
    toolButton(ICON.copy, "Kopieren", copy, { shortcut: "Strg+Umschalt+C" }),
    toolButton(ICON.typeClipboard, "Einfügen", paste, { shortcut: "Strg+Umschalt+V" }),
    toolButton(ICON.search, "Suchen", find, { shortcut: "Strg+Umschalt+F" }),
    broadcastBtn,
    toolButton(ICON.typeClipboard, "Textbausteine", () => void snippets(), { secondary: true }),
    schemeBtn,
    ...(isTouch() ? [kb.button] : []),
    toolSep(),
    local ? toolButton(ICON.close, "Schliessen", () => ctx.closeSelf(), { text: true }) : toolButton(ICON.disconnect, "Trennen", () => ctx.closeSelf(), { text: true }),
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

  // ---------- Rechner: Raspberry Pi, Temperatur, Unterspannung ----------
  // Einmal nach dem Verbinden, dann jede Minute, solange der Tab sichtbar ist (vcgencmd nur auf Raspberry Pi OS).
  const baseSub = local ? "" : `SSH · ${c.user}@${c.address}:${c.sshPort}`;
  let hostTimer = 0;
  const watchHost = async () => {
    clearTimeout(hostTimer);
    const s = sid;
    if (!s) return;
    if (ctx.isActive()) {
      const p = await api.shellProbe(s).catch(() => null);
      if (!p || s !== sid) return;
      const pi = p.model.startsWith("Raspberry Pi");
      if (pi || p.osId === "ubuntu") void api.connSetOs(c.id, pi ? "raspberry" : "ubuntu").then((changed) => changed && dispatchEvent(new CustomEvent("sesam:library-changed"))).catch(() => {});
      const parts = [baseSub, pi ? p.model.replace(/ Rev [\d.]+$/, "") : "", p.temp != null ? `${Math.round(p.temp)} °C` : "", p.undervoltNow ? "⚠ Unterspannung" : p.undervoltSinceBoot ? "Unterspannung seit dem Start" : ""];
      ctx.setSub(parts.filter(Boolean).join(" · "));
      if (!pi && p.temp == null) return; // nichts zu beobachten
    }
    hostTimer = window.setTimeout(watchHost, 60_000);
  };

  // ---------- Verbinden ----------
  const connect = async () => {
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
        if (!local) void watchHost();
      } catch (e) {
        const msg = String(e);
        if (trust === null && msg.startsWith(HOSTKEY_UNKNOWN)) {
          trust = await askHostKey(c.address, msg);
          if (trust) continue;
          status.textContent = "Verbindung abgebrochen: Server nicht bestätigt.";
        } else status.textContent = localNetworkText(msg, c.address) ?? `Keine Verbindung: ${msg}`;
        ctx.setState("closed");
      }
      break;
    }
  };
  void connect();

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
    reconnect() {
      if (sid) api.shellClose(sid);
      sid = null;
      term.write("\r\n\x1b[2m— Neu verbinden —\x1b[0m\r\n");
      void connect();
    },
    autoReconnect: !local,
    handoff: () => (sid ? { sid, state: bufferText() } : null),
    release() {
      handedOff = true;
      this.close();
    },
    close() {
      clearTimeout(hostTimer);
      ro.disconnect();
      kb.dispose();
      closePop();
      allShells.delete(me);
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
