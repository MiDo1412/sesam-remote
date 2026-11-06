import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, HOSTKEY_UNKNOWN, type Entry, type Listing, type Progress } from "./api";
import { ask, askHostKey, askText, tell } from "./ui";
import { openMenu, type MenuItem } from "./menu";
import { ICON } from "./icons";
import { handoffDone, toolButton, toolSep, type TabCtx, type TabHandle } from "./tabs";

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

type SortKey = "name" | "size" | "date";

/** Dateityp grob nach Endung, für das Symbol in der Liste. */
function fileKind(name: string, dir: boolean) {
  if (dir) return "dir";
  const ext = name.toLowerCase().split(".").pop() ?? "";
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "heic", "bmp"].includes(ext)) return "img";
  if (["zip", "gz", "tgz", "xz", "bz2", "7z", "rar", "zst", "tar"].includes(ext)) return "zip";
  if (["pdf"].includes(ext)) return "pdf";
  if (["sh", "py", "js", "ts", "rs", "go", "c", "h", "json", "yml", "yaml", "toml", "conf", "ini", "md", "txt", "log", "html", "css"].includes(ext)) return "txt";
  return "file";
}

interface Pane {
  side: Side;
  root: HTMLElement;
  body: HTMLElement;
  pathEl: HTMLElement;
  listing: Listing | null;
  selected: Set<string>;
  /** Pfad-Trenner der Seite. */
  sep: string;
  sort: SortKey;
  desc: boolean;
  hidden: boolean;
  filter: string;
}

/** Dateien (SFTP) als Tab. */
/** 0o755 → «rwxr-xr-x» (mit d für Ordner). */
function permText(m: number, dir: boolean) {
  const r = [6, 3, 0].map((s) => ["r", "w", "x"].map((ch, i) => ((m >> (s + 2 - i)) & 1 ? ch : "-")).join("")).join("");
  return (dir ? "d" : "-") + r;
}

