import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, HOSTKEY_UNKNOWN, type Connection, type Entry, type Listing, type Progress } from "./api";
import { ask, askHostKey, askText, showView, tell } from "./ui";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = "") => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
};

const num = new Intl.NumberFormat("de-CH", { maximumFractionDigits: 1 });
export function formatSize(n: number) {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let i = -1;
  do {
    n /= 1024;
    i++;
  } while (n >= 1024 && i < units.length - 1);
  return `${num.format(n)} ${units[i]}`;
}

function formatDate(secs: number | null) {
  if (secs === null) return "";
  const d = new Date(secs * 1000);
  const now = new Date();
  const time = d.toLocaleTimeString("de-CH", { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === now.toDateString()) return `heute, ${time}`;
  return d.toLocaleDateString("de-CH", { day: "numeric", month: "short", ...(d.getFullYear() !== now.getFullYear() && { year: "numeric" }) });
}

type Side = "local" | "remote";

interface Pane {
  side: Side;
  root: HTMLElement;
  body: HTMLElement;
  pathEl: HTMLElement;
  listing: Listing | null;
  selected: Set<string>;
  /** Pfad-Trenner der Seite. */
  sep: string;
}

let close: (() => void) | null = null;

export async function openFiles(c: Connection, onBack: () => void) {
  close?.();
  showView("files");
  $("session-title").textContent = c.name;
  $("session-sub").textContent = `SFTP über SSH · ${c.user}@${c.address}`;
  const status = $("session-status");
  status.textContent = "Verbinde …";

  const host = $("files");
  host.replaceChildren();
  const isWin = navigator.userAgent.includes("Windows");
  const panes: Record<Side, Pane> = {
    local: makePane("local", "Dieser Computer", isWin ? "\\" : "/"),
    remote: makePane("remote", c.name, "/"),
  };
  const transfers = el("section", "transfers");
  transfers.setAttribute("aria-label", "Übertragungen");
  const transferHead = el("div", "transfers-head");
  transferHead.append(el("h2", "", "Übertragungen"), el("span", "sub", "Zwischen den Seiten ziehen, Dateien aus dem Explorer/Finder rechts ablegen, oder doppelklicken."));
  const transferList = el("div", "transfer-list");
  transfers.append(transferHead, transferList);
  const grid = el("div", "panes");
  grid.append(panes.local.root, panes.remote.root);
  host.append(grid, transfers);

  let sid: string | null = null;
  const join = (p: Pane, name: string) => {
    const dir = p.listing!.path;
    return dir === "" ? name : dir.endsWith(p.sep) ? dir + name : dir + p.sep + name;
  };

  function makePane(side: Side, title: string, sep: string): Pane {
    const root = el("section", "pane");
    root.dataset.side = side;
    root.setAttribute("aria-label", title);
    const head = el("div", "pane-head");
    const up = el("button", "plain", "‹");
    up.type = "button";
    up.title = "Übergeordneter Ordner";
    up.setAttribute("aria-label", "Übergeordneter Ordner");
    const refresh = el("button", "plain", "↻");
    refresh.type = "button";
    refresh.title = "Neu laden";
    refresh.setAttribute("aria-label", "Neu laden");
    const pathEl = el("span", "path mono");
    head.append(up, el("h2", "", title), pathEl, el("span", "grow"), refresh);
    const table = el("table");
    const thead = el("thead");
    const tr = el("tr");
    tr.append(el("th", "", "Name"), el("th", "size", "Grösse"), el("th", "date", "Geändert"));
    thead.append(tr);
    const body = el("tbody");
    table.append(thead, body);
    const scroller = el("div", "pane-list");
    scroller.append(table);
    root.append(head, scroller);
    const pane: Pane = { side, root, body, pathEl, listing: null, selected: new Set(), sep };
    up.onclick = () => pane.listing?.parent != null && load(pane, pane.listing.parent);
    refresh.onclick = () => pane.listing && load(pane, pane.listing.path);
    return pane;
  }

  async function load(p: Pane, path: string) {
    try {
      p.listing = p.side === "local" ? await api.localList(path) : await api.sftpList(sid!, path);
      p.selected.clear();
      render(p);
    } catch (e) {
      status.textContent = String(e);
    }
  }

  function render(p: Pane) {
    const l = p.listing!;
    p.pathEl.textContent = l.path || "Laufwerke";
    p.pathEl.title = l.path;
    p.body.replaceChildren(
      ...l.entries.map((e) => {
        const tr = el("tr");
        tr.tabIndex = -1;
        tr.dataset.name = e.name;
        tr.setAttribute("aria-selected", String(p.selected.has(e.name)));
        const name = el("td", "name");
        name.append(el("span", e.dir ? "icon dir" : "icon file"), document.createTextNode(e.name));
        tr.append(name, el("td", "size", e.dir ? "—" : formatSize(e.size)), el("td", "date", formatDate(e.modified)));
        tr.onclick = (ev) => select(p, e.name, ev.ctrlKey || ev.metaKey);
        tr.ondblclick = () => activate(p, e);
        tr.onkeydown = (ev) => {
          if (ev.key === "Enter") activate(p, e);
          if (p.side === "remote" && ev.key === "F2") renameRemote();
          if (p.side === "remote" && ev.key === "Delete") deleteRemote();
          if (ev.key === "Backspace" && l.parent != null) load(p, l.parent);
        };
        tr.onpointerdown = (ev) => startDrag(p, e.name, ev);
        return tr;
      }),
    );
    if (!l.entries.length) {
      const tr = el("tr");
      const td = el("td", "empty", "Leerer Ordner");
      td.colSpan = 3;
      tr.append(td);
      p.body.append(tr);
    }
  }

  function select(p: Pane, name: string, toggle: boolean) {
    if (!toggle) p.selected.clear();
    if (toggle && p.selected.has(name)) p.selected.delete(name);
    else p.selected.add(name);
    for (const tr of p.body.querySelectorAll<HTMLElement>("tr[data-name]")) tr.setAttribute("aria-selected", String(p.selected.has(tr.dataset.name!)));
  }

  function activate(p: Pane, e: Entry) {
    if (e.dir) load(p, join(p, e.name));
    else transfer(p, [e.name]);
  }

  // ---------- Übertragungen ----------
  const other = (p: Pane) => (p.side === "local" ? panes.remote : panes.local);
  const running = new Set<string>();

  function transfer(from: Pane, names: string[]) {
    const to = other(from);
    if (!sid || !to.listing || !from.listing) return;
    if (to.side === "local" && to.listing.path === "") return void (status.textContent = "Bitte zuerst links einen Ordner öffnen.");
    for (const name of names) start(from.side === "local", join(from, name), to.listing.path, name);
  }

  function start(upload: boolean, src: string, destDir: string, name: string) {
    const row = el("div", "transfer");
    const label = el("span", "transfer-name", `${upload ? "↑" : "↓"} ${name} → ${upload ? c.name : "Dieser Computer"}`);
    const bar = el("progress");
    bar.max = 1;
    bar.value = 0;
    bar.setAttribute("aria-label", `Fortschritt ${name}`);
    const state = el("span", "transfer-state", "Startet …");
    const cancel = el("button", "plain", "Abbrechen");
    cancel.type = "button";
    row.append(label, bar, state, cancel);
    transferList.prepend(row);
    let tid: string | null = null;
    const finish = (text: string) => {
      state.textContent = text;
      cancel.hidden = true;
      if (tid) running.delete(tid);
    };
    cancel.onclick = () => {
      if (tid) api.transferCancel(tid);
      finish("Abgebrochen");
    };
    api
      .transferStart(sid!, upload, src, destDir, (p: Progress) => {
        if (p.state === "running") {
          bar.value = p.total ? p.done / p.total : 0;
          state.textContent = `${formatSize(p.done)} von ${formatSize(p.total)}`;
        } else if (p.state === "done") {
          bar.value = 1;
          finish("Fertig");
          const to = upload ? panes.remote : panes.local;
          if (to.listing?.path === destDir) load(to, destDir);
        } else finish(`Fehler: ${p.error}`);
      })
      .then((id) => {
        tid = id;
        running.add(id);
      })
      .catch((e) => finish(`Fehler: ${e}`));
  }

  $("files-up").onclick = () => transfer(panes.local, [...panes.local.selected]);
  $("files-down").onclick = () => transfer(panes.remote, [...panes.remote.selected]);
  $("files-mkdir").onclick = async () => {
    const dir = panes.remote.listing?.path;
    if (!sid || !dir) return;
    const name = await askText("Neuer Ordner", `In ${dir} auf ${c.name}`, "Anlegen", "Neuer Ordner");
    if (!name) return;
    try {
      await api.sftpMkdir(sid, dir, name);
      await load(panes.remote, dir);
    } catch (e) {
      tell("Ordner nicht angelegt", String(e));
    }
  };

  async function renameRemote() {
    const p = panes.remote;
    const [name] = p.selected;
    if (!sid || !p.listing || p.selected.size !== 1) return void (status.textContent = "Bitte rechts genau ein Element wählen.");
    const next = await askText("Umbenennen", `Neuer Name für «${name}»`, "Umbenennen", name);
    if (!next || next === name) return;
    try {
      await api.sftpRename(sid, join(p, name), next);
      await load(p, p.listing.path);
    } catch (e) {
      tell("Nicht umbenannt", String(e));
    }
  }

  async function deleteRemote() {
    const p = panes.remote;
    const names = [...p.selected];
    if (!sid || !p.listing || !names.length) return void (status.textContent = "Bitte rechts auswählen, was gelöscht werden soll.");
    const dirs = names.filter((n) => p.listing!.entries.find((e) => e.name === n)?.dir).length;
    const what = names.length === 1 ? `«${names[0]}»` : `${names.length} Elemente`;
    const ok = await ask(
      `${what} löschen?`,
      `Wird auf ${c.name} endgültig gelöscht, ohne Papierkorb.${dirs ? " Ordner werden samt Inhalt gelöscht." : ""}`,
      "Löschen",
    );
    if (!ok) return;
    for (const n of names) {
      try {
        await api.sftpRemove(sid, join(p, n));
      } catch (e) {
        tell("Nicht gelöscht", String(e));
        break;
      }
    }
    await load(p, p.listing.path);
  }

  $("files-rename").onclick = renameRemote;
  $("files-delete").onclick = deleteRemote;

  // ---------- Ziehen zwischen den Seiten ----------
  // Eigene Zeiger-Logik: Tauri fängt HTML-Drag&Drop für Dateien aus dem System ab.
  let ghost: HTMLElement | null = null;
  function paneAt(x: number, y: number): Pane | undefined {
    const side = (document.elementFromPoint(x, y)?.closest(".pane") as HTMLElement | null)?.dataset.side as Side | undefined;
    return side && panes[side];
  }
  function startDrag(p: Pane, name: string, ev: PointerEvent) {
    if (ev.button !== 0) return;
    const [sx, sy] = [ev.clientX, ev.clientY];
    const target = ev.currentTarget as HTMLElement;
    const move = (m: PointerEvent) => {
      if (!ghost && Math.hypot(m.clientX - sx, m.clientY - sy) < 6) return;
      if (!ghost) {
        if (!p.selected.has(name)) select(p, name, false);
        target.setPointerCapture(m.pointerId);
        ghost = el("div", "drag-ghost", p.selected.size > 1 ? `${p.selected.size} Elemente` : name);
        document.body.append(ghost);
      }
      ghost.style.transform = `translate(${m.clientX + 12}px, ${m.clientY + 8}px)`;
      const over = paneAt(m.clientX, m.clientY);
      for (const q of Object.values(panes)) q.root.classList.toggle("drop", q === over && q !== p);
    };
    const up = (u: PointerEvent) => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      if (!ghost) return;
      ghost.remove();
      ghost = null;
      for (const q of Object.values(panes)) q.root.classList.remove("drop");
      if (paneAt(u.clientX, u.clientY) === other(p)) transfer(p, [...p.selected]);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
  }

  // Dateien aus Explorer/Finder auf die Server-Seite ziehen.
  const unlistenDrop = await getCurrentWebview().onDragDropEvent((ev) => {
    const t = ev.payload.type;
    const pos = "position" in ev.payload ? ev.payload.position : null;
    const over = pos ? paneAt(pos.x / devicePixelRatio, pos.y / devicePixelRatio) : undefined;
    panes.remote.root.classList.toggle("drop", t === "over" && over === panes.remote);
    if (t === "drop" && over === panes.remote && sid && panes.remote.listing) {
      for (const path of ev.payload.paths) {
        const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
        start(true, path, panes.remote.listing.path, name);
      }
    }
  });

  close = () => {
    unlistenDrop();
    for (const t of running) api.transferCancel(t);
    running.clear();
    if (sid) api.sftpClose(sid);
    sid = null;
    close = null;
  };
  $("session-back").onclick = () => {
    if (running.size) status.textContent = "Laufende Übertragungen wurden abgebrochen.";
    close?.();
    showView(null);
    onBack();
  };

  // ---------- Verbinden ----------
  load(panes.local, await api.localHome());
  let trust: string | null = null;
  for (;;) {
    try {
      sid = await api.sftpOpen(c.id, trust);
      status.textContent = "";
      await load(panes.remote, "");
    } catch (e) {
      const msg = String(e);
      if (trust === null && msg.startsWith(HOSTKEY_UNKNOWN)) {
        trust = await askHostKey(c.address, msg);
        if (trust) continue;
        status.textContent = "Verbindung abgebrochen: Server nicht bestätigt.";
      } else status.textContent = `Keine Verbindung: ${msg}`;
    }
    break;
  }
}
