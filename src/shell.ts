import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { api, type Connection } from "./api";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

let close: (() => void) | null = null;

export async function openShell(c: Connection, onBack: () => void) {
  close?.();
  $("library").hidden = true;
  $("session").hidden = false;
  $("session-title").textContent = c.name;
  $("session-sub").textContent = `SSH · ${c.user}@${c.address}:${c.sshPort}`;
  const status = $("session-status");
  status.textContent = "Verbinde …";

  const host = $("terminal");
  host.replaceChildren();
  const term = new Terminal({
    cursorBlink: true,
    fontFamily: "'Cascadia Mono', 'SF Mono', Menlo, Consolas, monospace",
    fontSize: 13,
    theme: { background: "#161618", foreground: "#E5E5EA", cursor: "#4CD08F" },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(host);
  fit.fit();

  let sid: string | null = null;
  const enc = new TextEncoder();
  const ro = new ResizeObserver(() => {
    fit.fit();
    if (sid) api.shellResize(sid, term.cols, term.rows).catch(() => {});
  });
  ro.observe(host);
  term.onData((d) => sid && api.shellWrite(sid, enc.encode(d)).catch((e) => (status.textContent = String(e))));

  close = () => {
    ro.disconnect();
    if (sid) api.shellClose(sid);
    term.dispose();
    close = null;
  };
  $("session-back").onclick = () => {
    close?.();
    $("session").hidden = true;
    $("library").hidden = false;
    onBack();
  };

  try {
    sid = await api.shellOpen(c.id, term.cols, term.rows, (e) => {
      if (e.kind === "data") term.write(new Uint8Array(e.data));
      else status.textContent = e.reason;
    });
    status.textContent = "";
    term.focus();
  } catch (e) {
    status.textContent = `Keine Verbindung: ${e}`;
  }
}
