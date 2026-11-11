import { dropFiles } from "./dropfiles";
import { askConflict, type Choice } from "./conflict";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, CERT_CHANGED, CERT_UNKNOWN, filesProtocol, ftpLabel, HOSTKEY_UNKNOWN, type Entry, type Listing, type Progress } from "./api";
import { ask, askHostKey, askPassword, askText, choose, tell } from "./ui";
import { compareDirs, type Mark } from "./compare";
import { openMenu, type MenuItem } from "./menu";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { isMobile } from "./titlebar";
import { ICON } from "./icons";
import { handoffDone, toolButton, toolSep, type TabCtx, type TabHandle } from "./tabs";
import { localNetworkText } from "./ubuntu";

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
  /** Ausgangspunkt für Umschalt-Klick und die Zeile mit dem Tastatur-Fokus. */
  anchor: string | null;
  cursor: string | null;
  /** Pfad-Trenner der Seite. */
  sep: string;
  sort: SortKey;
  desc: boolean;
  hidden: boolean;
  filter: string;
  /** Verlauf für Zurück/Vorwärts. */
  back: string[];
  fwd: string[];
  nav: { back: HTMLButtonElement; fwd: HTMLButtonElement; up: HTMLButtonElement };
}

/** Dateien (SFTP) als Tab. */
/** 0o755 → «rwxr-xr-x» (mit d für Ordner). */
function permText(m: number, dir: boolean) {
  const r = [6, 3, 0].map((s) => ["r", "w", "x"].map((ch, i) => ((m >> (s + 2 - i)) & 1 ? ch : "-")).join("")).join("");
  return (dir ? "d" : "-") + r;
}