export function filesTab(ctx: TabCtx): TabHandle {
  const c = ctx.conn;
  ctx.setSub(`SFTP über SSH · ${c.user}@${c.address}`);
  const status = ctx.status;
  status.textContent = "Verbinde …";

  const host = el("div", "files");
  ctx.pane.prepend(host);
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
  let scp = false;
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
    up.title = "Übergeordneter Ordner (Rücktaste)";
    up.setAttribute("aria-label", "Übergeordneter Ordner");
    const refresh = el("button", "plain", "↻");
    refresh.type = "button";
    refresh.title = "Neu laden";
    refresh.setAttribute("aria-label", "Neu laden");
    // Pfad: Brotkrumen, Klick darauf macht ein Eingabefeld daraus.
    const pathEl = el("span", "path crumbs");
    const marks = el("button", "plain", "★");
    marks.type = "button";
    marks.title = "Lesezeichen";
    marks.setAttribute("aria-label", "Lesezeichen");
    head.append(up, el("h2", "", title), pathEl, el("span", "grow"), marks, refresh);
    const tools = el("div", "pane-tools");
    const filter = el("input", "pane-filter") as HTMLInputElement;
    filter.type = "search";
    filter.placeholder = "Filtern";
    filter.setAttribute("aria-label", `${title} filtern`);
    const hidden = el("label", "pane-hidden");
    const hiddenBox = Object.assign(document.createElement("input"), { type: "checkbox" });
    hidden.append(hiddenBox, document.createTextNode(" Versteckte Dateien"));
    tools.append(filter, hidden);
    const table = el("table");
    const thead = el("thead");
    const tr = el("tr");
    const th = (key: SortKey, label: string, cls: string) => {
      const h = el("th", cls);
      const b = el("button", "sort", label) as HTMLButtonElement;
      b.type = "button";
      b.onclick = () => {
        pane.desc = pane.sort === key ? !pane.desc : false;
        pane.sort = key;
        render(pane);
      };
      h.dataset.key = key;
      h.append(b);
      return h;
    };
    tr.append(th("name", "Name", ""), th("size", "Grösse", "size"), th("date", "Geändert", "date"));
    thead.append(tr);
    const body = el("tbody");
    table.append(thead, body);
    const scroller = el("div", "pane-list");
    scroller.append(table);
    root.append(head, tools, scroller);
    const pane: Pane = { side, root, body, pathEl, listing: null, selected: new Set(), sep, sort: "name", desc: false, hidden: false, filter: "" };
    up.onclick = () => pane.listing?.parent != null && load(pane, pane.listing.parent);
    refresh.onclick = () => pane.listing && load(pane, pane.listing.path);
    filter.oninput = () => ((pane.filter = filter.value.toLowerCase()), render(pane));
    hiddenBox.onchange = () => ((pane.hidden = hiddenBox.checked), render(pane));
    marks.onclick = () => bookmarkMenu(pane, marks);
    pathEl.ondblclick = () => editPath(pane);
    scroller.oncontextmenu = (ev) => {
      ev.preventDefault();
      pane.selected.clear();
      render(pane);
      contextMenu(pane, scroller, null);
    };
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

  /** Pfad als Brotkrumen; Doppelklick = eintippen. */
  function renderCrumbs(p: Pane) {
    const path = p.listing!.path;
    p.pathEl.title = `${path}\nDoppelklick: Pfad eingeben`;
    if (!path) return void (p.pathEl.textContent = "Laufwerke");
    const parts = path.split(/[\\/]/).filter(Boolean);
    const lead = p.sep === "/" ? "/" : "";
    p.pathEl.replaceChildren();
    let acc = lead;
    if (lead) p.pathEl.append(crumb(p, "/", "/"));
    parts.forEach((part, i) => {
      acc = i === 0 && !lead ? `${part}${p.sep}` : `${acc}${acc.endsWith(p.sep) ? "" : p.sep}${part}`;
      if (i > 0 || lead) p.pathEl.append(el("span", "crumb-sep", "›"));
      p.pathEl.append(crumb(p, part, acc));
    });
  }
  function crumb(p: Pane, label: string, target: string) {
    const b = el("button", "crumb", label) as HTMLButtonElement;
    b.type = "button";
    b.onclick = () => load(p, target);
    return b;
  }
  async function editPath(p: Pane) {
    const v = await askText("Gehe zu", p.side === "local" ? "Ordner auf diesem Computer" : `Ordner auf ${c.name}`, "Öffnen", p.listing?.path ?? "");
    if (v) load(p, v);
  }

  // Lesezeichen pro Seite und Verbindung (nur Pfade, lokal gespeichert).
  const markKey = (p: Pane) => `sesam.marks.${p.side === "local" ? "local" : c.id}`;
  const marksOf = (p: Pane): string[] => JSON.parse(localStorage.getItem(markKey(p)) ?? "[]");
  async function bookmarkMenu(p: Pane, anchor: HTMLElement) {
    const list = marksOf(p);
    const here = p.listing?.path ?? "";
    const v = await openMenu(anchor, [
      ...list.map((m) => ({ value: `go:${m}`, label: m })),
      { value: "add", label: "Diesen Ordner merken", separator: list.length > 0, disabled: !here || list.includes(here) },
      { value: "del", label: "Diesen Ordner vergessen", disabled: !list.includes(here) },
    ]);
    if (!v) return;
    if (v.startsWith("go:")) load(p, v.slice(3));
    if (v === "add") localStorage.setItem(markKey(p), JSON.stringify([...list, here]));
    if (v === "del") localStorage.setItem(markKey(p), JSON.stringify(list.filter((m) => m !== here)));
  }

  function visible(p: Pane) {
    const l = p.listing!;
    const dir = p.desc ? -1 : 1;
    return l.entries
      .filter((e) => (p.hidden || !e.name.startsWith(".")) && (!p.filter || e.name.toLowerCase().includes(p.filter)))
      .sort((a, b) => {
        if (a.dir !== b.dir) return a.dir ? -1 : 1;
        const d = p.sort === "size" ? a.size - b.size : p.sort === "date" ? (a.modified ?? 0) - (b.modified ?? 0) : a.name.localeCompare(b.name, "de-CH", { numeric: true });
        return d * dir;
      });
  }

  function render(p: Pane) {
    const l = p.listing!;
    renderCrumbs(p);
    for (const h of p.root.querySelectorAll<HTMLElement>("th[data-key]")) {
      h.setAttribute("aria-sort", h.dataset.key === p.sort ? (p.desc ? "descending" : "ascending") : "none");
    }
    p.body.replaceChildren(
      ...visible(p).map((e) => {
        const tr = el("tr");
        tr.tabIndex = -1;
        tr.dataset.name = e.name;
        tr.setAttribute("aria-selected", String(p.selected.has(e.name)));
        const name = el("td", "name");
        name.append(el("span", `icon ${fileKind(e.name, e.dir)}`), document.createTextNode(e.name));
        tr.append(name, el("td", "size", e.dir ? "—" : formatSize(e.size)), el("td", "date", formatDate(e.modified)));
        tr.onclick = (ev) => select(p, e.name, ev.ctrlKey || ev.metaKey);
        tr.ondblclick = () => activate(p, e);
        tr.onkeydown = (ev) => {
          if (ev.key === "Enter") activate(p, e);
          if (ev.key === "F2") p.side === "remote" ? renameRemote() : renameLocal();
          if (p.side === "remote" && ev.key === "Delete") deleteRemote();
          if (ev.key === "Backspace" && l.parent != null) load(p, l.parent);
        };
        tr.onpointerdown = (ev) => startDrag(p, e.name, ev);
        tr.oncontextmenu = (ev) => {
          ev.preventDefault();
          ev.stopPropagation();
          if (!p.selected.has(e.name)) select(p, e.name, false);
          contextMenu(p, tr, e);
        };
        if (e.perm != null) tr.title = `${e.name}\nRechte: ${permText(e.perm, e.dir)} (${e.perm.toString(8).padStart(3, "0")})`;
        return tr;
      }),
    );
    if (!p.body.children.length) {
      const tr = el("tr");
      const td = el("td", "empty", l.entries.length ? "Keine Treffer" : "Leerer Ordner");
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
    let [lastAt, lastDone, speed] = [0, 0, 0];
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
          // Geschwindigkeit gleitend, Restzeit daraus
          const now = performance.now();
          if (lastAt) speed = speed * 0.7 + (((p.done - lastDone) / Math.max(1, now - lastAt)) * 1000) * 0.3;
          [lastAt, lastDone] = [now, p.done];
          const rest = speed > 0 ? Math.round((p.total - p.done) / speed) : 0;
          state.textContent = `${formatSize(p.done)} von ${formatSize(p.total)} · ${formatSize(speed)}/s${rest ? ` · noch ${rest < 60 ? `${rest} s` : `${Math.round(rest / 60)} min`}` : ""}`;
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

  const mkdir = async (p: Pane = panes.remote) => {
    const dir = p.listing?.path;
    if (!dir || (p.side === "remote" && !sid)) return;
    const name = await askText("Neuer Ordner", `In ${dir}${p.side === "remote" ? ` auf ${c.name}` : ""}`, "Anlegen", "Neuer Ordner");
    if (!name) return;
    try {
      await (p.side === "remote" ? api.sftpMkdir(sid!, dir, name) : api.localMkdir(dir, name));
      await load(p, dir);
    } catch (e) {
      tell("Ordner nicht angelegt", String(e));
    }
  };

  async function newFile() {
    const dir = panes.remote.listing?.path;
    if (!sid || !dir) return;
    const name = await askText("Neue Datei", `Leere Datei in ${dir} auf ${c.name}`, "Anlegen", "neu.txt");
    if (!name) return;
    try {
      await api.sftpTouch(sid, dir, name);
      await load(panes.remote, dir);
    } catch (e) {
      tell("Datei nicht angelegt", String(e));
    }
  }

  async function renameLocal() {
    const p = panes.local;
    const [name] = p.selected;
    if (!p.listing || p.selected.size !== 1) return void (status.textContent = "Bitte links genau ein Element wählen.");
    const next = await askText("Umbenennen", `Neuer Name für «${name}»`, "Umbenennen", name);
    if (!next || next === name) return;
    try {
      await api.localRename(join(p, name), next);
      await load(p, p.listing.path);
    } catch (e) {
      tell("Nicht umbenannt", String(e));
    }
  }

  async function chmod(e: Entry) {
    const p = panes.remote;
    if (!sid || !p.listing) return;
    const cur = (e.perm ?? (e.dir ? 0o755 : 0o644)).toString(8).padStart(3, "0");
    const v = await askText("Rechte ändern", `Für «${e.name}», als Zahl wie bei chmod: 644 = Datei lesbar, 755 = Ordner oder Programm, 600 = nur für dich.`, "Ändern", cur);
    if (!v || v === cur) return;
    if (!/^[0-7]{3,4}$/.test(v.trim())) return void tell("Ungültige Rechte", "Bitte drei oder vier Ziffern von 0 bis 7, z. B. 644.");
    try {
      await api.sftpChmod(sid, join(p, e.name), parseInt(v.trim(), 8));
      await load(p, p.listing.path);
    } catch (err) {
      tell("Rechte nicht geändert", String(err));
    }
  }

  /** Rechtsklick auf eine Zeile (e) oder auf die leere Fläche (e = null). */
  async function contextMenu(p: Pane, anchor: HTMLElement, e: Entry | null) {
    const remote = p.side === "remote";
    const one = p.selected.size === 1;
    const items: MenuItem[] = [];
    if (e) {
      items.push(
        { value: "transfer", label: remote ? "Herunterladen" : "Hochladen", icon: remote ? ICON.download : ICON.upload, disabled: !sid },
        { value: "rename", label: "Umbenennen …", icon: ICON.rename, shortcut: remote ? "F2" : undefined, disabled: !one || (remote && scp) },
      );
      if (remote) items.push({ value: "chmod", label: "Rechte ändern …", disabled: !one || scp });
      items.push({ value: "copy", label: "Pfad kopieren", icon: ICON.copy });
      if (remote) items.push({ value: "delete", label: "Löschen", icon: ICON.trash, shortcut: "Entf", disabled: scp, separator: true });
    }
    if (p.listing?.path) {
      items.push({ value: "mkdir", label: "Neuer Ordner …", icon: ICON.newFolder, separator: !!e, disabled: remote && !sid });
      if (remote) items.push({ value: "touch", label: "Neue Datei …", icon: ICON.plus, disabled: !sid || scp });
    }
    if (!items.length) return;
    const v = await openMenu(anchor, items);
    if (v === "transfer") transfer(p, [...p.selected]);
    if (v === "rename") remote ? renameRemote() : renameLocal();
    if (v === "chmod" && e) chmod(e);
    if (v === "copy") navigator.clipboard.writeText([...p.selected].map((n) => join(p, n)).join("\n")).catch(() => {});
    if (v === "delete") deleteRemote();
    if (v === "mkdir") mkdir(p);
    if (v === "touch") newFile();
  }

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

  const renameBtn = toolButton(ICON.rename, "Umbenennen", () => renameRemote(), { shortcut: "F2" });
  const trashBtn = toolButton(ICON.trash, "Löschen", () => deleteRemote(), { shortcut: "Entf", danger: true });
  ctx.toolbar.append(
    toolButton(ICON.upload, "Hochladen", () => transfer(panes.local, [...panes.local.selected]), { text: true }),
    toolButton(ICON.download, "Herunterladen", () => transfer(panes.remote, [...panes.remote.selected]), { text: true }),
    toolSep(),
    toolButton(ICON.newFolder, "Neuer Ordner", () => mkdir()),
    toolButton(ICON.plus, "Neue Datei", () => newFile()),
    renameBtn,
    trashBtn,
    toolButton(ICON.refresh, "Neu laden", () => {
      for (const p of Object.values(panes)) if (p.listing) load(p, p.listing.path);
    }),
    toolSep(),
    toolButton(ICON.disconnect, "Trennen", () => ctx.closeSelf(), { text: true }),
  );

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

  // Dateien aus Explorer/Finder auf die Server-Seite ziehen (nur im sichtbaren Tab).
  const unlistenDrop = getCurrentWebview().onDragDropEvent((ev) => {
    if (!ctx.isActive()) return;
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

  const close = () => {
    unlistenDrop.then((u) => u());
    for (const t of running) api.transferCancel(t);
    running.clear();
    if (sid) api.sftpClose(sid);
    sid = null;
  };

  // ---------- Verbinden ----------
  void (async () => {
  load(panes.local, await api.localHome());
  let trust: string | null = null;
  for (;;) {
    try {
      const at = ctx.extra?.attach;
      const st = at?.state as { scp: boolean; remote: string; local: string } | undefined;
      const opened = at ? { sid: at.sid, scp: !!st?.scp } : await api.sftpOpen(c.id, trust);
      if (at) {
        handoffDone(at.token);
        if (st?.local) load(panes.local, st.local);
      }
      sid = opened.sid;
      status.textContent = "";
      if (opened.scp) {
        ctx.setSub(`SCP über SSH · ${c.user}@${c.address}`);
        status.textContent = "Server ohne SFTP, Übertragung über SCP (eingeschränkt: kein Umbenennen/Löschen/Rechte)";
        renameBtn.disabled = trashBtn.disabled = true;
        scp = true;
      }
      ctx.setState("connected");
      await load(panes.remote, st?.remote ?? "");
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
    close,
    handoff: () => (sid ? { sid, state: { scp, remote: panes.remote.listing?.path ?? "", local: panes.local.listing?.path ?? "" } } : null),
    release: () => {
      sid = null; // SFTP-Sitzung läuft im anderen Fenster weiter
      close();
    },
    busy: () => (running.size ? `${running.size} Übertragung(en) laufen noch und werden abgebrochen.` : null),
  };
}