export function filesTab(ctx: TabCtx): TabHandle {
  const c = ctx.conn;
  const ftp = filesProtocol(c) === "ftp";
  ctx.setSub(ftp ? `${ftpLabel(c)} · ${c.user || "anonymous"}@${c.address}:${c.ftpPort}` : `SFTP über SSH · ${c.user}@${c.address}`);
  const status = ctx.status;
  status.textContent = "Verbinde …";

  const host = el("div", "files");
  ctx.pane.prepend(host);
  const isWin = navigator.userAgent.includes("Windows");
// iPad: links nur der Ordner der App («Auf meinem iPad › Sesam Remote» in der Dateien-App).
const localName = isMobile ? "Auf meinem iPad" : "Dieser Computer";
/** Apple-Tastenkürzel (⌘) auf Mac und iPad mit Tastatur. */
const isMacUi = navigator.userAgent.includes("Mac");
let localRoot = "";
  const panes: Record<Side, Pane> = {
    local: makePane("local", localName, isWin ? "\\" : "/"),
    remote: makePane("remote", c.name, "/"),
  };
  const transfers = el("section", "transfers");
  transfers.setAttribute("aria-label", "Übertragungen");
  const transferHead = el("div", "transfers-head");
  transferHead.append(el("h2", "", "Übertragungen"), el("span", "sub", isMobile ? "Zwischen den Seiten ziehen oder doppeltippen; «Hochladen» ohne Auswahl öffnet die Dateiauswahl. Heruntergeladenes liegt in der App «Dateien» unter «Auf meinem iPad › Sesam Remote»." : "Zwischen den Seiten ziehen, Dateien aus dem Explorer/Finder rechts ablegen, oder doppelklicken."));
  const transferList = el("div", "transfer-list");
  transfers.append(transferHead, transferList);
  const grid = el("div", "panes");
  grid.append(panes.local.root, panes.remote.root);
  // Beim Vergleich scrollen beide Listen gemeinsam.
  const lists = [panes.local.root, panes.remote.root].map((r) => r.querySelector<HTMLElement>(".pane-list")!);
  let syncing = false;
  lists.forEach((l, i) =>
    l.addEventListener("scroll", () => {
      if (!comparing || syncing) return;
      syncing = true;
      lists[1 - i].scrollTop = l.scrollTop;
      requestAnimationFrame(() => (syncing = false));
    }),
  );

  // Vergleich der beiden offenen Ordner (gleiche Namen).
  let comparing = false;
  type CmpFilter = "all" | "diff" | "left" | "right";
  let cmpFilter: CmpFilter = "all";
  /** Ergebnis von «Inhalt prüfen»: Schlüssel aus Pfaden, Grössen und Zeiten → gleicher Inhalt. */
  const hashes = new Map<string, boolean>();
  const bar = el("div", "compare-bar");
  bar.hidden = true;
  const summary = el("span", "compare-summary");
  summary.setAttribute("aria-live", "polite");
  const diffLabel = el("div", "segmented compare-filter");
  diffLabel.setAttribute("role", "radiogroup");
  diffLabel.setAttribute("aria-label", "Anzeigen");
  const FILTERS: [CmpFilter, string][] = [["all", "Alle"], ["diff", "Unterschiede"], ["left", "Nur links"], ["right", "Nur rechts"]];
  const radios = FILTERS.map(([v, t], i) => {
    const l = el("label");
    const r = Object.assign(document.createElement("input"), { type: "radio", name: `cmpFilter-${Math.random().toString(36).slice(2)}`, checked: i === 0 });
    r.onchange = () => ((cmpFilter = v), renderAll());
    l.append(r, document.createTextNode(t));
    return l;
  });
  for (const r of radios) r.querySelector("input")!.name = radios[0].querySelector("input")!.name;
  diffLabel.append(...radios);
  const barButton = (label: string, onClick: () => void, cls = "") => {
    const b = el("button", cls, label) as HTMLButtonElement;
    b.type = "button";
    b.onclick = onClick;
    return b;
  };
  const checkBtn = barButton("Inhalt prüfen", () => checkContent());
  checkBtn.title = "Gleichnamige Dateien per SHA-256 vergleichen (auf dem Server mit sha256sum)";
  const syncBtn = barButton("Synchronisieren …", () => void syncDialog(), "primary");
  bar.append(summary, el("span", "grow"), diffLabel, checkBtn, syncBtn, barButton("Beenden", () => toggleCompare(), "plain"));
  host.append(bar, grid, transfers);

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
    const navButton = (icon: string, label: string, hint: string) => {
      const b = el("button", "plain nav") as HTMLButtonElement;
      b.type = "button";
      b.innerHTML = icon;
      b.title = `${label} (${hint})`;
      b.setAttribute("aria-label", label);
      return b;
    };
    const back = navButton(ICON.back, "Zurück", "Alt+Links");
    const fwd = navButton(ICON.forward, "Vorwärts", "Alt+Rechts");
    const up = navButton(ICON.up, "Übergeordneter Ordner", "Alt+Hoch oder Rücktaste");
    const home = navButton(ICON.home, side === "local" ? "Benutzerordner" : "Startordner", side === "local" ? "Home" : "Ordner nach dem Anmelden");
    const refresh = navButton(ICON.refresh, "Neu laden", "F5");
    // Pfad: Brotkrumen; Klick daneben oder auf den Stift macht ein Eingabefeld daraus.
    const pathEl = el("span", "path crumbs");
    const edit = navButton(ICON.rename, "Pfad eingeben", "oder Klick neben den Pfad");
    const marks = el("button", "plain", "★");
    marks.type = "button";
    marks.title = "Lesezeichen";
    marks.setAttribute("aria-label", "Lesezeichen");
    head.append(back, fwd, up, home, el("h2", "", title), pathEl, edit, marks, refresh);
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
        // Beim Vergleich gilt die Sortierung für beide Seiten (Zeilen auf gleicher Höhe).
        const t = comparing ? panes.local : pane;
        t.desc = t.sort === key ? !t.desc : false;
        t.sort = key;
        if (comparing) renderAll();
        else render(pane);
      };
      h.dataset.key = key;
      h.append(b);
      return h;
    };
    tr.append(th("name", "Name", ""), th("size", "Grösse", "size"), th("date", "Geändert", "date"));
    // Mittelspalte des Vergleichs (rechter Rand der linken Liste)
    if (side === "local") {
      const st = el("th", "st", "");
      st.setAttribute("aria-label", "Vergleich");
      tr.append(st);
    }
    thead.append(tr);
    const body = el("tbody");
    table.append(thead, body);
    const scroller = el("div", "pane-list");
    scroller.append(table);
    root.append(head, tools, scroller);
    const pane: Pane = { side, root, body, pathEl, listing: null, selected: new Set(), anchor: null, cursor: null, sep, sort: "name", desc: false, hidden: false, filter: "", back: [], fwd: [], nav: { back, fwd, up } };
    const goUp = () => pane.listing?.parent != null && load(pane, pane.listing.parent, "go");
    const reload = () => pane.listing && load(pane, pane.listing.path);
    up.onclick = goUp;
    refresh.onclick = reload;
    back.onclick = () => step(pane, "back");
    fwd.onclick = () => step(pane, "fwd");
    // Server: "" ist der Ordner nach dem Anmelden.
    home.onclick = async () => load(pane, side === "local" ? await api.localHome() : "", "go");
    edit.onclick = () => editPath(pane);
    // Tastatur, solange die Seite den Fokus hat.
    root.tabIndex = -1;
    root.onkeydown = (ev) => {
      const typing = (ev.target as HTMLElement).matches("input, textarea");
      if (!typing && listKey(pane, ev)) return ev.preventDefault();
      const mod = isMacUi ? ev.metaKey : ev.altKey;
      const act =
        ev.altKey && ev.key === "ArrowLeft" ? () => step(pane, "back")
        : ev.altKey && ev.key === "ArrowRight" ? () => step(pane, "fwd")
        : (mod && ev.key === "ArrowUp") || (ev.key === "Backspace" && !typing && !ev.altKey && !ev.metaKey) ? goUp
        : ev.key === "F5" ? reload
        : null;
      if (!act) return;
      ev.preventDefault();
      act();
    };
    // Gummiband: auf freier Fläche ziehen wählt die überstrichenen Zeilen (mit ⌘/Strg dazu).
    scroller.addEventListener("pointerdown", (ev) => {
      if (ev.button !== 0 || ev.pointerType === "touch" || (ev.target as HTMLElement).closest("tr[data-name], thead, button, input")) return;
      root.focus({ preventScroll: true });
      const add = ev.ctrlKey || ev.metaKey;
      const base = add ? new Set(pane.selected) : new Set<string>();
      if (!add) setSelection(pane, base);
      const box = scroller.getBoundingClientRect();
      const [sx, sy] = [ev.clientX, ev.clientY + scroller.scrollTop];
      let band: HTMLElement | null = null;
      const move = (m: PointerEvent) => {
        const [x0, x1] = [Math.min(sx, m.clientX), Math.max(sx, m.clientX)];
        const [y0, y1] = [Math.min(sy, m.clientY + scroller.scrollTop), Math.max(sy, m.clientY + scroller.scrollTop)];
        if (!band && y1 - y0 < 4 && x1 - x0 < 4) return;
        if (!band) scroller.append((band = el("div", "rubber-band")));
        Object.assign(band.style, { left: `${x0 - box.left}px`, top: `${y0 - box.top}px`, width: `${x1 - x0}px`, height: `${y1 - y0}px` });
        const hit = new Set(base);
        for (const tr of pane.body.querySelectorAll<HTMLElement>("tr[data-name]")) {
          const r = tr.getBoundingClientRect();
          if (r.bottom + scroller.scrollTop > y0 && r.top + scroller.scrollTop < y1) hit.add(tr.dataset.name!);
        }
        setSelection(pane, hit);
      };
      const up = () => {
        scroller.removeEventListener("pointermove", move);
        scroller.removeEventListener("pointerup", up);
        band?.remove();
      };
      try {
        scroller.setPointerCapture(ev.pointerId);
      } catch {}
      scroller.addEventListener("pointermove", move);
      scroller.addEventListener("pointerup", up);
    });
    const redraw = () => (comparing ? renderAll() : render(pane));
    filter.oninput = () => ((pane.filter = filter.value.toLowerCase()), redraw());
    hiddenBox.onchange = () => ((pane.hidden = hiddenBox.checked), redraw());
    marks.onclick = () => bookmarkMenu(pane, marks);
    pathEl.onclick = (ev) => ev.target === pathEl && editPath(pane);
    scroller.oncontextmenu = (ev) => {
      ev.preventDefault();
      pane.selected.clear();
      render(pane);
      contextMenu(pane, scroller, null);
    };
    return pane;
  }

  /** Ordner laden; `nav` führt den Verlauf (go = neuer Schritt, back/fwd = im Verlauf). */
  async function load(p: Pane, path: string, nav: "go" | "back" | "fwd" | null = null) {
    try {
      const l = p.side === "local" ? await api.localList(path) : await api.sftpList(sid!, path);
      const cur = p.listing?.path;
      if (nav === "go" && cur != null && cur !== l.path) {
        p.back.push(cur);
        p.fwd.length = 0;
      } else if (nav === "back" || nav === "fwd") {
        const [from, to] = nav === "back" ? [p.back, p.fwd] : [p.fwd, p.back];
        from.pop();
        if (cur != null) to.push(cur);
      }
      if (cur !== l.path) (expanded.clear(), subLists.clear());
      p.listing = l;
      p.selected.clear();
      p.anchor = p.cursor = null;
      if (comparing) renderAll();
      else render(p);
    } catch (e) {
      status.textContent = String(e);
      // SSH-Verbindung weg (Schlaf, Netzwechsel): Tab bietet «Neu verbinden» an.
      if (p.side === "remote" && /closed|geschlossen|reset|broken pipe|eof|timeout|timed out|not connected|getrennt|beendet/i.test(String(e))) ctx.setState("closed");
    }
  }
  function step(p: Pane, dir: "back" | "fwd") {
    const list = dir === "back" ? p.back : p.fwd;
    const target = list[list.length - 1];
    if (target != null) load(p, target, dir);
  }

  /** Pfad als Brotkrumen; Doppelklick = eintippen. */
  function renderCrumbs(p: Pane) {
    const path = p.listing!.path;
    p.pathEl.title = `${path || "Laufwerke"}\nKlick neben den Pfad: Pfad eingeben`;
    p.pathEl.replaceChildren();
    // Unter Windows führt der erste Teil zur Laufwerksliste.
    const drives = p.side === "local" && isWin;
    if (drives) p.pathEl.append(crumb(p, "Laufwerke", ""));
    if (!path) return;
    // iPad: Pfad ab dem Ordner der App, ohne den Container davor.
    if (p.side === "local" && isMobile && localRoot && path.startsWith(localRoot)) {
      p.pathEl.append(crumb(p, "Sesam Remote", localRoot));
      let acc = localRoot;
      for (const part of path.slice(localRoot.length).split("/").filter(Boolean)) {
        acc = `${acc}/${part}`;
        p.pathEl.append(el("span", "crumb-sep", "›"), crumb(p, part, acc));
      }
      return;
    }
    const parts = path.split(/[\\/]/).filter(Boolean);
    const lead = p.sep === "/" ? "/" : "";
    let acc = lead;
    if (lead) p.pathEl.append(crumb(p, "/", "/"));
    parts.forEach((part, i) => {
      acc = i === 0 && !lead ? `${part}${p.sep}` : `${acc}${acc.endsWith(p.sep) ? "" : p.sep}${part}`;
      if (i > 0 || lead || drives) p.pathEl.append(el("span", "crumb-sep", "›"));
      p.pathEl.append(crumb(p, part, acc));
    });
    requestAnimationFrame(() => collapseCrumbs(p));
  }
  /** Zu lang: Anfang und aktueller Ordner bleiben, die Mitte wird zu «…» (Klick zeigt die versteckten Ordner). */
  function collapseCrumbs(p: Pane) {
    const el_ = p.pathEl;
    const crumbs = [...el_.querySelectorAll<HTMLButtonElement>(".crumb")];
    if (el_.scrollWidth <= el_.clientWidth + 1 || crumbs.length < 3) return;
    const more = el("button", "crumb crumb-more", "…") as HTMLButtonElement;
    more.type = "button";
    more.title = "Weitere Ordner";
    crumbs[0].after(el("span", "crumb-sep", "›"), more);
    const hidden: HTMLButtonElement[] = [];
    for (let i = 1; i < crumbs.length - 1 && el_.scrollWidth > el_.clientWidth + 1; i++) {
      crumbs[i].hidden = true;
      (crumbs[i].previousElementSibling as HTMLElement | null)?.classList.contains("crumb-sep") && ((crumbs[i].previousElementSibling as HTMLElement).hidden = true);
      hidden.push(crumbs[i]);
    }
    more.onclick = async () => {
      const v = await openMenu(more, hidden.map((b, i) => ({ value: String(i), label: b.textContent ?? "", icon: ICON.folder })));
      if (v !== null) hidden[Number(v)].click();
    };
  }
  function crumb(p: Pane, label: string, target: string) {
    const b = el("button", "crumb", label) as HTMLButtonElement;
    b.type = "button";
    b.onclick = () => load(p, target, "go");
    return b;
  }
  async function editPath(p: Pane) {
    const v = await askText("Gehe zu", p.side === "local" ? "Ordner auf diesem Computer" : `Ordner auf ${c.name}`, "Öffnen", p.listing?.path ?? "");
    if (v) load(p, v, "go");
  }

  // Lesezeichen pro Seite und Verbindung (nur Pfade, lokal gespeichert).
  const markKey = (p: Pane) => `sesam.marks.${p.side === "local" ? "local" : c.id}`;
  const marksOf = (p: Pane): string[] => JSON.parse(localStorage.getItem(markKey(p)) ?? "[]");
  async function bookmarkMenu(p: Pane, anchor: HTMLElement) {
    const list = marksOf(p);
    const here = p.listing?.path ?? "";
    const v = await openMenu(anchor, [
      ...list.map((m) => ({ value: `go:${m}`, label: m, icon: ICON.folder })),
      { value: "add", label: "Diesen Ordner merken", icon: ICON.plus, separator: list.length > 0, disabled: !here || list.includes(here) },
      { value: "del", label: "Diesen Ordner vergessen", icon: ICON.trash, disabled: !list.includes(here) },
    ]);
    if (!v) return;
    if (v.startsWith("go:")) load(p, v.slice(3), "go");
    if (v === "add") localStorage.setItem(markKey(p), JSON.stringify([...list, here]));
    if (v === "del") localStorage.setItem(markKey(p), JSON.stringify(list.filter((m) => m !== here)));
  }

  function visible(p: Pane) {
    const dir = p.desc ? -1 : 1;
    return p.listing!.entries
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
      const s = comparing ? panes.local : p;
      h.setAttribute("aria-sort", h.dataset.key === s.sort ? (s.desc ? "descending" : "ascending") : "none");
    }
    p.nav.back.disabled = !p.back.length;
    p.nav.fwd.disabled = !p.fwd.length;
    p.nav.up.disabled = l.parent == null;
    if (cmpOn()) return renderCompare(p, cmpRows(cmpFilter));
    p.body.replaceChildren(...visible(p).map((e) => entryRow(p, e, e.name)));
    if (!p.body.children.length) emptyRow(p, !p.listing!.entries.length ? "Leerer Ordner" : p.filter ? "Keine Treffer" : "Nur versteckte Dateien – «Versteckte Dateien» einschalten");
  }

  function emptyRow(p: Pane, why: string) {
    const tr = el("tr");
    const td = el("td", "empty", why);
    td.colSpan = 4;
    tr.append(td);
    p.body.append(tr);
  }

  /** Zeile eines vorhandenen Eintrags; `name` ist der Pfad relativ zum offenen Ordner (im Vergleich auch in Unterordnern). */
  function entryRow(p: Pane, e: Entry, name: string, lead?: HTMLElement) {
    const tr = el("tr");
    tr.tabIndex = -1;
    tr.dataset.name = name;
    tr.setAttribute("aria-selected", String(p.selected.has(name)));
    tr.classList.toggle("cursor", p.cursor === name);
    const td = el("td", "name");
    if (lead) td.append(lead);
    td.append(el("span", `icon ${fileKind(e.name, e.dir)}`), el("span", "fname", e.name));
    tr.append(td, el("td", "size", e.dir ? "—" : formatSize(e.size)), el("td", "date", formatDate(e.modified)));
    const at = { ...e, name };
    tr.onclick = (ev) => {
      select(p, name, ev.ctrlKey || ev.metaKey, ev.shiftKey);
      p.root.focus({ preventScroll: true });
    };
    tr.ondblclick = () => activate(p, at);
    tr.onpointerdown = (ev) => startDrag(p, name, ev);
    tr.oncontextmenu = (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      if (!p.selected.has(name)) select(p, name, false);
      contextMenu(p, tr, at);
    };
    if (e.perm != null) tr.title = `${e.name}
Rechte: ${permText(e.perm, e.dir)} (${e.perm.toString(8).padStart(3, "0")})`;
    return tr;
  }

  /** Klick: nur dieses; ⌘/Strg: dazu oder weg; Umschalt: Bereich ab dem Ausgangspunkt (mit ⌘/Strg dazu). */
  function select(p: Pane, name: string, toggle: boolean, range = false) {
    const rows = rowNames(p);
    if (range && p.anchor && rows.includes(p.anchor)) {
      const [a, b] = [rows.indexOf(p.anchor), rows.indexOf(name)].sort((x, y) => x - y);
      const next = toggle ? new Set(p.selected) : new Set<string>();
      for (const n of rows.slice(a, b + 1)) next.add(n);
      p.cursor = name;
      return setSelection(p, next);
    }
    const next = toggle ? new Set(p.selected) : new Set<string>();
    if (toggle && next.has(name)) next.delete(name);
    else next.add(name);
    p.anchor = p.cursor = name;
    setSelection(p, next);
  }
  const rowNames = (p: Pane) => [...p.body.querySelectorAll<HTMLElement>("tr[data-name]")].map((tr) => tr.dataset.name!);
  function setSelection(p: Pane, names: Set<string>) {
    p.selected = names;
    for (const tr of p.body.querySelectorAll<HTMLElement>("tr[data-name]")) {
      tr.setAttribute("aria-selected", String(names.has(tr.dataset.name!)));
      tr.classList.toggle("cursor", tr.dataset.name === p.cursor);
    }
    p.body.querySelector<HTMLElement>("tr.cursor")?.scrollIntoView({ block: "nearest" });
  }

  /** Tastatur in der Liste wie im Finder bzw. Explorer. `true` = erledigt. */
  let typed = "";
  let typedAt = 0;
  function listKey(p: Pane, ev: KeyboardEvent): boolean {
    const rows = rowNames(p);
    const cmd = isMacUi ? ev.metaKey : ev.ctrlKey;
    const entry = (n: string | null) => (n == null ? null : (p.listing?.entries.find((e) => e.name === n) ?? null));
    if (cmd && ev.key.toLowerCase() === "a") return setSelection(p, new Set(rows)), true;
    if (ev.key === "ArrowDown" || ev.key === "ArrowUp" || ev.key === "Home" || ev.key === "End") {
      if (isMacUi && ev.metaKey && ev.key === "ArrowDown") {
        const e = entry(p.cursor);
        if (e) activate(p, e);
        return true;
      }
      if (ev.altKey || ev.metaKey) return false;
      if (!rows.length) return true;
      const i = p.cursor ? rows.indexOf(p.cursor) : -1;
      const j = ev.key === "Home" ? 0 : ev.key === "End" ? rows.length - 1 : Math.max(0, Math.min(rows.length - 1, i + (ev.key === "ArrowDown" ? 1 : -1)));
      if (ev.shiftKey) select(p, rows[j], false, true);
      else select(p, rows[j], false);
      return true;
    }
    // Mac wie Finder: Enter benennt um, ⌘↓ öffnet. Windows/Linux wie Explorer: Enter öffnet, F2 benennt um.
    if (ev.key === "Enter" && !ev.metaKey && !ev.ctrlKey) {
      const e = entry(p.cursor ?? [...p.selected][0] ?? null);
      if (isMacUi && p.selected.size === 1) p.side === "remote" ? renameRemote() : renameLocal();
      else if (e) activate(p, e);
      return true;
    }
    if (ev.key === "F2") return p.side === "remote" ? renameRemote() : renameLocal(), true;
    if (p.side === "remote" && (ev.key === "Delete" || (isMacUi && ev.metaKey && ev.key === "Backspace"))) return deleteRemote(), true;
    if (ev.key === "Escape" && p.selected.size) return setSelection(p, new Set()), true;
    // Tippen springt zum ersten passenden Namen.
    if (ev.key.length === 1 && !ev.metaKey && !ev.ctrlKey && !ev.altKey && ev.key !== " ") {
      const now = performance.now();
      typed = now - typedAt > 800 ? ev.key.toLowerCase() : typed + ev.key.toLowerCase();
      typedAt = now;
      const hit = rows.find((n) => n.toLowerCase().startsWith(typed));
      if (hit) select(p, hit, false);
      return true;
    }
    return false;
  }

  function activate(p: Pane, e: Entry) {
    if (e.dir) load(p, join(p, e.name), "go");
    else transfer(p, [e.name]);
  }

  // ---------- Vergleichen ----------
  /** Eine Zeile des Vergleichs: derselbe relative Pfad links und rechts (rel mit «/»). */
  /** `lv`/`rv`: auf dieser Seite angezeigt (Filter und «Versteckte Dateien» gelten je Seite). */
  interface Row { rel: string; depth: number; l?: Entry; r?: Entry; mark?: Mark; lv?: boolean; rv?: boolean }
  /** Richtung aus Sicht der Mittelspalte. */
  type State = "toRight" | "toLeft" | "same" | "differ" | "unknown";
  /** Aufgeklappte Ordner (auf beiden Seiten vorhanden) und ihre Inhalte links/rechts. */
  const expanded = new Set<string>();
  const subLists = new Map<string, { l: Entry[]; r: Entry[] }>();
  const cmpOn = () => comparing && !!panes.local.listing?.path && !!panes.remote.listing;
  const sidePath = (p: Pane, rel: string) => join(p, rel.split("/").join(p.sep));
  const bothDirs = (r: Row) => !!(r.l?.dir && r.r?.dir);
  const hashKey = (rel: string, a: Entry, b: Entry) => [panes.local.listing!.path, panes.remote.listing!.path, rel, a.size, a.modified, b.size, b.modified].join("\n");

  function rowState(r: Row): State {
    if (r.l && !r.r) return "toRight";
    if (r.r && !r.l) return "toLeft";
    if (r.mark === "newer") return "toRight";
    if (r.mark === "older") return "toLeft";
    if (r.mark === "same") return "same";
    if (r.mark === "differ") return "differ";
    return "unknown"; // Ordner auf beiden Seiten, noch nicht aufgeklappt
  }

  /** Zeilen beider Seiten in gemeinsamer Reihenfolge (Sortierung der linken Seite); aufgeklappte Ordner rekursiv. */
  function cmpRows(filter: CmpFilter): Row[] {
    const [L, R] = [panes.local.listing!, panes.remote.listing!];
    const s = panes.local;
    const dir = s.desc ? -1 : 1;
    const key = (e: Entry) => (s.sort === "size" ? e.size : s.sort === "date" ? (e.modified ?? 0) : 0);
    const pass = (p: Pane, e: Entry | undefined, depth: number) =>
      !!e && (p.hidden || !e.name.startsWith(".")) && (depth > 0 || !p.filter || e.name.toLowerCase().includes(p.filter));
    const level = (le: Entry[], re: Entry[], prefix: string, depth: number): { rows: Row[]; agg: Mark | undefined } => {
      const marks = compareDirs(le, re, 2, (n) => {
        const [a, b] = [le.find((e) => e.name === n), re.find((e) => e.name === n)];
        return a && b ? hashes.get(hashKey(prefix + n, a, b)) : undefined;
      });
      const lm = new Map(le.map((e) => [e.name, e]));
      const rm = new Map(re.map((e) => [e.name, e]));
      const items: Row[] = [...new Set([...lm.keys(), ...rm.keys()])].map((n) => ({ rel: prefix + n, depth, l: lm.get(n), r: rm.get(n), mark: marks.get(n) }));
      items.sort((a, b) => {
        const [x, y] = [(a.l ?? a.r)!, (b.l ?? b.r)!];
        if (x.dir !== y.dir) return x.dir ? -1 : 1;
        const d = s.sort === "name" ? x.name.localeCompare(y.name, "de-CH", { numeric: true }) : key(x) - key(y);
        return d * dir || x.name.localeCompare(y.name);
      });
      const rows: Row[] = [];
      let agg: Mark | undefined = "same";
      for (const it of items) {
        let kids: Row[] = [];
        const sub = bothDirs(it) ? subLists.get(it.rel) : undefined;
        if (sub) {
          const k = level(sub.l, sub.r, it.rel + "/", depth + 1);
          it.mark = k.agg;
          if (expanded.has(it.rel)) kids = k.rows;
        }
        const st = rowState(it);
        // Versteckte Dateien zählen nur mit, wenn sie auf ihrer Seite angezeigt werden.
        it.lv = pass(panes.local, it.l, depth);
        it.rv = pass(panes.remote, it.r, depth);
        if (!it.lv && !it.rv) continue;
        if (st === "unknown") agg = agg === "differ" ? "differ" : undefined;
        else if (st !== "same") agg = "differ";
        const keep =
          filter === "all" ||
          (filter === "diff" && st !== "same") ||
          (filter === "left" && (st === "toRight" && !it.r)) ||
          (filter === "right" && (st === "toLeft" && !it.l)) ||
          (bothDirs(it) && expanded.has(it.rel) && kids.length > 0);
        if (keep) rows.push(it, ...kids);
      }
      return { rows, agg };
    };
    return level(L.entries, R.entries, "", 0).rows;
  }

  const SYMBOL: Record<State, string> = { toRight: "→", toLeft: "←", same: "=", differ: "≠", unknown: "" };
  /** Färbung aus Sicht einer Seite (gleiche Klassen wie die Markierungen). */
  function tint(r: Row, left: boolean): Mark | null {
    const st = rowState(r);
    if (st === "same") return "same";
    if (st === "differ") return "differ";
    if (st === "unknown") return null;
    if (left ? !r.r : !r.l) return "only";
    return (st === "toRight") === left ? "newer" : "older";
  }

  function renderCompare(p: Pane, rows: Row[]) {
    const left = p.side === "local";
    p.body.replaceChildren(
      ...rows.map((row) => {
        const e = left ? (row.lv ? row.l : undefined) : row.rv ? row.r : undefined;
        const twisty = el("button", "twisty", bothDirs(row) ? (expanded.has(row.rel) ? "▾" : "▸") : "") as HTMLButtonElement;
        twisty.type = "button";
        twisty.tabIndex = -1;
        if (bothDirs(row)) {
          twisty.title = expanded.has(row.rel) ? "Zuklappen" : "Aufklappen und vergleichen";
          twisty.setAttribute("aria-expanded", String(expanded.has(row.rel)));
          twisty.onclick = (ev) => (ev.stopPropagation(), void toggleExpand(row.rel));
          twisty.onpointerdown = (ev) => ev.stopPropagation();
        } else twisty.disabled = true;
        twisty.style.marginLeft = `${row.depth * 16}px`;
        let tr: HTMLElement;
        if (e) {
          tr = entryRow(p, e, row.rel.split("/").join(p.sep), twisty);
          const t = tint(row, left);
          if (t) tr.classList.add("cmp-row", `cmp-row-${t}`);
        } else {
          // Platzhalter mit gleichem Aufbau (unsichtbar), damit beide Seiten auf gleicher Höhe bleiben.
          const o = (row.l ?? row.r)!;
          tr = el("tr", "ghost");
          tr.title = (left ? row.l : row.r) ? `${o.name}: auf dieser Seite ausgeblendet (Filter oder versteckte Datei)` : `${o.name}: nur auf der anderen Seite`;
          const name = el("td", "name");
          name.append(twisty, el("span", `icon ${fileKind(o.name, o.dir)}`), el("span", "fname", o.name));
          tr.append(name, el("td", "size", "—"), el("td", "date", formatDate(o.modified)));
        }
        if (left) tr.append(statusCell(row));
        return tr;
      }),
    );
    if (!rows.length) emptyRow(p, cmpFilter === "all" ? "Keine Einträge" : "Keine passenden Einträge");
  }

  function statusCell(row: Row) {
    const td = el("td", "st");
    const st = rowState(row);
    const name = (row.l ?? row.r)!.name;
    if (st === "toRight" || st === "toLeft") {
      const b = el("button", `st-btn st-${st}`, SYMBOL[st]) as HTMLButtonElement;
      b.type = "button";
      const where = st === "toRight" ? `nach ${c.name}` : "auf diesen Computer";
      b.title = `«${name}» ${where} kopieren`;
      b.setAttribute("aria-label", b.title);
      b.onclick = (ev) => (ev.stopPropagation(), void copyRow(row, st === "toRight"));
      b.onpointerdown = (ev) => ev.stopPropagation();
      td.append(b);
    } else {
      const sp = el("span", `st-sym st-${st}`, SYMBOL[st]);
      sp.title = st === "same" ? "Gleich" : st === "differ" ? (bothDirs(row) ? "Inhalt verschieden – aufklappen" : "Verschieden ohne klare Richtung (gleiche Zeit, andere Grösse oder anderer Inhalt) – zum Kopieren in die gewünschte Richtung ziehen") : "Ordner – aufklappen zum Vergleichen";
      td.append(sp);
    }
    return td;
  }

  const info = (e?: Entry) => (e ? (e.dir ? "Ordner" : `${formatSize(e.size)}, ${formatDate(e.modified)}`) : "");
  const parentOf = (rel: string) => (rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "");
  const destDir = (to: Pane, rel: string) => (parentOf(rel) ? sidePath(to, parentOf(rel)) : to.listing!.path);

  /** Eine Zeile in Pfeilrichtung kopieren; ersetzt nur nach Bestätigung. */
  async function copyRow(row: Row, toRight: boolean) {
    const [from, to] = toRight ? [panes.local, panes.remote] : [panes.remote, panes.local];
    const [src, dst] = toRight ? [row.l, row.r] : [row.r, row.l];
    if (!sid || !src || !from.listing || !to.listing) return;
    if (dst) {
      const ok = await ask(
        `«${src.name}» ersetzen?`,
        `${toRight ? `Auf ${c.name}` : "Auf diesem Computer"} gibt es «${src.name}» schon.\n\nVorhanden: ${info(dst)}\nErsetzen durch: ${info(src)}`,
        "Ersetzen",
      );
      if (!ok) return;
    }
    start(toRight, sidePath(from, row.rel), destDir(to, row.rel), src.name, !!dst);
  }

  async function loadSub(rel: string) {
    const [l, r] = await Promise.all([api.localList(sidePath(panes.local, rel)), api.sftpList(sid!, sidePath(panes.remote, rel))]);
    subLists.set(rel, { l: l.entries, r: r.entries });
  }

  async function toggleExpand(rel: string) {
    if (expanded.delete(rel)) return renderAll();
    try {
      if (!subLists.has(rel)) await loadSub(rel);
      expanded.add(rel);
    } catch (e) {
      status.textContent = String(e);
    }
    renderAll();
  }

  /** Nach Übertragungen beide Seiten und aufgeklappte Ordner neu einlesen (gebündelt). */
  let refreshTimer = 0;
  function refreshCompareSoon() {
    clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(async () => {
      try {
        for (const p of Object.values(panes)) if (p.listing) p.listing = p.side === "local" ? await api.localList(p.listing.path) : await api.sftpList(sid!, p.listing.path);
        for (const rel of [...subLists.keys()]) await loadSub(rel).catch(() => (subLists.delete(rel), expanded.delete(rel)));
      } catch (e) {
        status.textContent = String(e);
      }
      renderAll();
    }, 500);
  }

  function renderAll() {
    host.classList.toggle("comparing", cmpOn());
    for (const p of Object.values(panes)) if (p.listing) render(p);
    bar.hidden = !comparing;
    compareBtn.classList.toggle("on", comparing);
    compareBtn.setAttribute("aria-pressed", String(comparing));
    if (!comparing) return;
    checkBtn.disabled = syncBtn.disabled = !cmpOn();
    if (!cmpOn()) return void (summary.textContent = "Zum Vergleichen links einen Ordner öffnen.");
    const rows = cmpRows("all");
    const n = (f: (r: Row) => boolean) => rows.filter(f).length;
    const parts = [
      [n((r) => !!r.l && !r.r), "nur auf diesem Computer"],
      [n((r) => !!r.r && !r.l), `nur auf ${c.name}`],
      [n((r) => r.mark === "newer"), "hier neuer"],
      [n((r) => r.mark === "older"), `auf ${c.name} neuer`],
      [n((r) => r.mark === "differ" && !bothDirs(r)), "verschieden"],
      [n((r) => r.mark === "same" && !bothDirs(r)), "gleich"],
    ].filter(([k]) => k) as [number, string][];
    summary.textContent = parts.length ? parts.map(([k, t]) => `${k} ${t}`).join(" · ") : "Keine Dateien zum Vergleichen";
  }

  function toggleCompare() {
    comparing = !comparing;
    if (!comparing) (expanded.clear(), subLists.clear());
    renderAll();
  }

  /** SHA-256 hier und auf dem Server für alle gleichnamigen Dateien (auch in aufgeklappten Ordnern). */
  async function checkContent() {
    if (!sid || !cmpOn()) return;
    const pairs = cmpRows("all").filter((r) => r.l && r.r && !r.l.dir && !r.r.dir);
    if (!pairs.length) return tell("Nichts zu prüfen", "In beiden Ordnern gibt es keine gleichnamigen Dateien.");
    const s = sid;
    checkBtn.disabled = true;
    try {
      for (const [i, r] of pairs.entries()) {
        const key = hashKey(r.rel, r.l!, r.r!);
        if (hashes.has(key)) continue;
        summary.textContent = `Prüfe Inhalt ${i + 1} von ${pairs.length}: ${r.rel}`;
        const [x, y] = await Promise.all([api.localSha256(sidePath(panes.local, r.rel)), api.sftpSha256(s, sidePath(panes.remote, r.rel))]);
        hashes.set(key, x === y);
      }
    } catch (e) {
      tell("Inhalt nicht geprüft", `${e}\n\nDer Vergleich nach Grösse und Änderungszeit bleibt.`);
    } finally {
      checkBtn.disabled = false;
      renderAll();
    }
  }

  /** «Synchronisieren …»: Richtung wählen, Vorschau sehen, dann übertragen. */
  async function syncDialog() {
    if (!sid || !cmpOn()) return;
    const rows = cmpRows("all").filter((r) => !bothDirs(r));
    const unclear = rows.filter((r) => rowState(r) === "differ").length;
    type Dir = "right" | "left" | "both";
    const jobs = (d: Dir) => rows.filter((r) => { const st = rowState(r); return (d !== "left" && st === "toRight") || (d !== "right" && st === "toLeft"); });
    const dlg = document.createElement("dialog");
    dlg.className = "sync-dlg";
    dlg.setAttribute("aria-labelledby", "sync-title");
    dlg.innerHTML = `
      <form class="sheet" method="dialog">
        <h2 id="sync-title">Synchronisieren</h2>
        <div class="segmented sync-dir" role="radiogroup" aria-label="Richtung">
          <label><input type="radio" name="dir" value="right" checked />→ nach <span class="to-name"></span></label>
          <label><input type="radio" name="dir" value="left" />← auf diesen Computer</label>
          <label><input type="radio" name="dir" value="both" />⇄ beide Richtungen</label>
        </div>
        <p class="note sync-count"></p>
        <ul class="sync-list"></ul>
        <p class="note sync-unclear"></p>
        <div class="row actions"><span></span><button type="button" value="cancel" class="sync-cancel">Abbrechen</button><button type="submit" class="primary sync-go">Übertragen</button></div>
      </form>`;
    dlg.querySelector(".to-name")!.textContent = c.name;
    document.body.append(dlg);
    const list = dlg.querySelector<HTMLElement>(".sync-list")!;
    const count = dlg.querySelector<HTMLElement>(".sync-count")!;
    const go = dlg.querySelector<HTMLButtonElement>(".sync-go")!;
    dlg.querySelector<HTMLElement>(".sync-unclear")!.textContent = unclear ? `${unclear} Datei(en) unterscheiden sich ohne klare Richtung und sind nicht dabei; einzeln in die gewünschte Richtung ziehen.` : "";
    let dir: Dir = "right";
    const show = () => {
      const js = jobs(dir);
      const replaced = js.filter((r) => r.l && r.r).length;
      count.textContent = js.length ? `${js.length} Element(e)${replaced ? `, davon ${replaced} ersetzt (ältere Fassung)` : ""}:` : "Nichts zu übertragen – in dieser Richtung ist alles aktuell.";
      list.replaceChildren(
        ...js.slice(0, 200).map((r) => {
          const li = el("li", "", `${rowState(r) === "toRight" ? "→" : "←"} ${r.rel}${r.l?.dir || r.r?.dir ? "/" : ""}${r.l && r.r ? " (ersetzt)" : ""}`);
          return li;
        }),
        ...(js.length > 200 ? [el("li", "more", `… und ${js.length - 200} weitere`)] : []),
      );
      go.disabled = !js.length;
      go.textContent = js.length ? `${js.length} übertragen` : "Übertragen";
    };
    for (const r of dlg.querySelectorAll<HTMLInputElement>("input[name=dir]")) r.onchange = () => ((dir = r.value as Dir), show());
    show();
    const chosen = await new Promise<boolean>((resolve) => {
      dlg.querySelector<HTMLButtonElement>(".sync-cancel")!.onclick = () => dlg.close();
      dlg.querySelector("form")!.onsubmit = (ev) => (ev.preventDefault(), resolve(true), dlg.close());
      dlg.onclose = () => resolve(false);
      dlg.showModal();
    });
    dlg.remove();
    if (!chosen || !sid) return;
    for (const r of jobs(dir)) {
      const toRight = rowState(r) === "toRight";
      const [from, to] = toRight ? [panes.local, panes.remote] : [panes.remote, panes.local];
      start(toRight, sidePath(from, r.rel), destDir(to, r.rel), (toRight ? r.l : r.r)!.name, !!(r.l && r.r));
    }
  }

  // ---------- Übertragungen ----------
  const other = (p: Pane) => (p.side === "local" ? panes.remote : panes.local);
  const running = new Set<string>();

  async function transfer(from: Pane, names: string[]) {
    const to = other(from);
    if (!sid || !to.listing || !from.listing) return;
    if (to.side === "local" && to.listing.path === "") return void (status.textContent = "Bitte zuerst links einen Ordner öffnen.");
    // Im Vergleich können Namen Unterordner enthalten («a/b»): dort entscheidet «Synchronisieren», hier ohne Rückfrage.
    const nested = names.filter((n) => !from.listing!.entries.some((e) => e.name === n));
    for (const n of nested) start(from.side === "local", join(from, n), to.listing.path, n);
    const items = from.listing.entries.filter((e) => names.includes(e.name));
    await copyInto(from.side === "local", from.listing.path, items, to.listing.path, to.listing.entries, {});
  }

  const pjoin = (sep: string, dir: string, name: string) => (dir === "" ? name : dir.endsWith(sep) ? dir + name : dir + sep + name);
  const listOf = (local: boolean, path: string) => (local ? api.localList(path) : api.sftpList(sid!, path));

  /** Elemente aus `srcDir` nach `destDir` kopieren; vorhandene Namen nur nach Rückfrage (askConflict).
   *  `policy`: «Für alle übernehmen», getrennt für Dateien und Ordner. `false` = abgebrochen. */
  async function copyInto(upload: boolean, srcDir: string, items: Entry[], destDir: string, existing: Entry[], policy: { file?: Choice; dir?: Choice }): Promise<boolean> {
    const [sSep, dSep] = upload ? [panes.local.sep, "/"] : ["/", panes.local.sep];
    let left = items.filter((e) => existing.some((d) => d.name === e.name)).length;
    for (const e of items) {
      const src = pjoin(sSep, srcDir, e.name);
      const dst = existing.find((d) => d.name === e.name);
      // Datei gegen Ordner gleichen Namens: nie ersetzen, mit Nummer daneben.
      if (!dst || e.dir !== dst.dir) {
        start(upload, src, destDir, e.name);
        continue;
      }
      left--;
      const key = e.dir ? "dir" : "file";
      let choice = policy[key];
      if (!choice) {
        const d = await askConflict(e, dst, upload ? localName : c.name, upload ? c.name : localName, destDir, left);
        if (!d.choice) return false;
        choice = d.choice;
        if (d.all) policy[key] = choice;
      }
      if (choice === "skip" || (choice === "newer" && (e.modified ?? 0) <= (dst.modified ?? 0))) continue;
      if (choice === "keep") start(upload, src, destDir, e.name);
      else if (choice === "merge") {
        try {
          const [sl, dl] = await Promise.all([listOf(upload, src), listOf(!upload, pjoin(dSep, destDir, e.name))]);
          if (!(await copyInto(upload, sl.path, sl.entries, dl.path, dl.entries, policy))) return false;
        } catch (err) {
          status.textContent = `«${e.name}» nicht zusammengeführt: ${err}`;
        }
      } else if (upload && ftp) {
        // FTP kann nicht ersetzen: alte Datei löschen, dann hochladen (ohne Papierkorb).
        api.sftpRemove(sid!, pjoin("/", destDir, e.name)).then(() => start(true, src, destDir, e.name)).catch((err) => (status.textContent = `«${e.name}» nicht ersetzt: ${err}`));
      } else start(upload, src, destDir, e.name, true);
    }
    return true;
  }

  /** Pfade von aussen (Finder/Explorer, iPad-Dateiauswahl, «Dateien»): wie Auswahl links, mit Rückfrage bei Konflikten. */
  async function uploadPaths(paths: string[]) {
    const dest = panes.remote.listing;
    if (!sid || !dest) return;
    const byDir = new Map<string, string[]>();
    for (const path of paths) {
      const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
      const [dir, name] = [path.slice(0, i) || "/", path.slice(i + 1)];
      byDir.set(dir, [...(byDir.get(dir) ?? []), name]);
    }
    const policy = {};
    for (const [dir, names] of byDir) {
      const items = await api.localList(dir).then((l) => l.entries.filter((e) => names.includes(e.name))).catch(() => null);
      if (!items) {
        for (const n of names) start(true, pjoin(panes.local.sep, dir, n), dest.path, n);
        continue;
      }
      if (!(await copyInto(true, dir, items, dest.path, dest.entries, policy))) return;
    }
  }

  /** Vor dem Ersetzen die alte Fassung in den Papierkorb kopieren; mit sudo, wenn dem Benutzer die Rechte fehlen
   *  (z. B. /etc). Zu gross oder nicht möglich: fragen. `false` = abgebrochen. */
  async function backupOld(target: string, name: string, admin: boolean): Promise<boolean> {
    const s = sid!;
    if (await tooBig(target)) return ask("Ohne Papierkorb ersetzen?", `Die alte Fassung von «${name}» ist grösser als ${mb()} und wird nicht gesichert. Trotzdem ersetzen, ohne Rückgängig?`, "Ersetzen");
    try {
      const slot = await api.sftpTrashSlot(s);
      const op = (pw?: string) => api.sftpSudoOp(s, "backup", target, slot, pw);
      if (!admin && !(await ask("Alte Fassung mit sudo sichern?", `Dein Benutzer darf «${name}» nicht in den Papierkorb verschieben. Sesam kopiert die alte Fassung mit sudo dorthin (Besitzer und Rechte werden gemerkt) und ersetzt sie dann.`, "Mit sudo sichern"))) return false;
      if (!(await withSudo("Alte Fassung sichern", (_, pw) => op(pw), true))) return false;
      undoRow(`↺ ${name} (ersetzt)`, `${slot}/${name}`, target, true);
      return true;
    } catch (e) {
      return ask("Alte Fassung nicht gesichert", `${e}\n\n«${name}» trotzdem ersetzen, ohne Rückgängig?`, "Ersetzen");
    }
  }

  /** iPad: ohne Auswahl links Dateien über die Dateiauswahl von iOS wählen und direkt hochladen. */
  async function pickAndUpload() {
    const dest = panes.remote.listing?.path;
    if (!sid || dest == null) return;
    const picked = await openDialog({ multiple: true, title: "Dateien hochladen" }).catch(() => null);
    void uploadPaths((Array.isArray(picked) ? picked : picked ? [picked] : []).map(String));
  }

  /** Hochladen ohne Schreibrecht im Ziel: in einen temporären Ordner laden, dann mit sudo an den Zielort verschieben. */
  async function uploadAsAdmin(src: string, destDir: string, name: string) {
    const s = sid;
    if (!s) return;
    const okd = await ask("Mit Administratorrechten hochladen?", `Dein Benutzer darf nicht in ${destDir} schreiben. Sesam lädt «${name}» zuerst in einen temporären Ordner auf ${c.name} und verschiebt es dann mit sudo dorthin. Bestehendes wird nicht überschrieben.`, "Mit sudo hochladen");
    if (!okd) return;
    try {
      const tmp = await api.sftpTempdir(s);
      start(true, src, tmp, name, false, async () => {
        try {
          let replace = false;
          const move = (pw?: string) => api.sftpSudoMove(s, `${tmp}/${name}`, destDir, replace, pw);
          const done = await withSudo("Verschieben", (_, pw) => move(pw), true).catch(async (e) => {
            if (!String(e).startsWith("EXISTS:")) throw e;
            if (!(await ask(`«${name}» ersetzen?`, `In ${destDir} gibt es «${name}» schon. Ersetzen behält Besitzer und Rechte der bestehenden Datei.`, "Ersetzen"))) return false;
            if (!(await backupOld(`${destDir.replace(/\/$/, "")}/${name}`, name, true))) return false;
            replace = true;
            return withSudo("Ersetzen", (_, pw) => move(pw), true);
          });
          if (done) {
            status.textContent = `«${name}» liegt jetzt in ${destDir}.`;
            if (panes.remote.listing?.path === destDir) load(panes.remote, destDir);
          }
        } catch (e) {
          tell("Nicht verschoben", `${e}\n\nDie Datei liegt noch in ${tmp} auf dem Server.`);
        }
      });
    } catch (e) {
      tell("Nicht hochgeladen", String(e));
    }
  }

  /** Herunterladen ohne Leserecht: mit sudo eine Kopie in einen temporären Ordner, die dir gehört, dann diese laden. */
  async function downloadAsAdmin(src: string, destDir: string, name: string) {
    const s = sid;
    if (!s || !(await ask("Mit Administratorrechten herunterladen?", `Dein Benutzer darf «${name}» nicht lesen. Sesam kopiert es mit sudo in einen temporären Ordner auf ${c.name}, lädt die Kopie herunter und löscht sie danach.`, "Mit sudo herunterladen"))) return;
    try {
      const tmp = await api.sftpTempdir(s);
      if (!(await withSudo("Kopieren", (_, pw) => api.sftpSudoOp(s, "fetch", src, tmp, pw), true))) return void api.sftpRemove(s, tmp).catch(() => {});
      start(false, `${tmp}/${name}`, destDir, name, false, () => void api.sftpRemove(s, tmp).catch(() => {}));
    } catch (e) {
      tell("Nicht heruntergeladen", String(e));
    }
  }

  function start(upload: boolean, src: string, destDir: string, name: string, replace: boolean | "force" = false, then?: () => void) {
    // Ersetzen auf dem Server: die alte Fassung zuerst in den Papierkorb, dann normal hochladen («force» = ohne).
    if (upload && replace === true && !ftp && sid) {
      const target = `${destDir.replace(/\/$/, "")}/${name}`;
      const go = () => start(true, src, destDir, name, "force", then);
      void (async () => {
        if (await tooBig(target)) return void ((await backupOld(target, name, false)) && go());
        try {
          undoRow(`↺ ${name} (ersetzt)`, await api.sftpTrash(sid!, target), target);
          go();
        } catch (e) {
          // Kein Recht, die alte Fassung wegzuschieben: mit sudo kopieren.
          if (DENIED.test(String(e)) ? await backupOld(target, name, false) : await ask("Alte Fassung nicht gesichert", `${e}\n\n«${name}» trotzdem ersetzen, ohne Rückgängig?`, "Ersetzen")) go();
        }
      })();
      return;
    }
    const row = el("div", "transfer");
    const label = el("span", "transfer-name", `${upload ? "↑" : "↓"} ${name} → ${upload ? c.name : localName}${replace ? " (ersetzt)" : ""}`);
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
          if (then) return then();
          const to = upload ? panes.remote : panes.local;
          if (comparing) refreshCompareSoon();
          else if (to.listing?.path === destDir) load(to, destDir);
        } else {
          finish(`Fehler: ${p.error}`);
          state.classList.add("error");
          state.title = p.error;
          // Keine Rechte (z. B. Hochladen nach /opt, Herunterladen aus /root): mit sudo anbieten.
          if (!ftp && !then && DENIED.test(p.error)) {
            const again = el("button", "plain", "Mit Administratorrechten wiederholen");
            again.type = "button";
            again.onclick = () => (again.remove(), upload ? uploadAsAdmin(src, destDir, name) : downloadAsAdmin(src, destDir, name));
            row.append(again);
          }
        }
      }, !!replace)
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
      const s = sid!;
      if (p.side === "local") await api.localMkdir(dir, name);
      else if (!(await withSudo("Ordner anlegen", (sudo, pw) => (sudo ? api.sftpSudoOp(s, "mkdir", dir, name, pw) : api.sftpMkdir(s, dir, name).then(() => {}))))) return;
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
      const s = sid;
      if (!(await withSudo("Datei anlegen", (sudo, pw) => (sudo ? api.sftpSudoOp(s, "touch", dir, name, pw) : api.sftpTouch(s, dir, name).then(() => {}))))) return;
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

  /** Fehlende Rechte auf dem Server? Dann (nur nach Bestätigung) mit sudo wiederholen; Passwort höchstens einmal fragen,
   *  nur über stdin an sudo. Gibt `false` zurück, wenn der Benutzer abbricht. */
  const DENIED = /permission denied|operation not permitted|eacces|eperm|keine berechtigung|zugriff verweigert/i;
  async function withSudo(what: string, run: (sudo: boolean, password?: string) => Promise<void>, confirmed = false): Promise<boolean> {
    if (!confirmed) {
      try {
        await run(false);
        return true;
      } catch (e) {
        if (!DENIED.test(String(e)) || ftp) throw e;
        if (!(await ask("Keine Berechtigung", `${String(e)}\n\n${what} mit Administratorrechten (sudo) wiederholen?`, "Mit sudo wiederholen"))) return false;
      }
    }
    try {
      await run(true);
      return true;
    } catch (e) {
      if (String(e) !== "SUDO_PASSWORD") throw e;
    }
    for (let i = 0; i < 2; i++) {
      const pw = await askPassword("Passwort für sudo", `sudo auf ${c.name} braucht das Passwort von ${c.user || "diesem Benutzer"}. Es wird nicht gespeichert.`, "Ausführen");
      if (!pw) return false;
      try {
        await run(true, pw);
        return true;
      } catch (e) {
        if (i === 1 || !String(e).includes("stimmt nicht")) throw e;
      }
    }
    return false;
  }

  /** Rechte-Dialog: rwx für Besitzer, Gruppe, Andere, als Zahl, Besitzer:Gruppe, rekursiv; über SSH mit sudo, wenn nötig. */
  async function permsDialog(e: Entry) {
    const p = panes.remote;
    if (!sid || !p.listing) return;
    const path = join(p, e.name);
    const s = sid;
    const owner = await api.sftpOwner(s, path).catch(() => "");
    const dlg = document.createElement("dialog");
    dlg.className = "perms-dlg";
    dlg.setAttribute("aria-labelledby", "perms-title");
    const who = ["Besitzer", "Gruppe", "Andere"];
    dlg.innerHTML = `<form class="sheet" method="dialog">
        <h2 id="perms-title">Berechtigungen</h2>
        <p class="note perms-path mono"></p>
        <table class="perms-grid"><thead><tr><th></th><th>Lesen</th><th>Schreiben</th><th>Ausführen</th></tr></thead><tbody>${who
          .map((w, i) => `<tr><th scope="row">${w}</th>${[4, 2, 1].map((b) => `<td><input type="checkbox" data-bit="${b << (3 * (2 - i))}" aria-label="${w}: ${b === 4 ? "Lesen" : b === 2 ? "Schreiben" : "Ausführen"}" /></td>`).join("")}</tr>`)
          .join("")}</tbody></table>
        <label class="row">Als Zahl<input name="octal" class="mono" inputmode="numeric" maxlength="4" size="5" /></label>
        <label class="row">Besitzer:Gruppe<input name="owner" class="mono" autocomplete="off" spellcheck="false" placeholder="benutzer:gruppe" /></label>
        <label class="row check"><input type="checkbox" name="recursive" />Auch für alles darin (rekursiv)</label>
        <p class="note">Wo dein Benutzer keine Rechte hat, fragt Sesam, ob es mit Administratorrechten (sudo) wiederholen soll.</p>
        <div class="row actions"><span></span><button type="button" class="perms-cancel">Abbrechen</button><button type="submit" class="primary">Übernehmen</button></div>
      </form>`;
    document.body.append(dlg);
    dlg.querySelector(".perms-path")!.textContent = path;
    const f = dlg.querySelector("form")!;
    const bits = [...dlg.querySelectorAll<HTMLInputElement>("[data-bit]")];
    const octal = f.elements.namedItem("octal") as HTMLInputElement;
    const own = f.elements.namedItem("owner") as HTMLInputElement;
    const rec = f.elements.namedItem("recursive") as HTMLInputElement;
    rec.closest("label")!.hidden = !e.dir;
    const start = e.perm ?? (e.dir ? 0o755 : 0o644);
    const special = start & 0o7000;
    const fromBits = () => bits.reduce((m, b) => m | (b.checked ? +b.dataset.bit! : 0), special);
    const show = (m: number) => {
      bits.forEach((b) => (b.checked = (m & +b.dataset.bit!) !== 0));
      octal.value = m.toString(8).padStart(3, "0");
    };
    show(start);
    own.value = owner;
    bits.forEach((b) => (b.onchange = () => (octal.value = fromBits().toString(8).padStart(3, "0"))));
    octal.oninput = () => /^[0-7]{3,4}$/.test(octal.value) && bits.forEach((b) => (b.checked = (parseInt(octal.value, 8) & +b.dataset.bit!) !== 0));
    const ok = await new Promise<boolean>((resolve) => {
      dlg.querySelector<HTMLButtonElement>(".perms-cancel")!.onclick = () => dlg.close();
      f.onsubmit = (ev) => {
        ev.preventDefault();
        if (!/^[0-7]{3,4}$/.test(octal.value)) return octal.setCustomValidity("Drei oder vier Ziffern von 0 bis 7, z. B. 644"), f.reportValidity(), octal.setCustomValidity("");
        resolve(true);
        dlg.close();
      };
      dlg.onclose = () => resolve(false);
      dlg.showModal();
    });
    dlg.remove();
    if (!ok) return;
    const mode = parseInt(octal.value, 8);
    const newOwner = own.value.trim() && own.value.trim() !== owner ? own.value.trim() : null;
    const recursive = e.dir && rec.checked;
    if (mode === start && !newOwner && !recursive) return;
    try {
      if (await withSudo("Rechte ändern", (sudo, pw) => api.sftpSetPerms(s, path, mode, newOwner, recursive, sudo, pw))) await load(p, p.listing!.path);
    } catch (err) {
      tell("Berechtigungen nicht geändert", String(err));
    }
  }

  async function chmod(e: Entry) {
    const p = panes.remote;
    if (!sid || !p.listing) return;
    if (!ftp) return permsDialog(e);
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
      if (remote) items.push({ value: "chmod", label: ftp ? "Rechte ändern …" : "Berechtigungen …", icon: ICON.permissions, disabled: !one || scp });
      items.push({ value: "copy", label: "Pfad kopieren", icon: ICON.copy });
      if (remote) items.push({ value: "delete", label: "Löschen", icon: ICON.trash, shortcut: "Entf", disabled: scp, separator: true });
    }
    if (p.listing?.entries.length) items.push({ value: "all", label: "Alle auswählen", shortcut: isMacUi ? "⌘A" : "Strg+A", separator: !!e });
    if (p.listing?.path) {
      items.push({ value: "mkdir", label: "Neuer Ordner …", icon: ICON.newFolder, separator: !!e, disabled: remote && !sid });
      if (remote) items.push({ value: "touch", label: "Neue Datei …", icon: ICON.plus, disabled: !sid || scp });
    }
    if (!items.length) return;
    const v = await openMenu(anchor, items);
    if (v === "transfer") transfer(p, [...p.selected]);
    if (v === "rename") remote ? renameRemote() : renameLocal();
    if (v === "chmod" && e) chmod(e);
    if (v === "copy") api.clipWrite([...p.selected].map((n) => join(p, n)).join("\n")).catch(() => {});
    if (v === "delete") deleteRemote();
    if (v === "mkdir") mkdir(p);
    if (v === "touch") newFile();
    if (v === "all") setSelection(p, new Set(rowNames(p)));
  }

  async function renameRemote() {
    const p = panes.remote;
    const [name] = p.selected;
    if (!sid || !p.listing || p.selected.size !== 1) return void (status.textContent = "Bitte rechts genau ein Element wählen.");
    const next = await askText("Umbenennen", `Neuer Name für «${name}»`, "Umbenennen", name);
    if (!next || next === name) return;
    try {
      const [s, from] = [sid, join(p, name)];
      if (!(await withSudo("Umbenennen", (sudo, pw) => (sudo ? api.sftpSudoOp(s, "rename", from, next, pw) : api.sftpRename(s, from, next).then(() => {}))))) return;
      await load(p, p.listing.path);
    } catch (e) {
      tell("Nicht umbenannt", String(e));
    }
  }

  // ---------- Papierkorb auf dem Server und Rückgängig ----------
  // Was Sesam löscht oder ersetzt, kommt zuerst nach ~/.sesam-papierkorb (Zeitstempel-Ordner); ⌘Z bzw. Strg+Z holt es zurück.
  const undoStack: { trashed: string; original: string; label: string; row: HTMLElement; sudo: boolean }[] = [];
  const trashDays = () => Math.max(1, Number(localStorage.getItem("sesam.trashDays")) || 7);
  const trashMax = () => (Number(localStorage.getItem("sesam.trashMaxMb")) || 500) * 1e6;
  /** `true`, wenn das Element zu gross für den Papierkorb ist (Grösse unbekannt: nein). */
  const tooBig = async (path: string) => (await api.sftpSize(sid!, path).catch(() => 0)) > trashMax();
  const mb = () => (trashMax() >= 1e9 ? `${trashMax() / 1e9} GB` : `${trashMax() / 1e6} MB`);

  /** Eintrag in der Übertragungsliste mit «Rückgängig». */
  function undoRow(label: string, trashed: string, original: string, sudo = false) {
    const row = el("div", "transfer");
    const name = el("span", "transfer-name", label);
    const state = el("span", "transfer-state", `im Papierkorb auf ${c.name} (${trashDays()} Tage)`);
    state.title = trashed;
    const btn = el("button", "plain", "Rückgängig");
    btn.type = "button";
    btn.title = `Rückgängig (${navigator.userAgent.includes("Mac") ? "⌘Z" : "Strg+Z"})`;
    const entry = { trashed, original, label, row, sudo };
    btn.onclick = () => void undo(entry);
    row.append(name, el("span"), state, btn);
    transferList.prepend(row);
    undoStack.push(entry);
  }

  async function undo(entry = undoStack[undoStack.length - 1]) {
    if (!entry || !sid) return;
    try {
      const s = sid;
      // Mit sudo Gesichertes (z. B. aus /etc) mit sudo zurück, samt Besitzer und Rechten.
      const restore = (pw?: string) => api.sftpSudoOp(s, "restore", entry.trashed, entry.original, pw);
      if (entry.sudo) {
        if (!(await withSudo("Zurückholen", (_, pw) => restore(pw), true))) return;
      } else await api.sftpRestore(s, entry.trashed, entry.original);
      undoStack.splice(undoStack.indexOf(entry), 1);
      entry.row.querySelector("button")?.remove();
      entry.row.querySelector(".transfer-state")!.textContent = "zurückgeholt";
      if (panes.remote.listing) await load(panes.remote, panes.remote.listing.path);
    } catch (e) {
      tell("Nicht zurückgeholt", `${e}\n\nDie Fassung liegt weiter unter ${entry.trashed}.`);
    }
  }
  ctx.pane.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === "z" && !(e.target instanceof HTMLInputElement) && undoStack.length) (e.preventDefault(), void undo());
  });

  /** Ein Element in den Papierkorb (mit sudo, wenn nötig). Kann der Papierkorb nicht geschrieben werden,
   *  fragt Sesam, ob endgültig gelöscht werden soll. `false` = abgebrochen. */
  async function trashOne(path: string, label: string): Promise<boolean> {
    const s = sid!;
    if (await tooBig(path)) {
      if (!(await ask("Ohne Papierkorb löschen?", `«${label}» ist grösser als ${mb()} (Einstellungen › Allgemein) und kommt darum nicht in den Papierkorb. Endgültig löschen, ohne Rückgängig?`, "Endgültig löschen"))) return false;
      return withSudo("Löschen", (sudo, pw) => (sudo ? api.sftpSudoOp(s, "remove", path, "", pw) : api.sftpRemove(s, path)));
    }
    try {
      undoRow(label, await api.sftpTrash(s, path), path);
      return true;
    } catch (e) {
      if (DENIED.test(String(e)) && !ftp) {
        const slot = await api.sftpTrashSlot(s);
        if (!(await withSudo("In den Papierkorb legen", (_, pw) => api.sftpSudoOp(s, "trash", path, slot, pw), false).catch(() => false))) return false;
        undoRow(label, `${slot}/${path.split("/").pop()}`, path, true);
        return true;
      }
      if (!(await ask("Papierkorb nicht möglich", `${e}\n\n«${label}» endgültig löschen, ohne Rückgängig?`, "Endgültig löschen"))) return false;
      return withSudo("Löschen", (sudo, pw) => (sudo ? api.sftpSudoOp(s, "remove", path, "", pw) : api.sftpRemove(s, path)));
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
      ftp
        ? `Wird auf ${c.name} endgültig gelöscht.${dirs ? " Ordner werden samt Inhalt gelöscht." : ""}`
        : `Kommt in den Papierkorb auf ${c.name} und lässt sich ${trashDays()} Tage lang zurückholen (Rückgängig bzw. ${navigator.userAgent.includes("Mac") ? "⌘Z" : "Strg+Z"}).`,
      "Löschen",
    );
    if (!ok) return;
    for (const n of names) {
      try {
        const [s, path] = [sid, join(p, n)];
        if (ftp) {
          if (!(await withSudo("Löschen", () => api.sftpRemove(s, path)))) break;
        } else if (!(await trashOne(path, `🗑 ${n}`))) break;
      } catch (e) {
        tell("Nicht gelöscht", String(e));
        break;
      }
    }
    await load(p, p.listing.path);
  }

  // «Dem Ordner der Shell folgen»: wechselt eine Shell desselben Geräts den Ordner (OSC 7), zieht die rechte Seite mit.
  let follow = false;
  const followBtn = toolButton(ICON.shell, "Dem Ordner der Shell folgen", () => {
    follow = !follow;
    followBtn.setAttribute("aria-pressed", String(follow));
    followBtn.classList.toggle("on", follow);
    status.textContent = follow ? "Folgt dem Ordner der Shell (wenn die Shell ihn meldet)." : "";
  }, { secondary: true });
  followBtn.setAttribute("aria-pressed", "false");
  const onShellCwd = (e: Event) => {
    const { conn, path } = (e as CustomEvent<{ conn: string; path: string }>).detail;
    if (follow && sid && conn === c.id && path !== panes.remote.listing?.path) load(panes.remote, path, "go");
  };
  addEventListener("sesam:shell-cwd", onShellCwd);

  const renameBtn = toolButton(ICON.rename, "Umbenennen", () => renameRemote(), { shortcut: "F2" });
  const compareBtn = toolButton(ICON.compare, "Vergleichen", () => toggleCompare(), { text: true, shortcut: "linken und rechten Ordner" });
  compareBtn.setAttribute("aria-pressed", "false");
  const trashBtn = toolButton(ICON.trash, "Löschen", () => deleteRemote(), { shortcut: "Entf", danger: true });
  ctx.toolbar.append(
    toolButton(ICON.upload, "Hochladen", () => (isMobile && !panes.local.selected.size ? void pickAndUpload() : transfer(panes.local, [...panes.local.selected])), { text: true }),
    toolButton(ICON.download, "Herunterladen", () => transfer(panes.remote, [...panes.remote.selected]), { text: true }),
    compareBtn,
    toolSep(),
    toolButton(ICON.newFolder, "Neuer Ordner", () => mkdir()),
    toolButton(ICON.plus, "Neue Datei", () => newFile()),
    renameBtn,
    trashBtn,
    toolButton(ICON.refresh, "Neu laden", () => {
      for (const p of Object.values(panes)) if (p.listing) load(p, p.listing.path);
    }),
    followBtn,
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

  // iPad: aus «Dateien» auf die Server-Seite ziehen
  dropFiles(panes.remote.root, () => ctx.isActive() && !!sid && !!panes.remote.listing, (paths) => uploadPaths(paths));
  // Dateien aus Explorer/Finder auf die Server-Seite ziehen (nur im sichtbaren Tab).
  const unlistenDrop = getCurrentWebview().onDragDropEvent((ev) => {
    if (!ctx.isActive()) return;
    const t = ev.payload.type;
    const pos = "position" in ev.payload ? ev.payload.position : null;
    const over = pos ? paneAt(pos.x / devicePixelRatio, pos.y / devicePixelRatio) : undefined;
    panes.remote.root.classList.toggle("drop", t === "over" && over === panes.remote);
    if (t === "drop" && over === panes.remote && sid && panes.remote.listing) void uploadPaths(ev.payload.paths);
  });

  const close = () => {
    removeEventListener("sesam:shell-cwd", onShellCwd);
    unlistenDrop.then((u) => u());
    for (const t of running) api.transferCancel(t);
    running.clear();
    if (sid) api.sftpClose(sid);
    sid = null;
  };

  // ---------- Verbinden ----------
  const connect = async () => {
  if (isMobile) localRoot = await api.localHome().catch(() => "");
  if (!panes.local.listing) load(panes.local, await api.localHome());
  let trust: string | null = null;
  let remember = true;
  for (;;) {
    try {
      const at = ctx.extra?.attach;
      const st = at?.state as { scp: boolean; remote: string; local: string } | undefined;
      const opened = at ? { sid: at.sid, scp: !!st?.scp } : await api.sftpOpen(c.id, trust, remember);
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
      if (!ftp && !opened.scp) void api.sftpTrashCleanup(opened.sid, trashDays()).catch(() => {}); // alte Einträge im Papierkorb
      await load(panes.remote, st?.remote ?? panes.remote.listing?.path ?? "");
    } catch (e) {
      const msg = String(e);
      const changed = msg.startsWith(CERT_CHANGED);
      if (trust === null && (changed || msg.startsWith(CERT_UNKNOWN))) {
        // FTPS: Zertifikat bestätigen wie bei RDP/VNC
        const fp = msg.slice((changed ? CERT_CHANGED : CERT_UNKNOWN).length);
        const choice = await choose(
          changed ? "Zertifikat hat sich geändert" : "Unbekanntes Zertifikat",
          changed
            ? `Das Zertifikat von ${c.address}:${c.ftpPort} ist nicht mehr dasselbe wie beim letzten Mal. Das kann ein Angriff sein, oder der Anbieter hat es erneuert (bei Let's Encrypt alle paar Wochen).`
            : `${c.address}:${c.ftpPort} zeigt dieses Zertifikat. Vergleiche den Fingerabdruck mit den Angaben des Anbieters, bevor du ihm vertraust.`,
          `SHA-256\n${fp.replace(/^SHA256:/, "").toLowerCase()}`,
          changed ? "Neuem Zertifikat vertrauen" : "Vertrauen und verbinden",
          "Nur dieses Mal verbinden",
        );
        if (choice) {
          trust = fp;
          remember = choice === "ok";
          continue;
        }
        status.textContent = "Verbindung abgebrochen: Zertifikat nicht bestätigt.";
      } else if (trust === null && msg.startsWith(HOSTKEY_UNKNOWN)) {
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

  return {
    close,
    reconnect() {
      if (sid) api.sftpClose(sid);
      sid = null;
      status.textContent = "";
      void connect();
    },
    handoff: () => (sid ? { sid, state: { scp, remote: panes.remote.listing?.path ?? "", local: panes.local.listing?.path ?? "" } } : null),
    release: () => {
      sid = null; // SFTP-Sitzung läuft im anderen Fenster weiter
      close();
    },
    busy: () => (running.size ? `${running.size} Übertragung(en) laufen noch und werden abgebrochen.` : null),
  };
}
