// Kleine Helfer und Typen der Dateiansicht (Grösse, Datum, Dateityp, Rechte).
import { locale } from "../i18n";
import type { Entry, Listing } from "../api";

export const el = <K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = "") => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
};

const num = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 });
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

export function formatDate(secs: number | null) {
  if (secs === null) return "";
  const d = new Date(secs * 1000);
  const now = new Date();
  const time = d.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === now.toDateString()) return `${new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(0, "day")}, ${time}`;
  return d.toLocaleDateString(locale, { day: "numeric", month: "short", ...(d.getFullYear() !== now.getFullYear() && { year: "numeric" }) });
}

export type Side = "local" | "remote";

export type SortKey = "name" | "size" | "date";

/** Dateityp grob nach Endung, für das Symbol in der Liste. */
export function fileKind(name: string, dir: boolean) {
  if (dir) return "dir";
  const ext = name.toLowerCase().split(".").pop() ?? "";
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "heic", "bmp"].includes(ext)) return "img";
  if (["zip", "gz", "tgz", "xz", "bz2", "7z", "rar", "zst", "tar"].includes(ext)) return "zip";
  if (["pdf"].includes(ext)) return "pdf";
  if (["sh", "py", "js", "ts", "rs", "go", "c", "h", "json", "yml", "yaml", "toml", "conf", "ini", "md", "txt", "log", "html", "css"].includes(ext)) return "txt";
  return "file";
}

export interface Pane {
  side: Side;
  root: HTMLElement;
  body: HTMLElement;
  pathEl: HTMLElement;
  listing: Listing | null;
  selected: Set<string>;
  /** Ausgangspunkt für Umschalt-Klick und die Zeile mit dem Tastatur-Fokus. */
  anchor: string | null;
  cursor: string | null;
  /** Suche in Unterordnern: Treffer mit relativem Pfad; `token` bricht eine alte Suche ab. */
  deepOn: boolean;
  deep: Entry[] | null;
  token: number;
  /** iPad: Tippen wählt dazu/weg (Häkchen). */
  selectMode: boolean;
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

/** 0o755 → «rwxr-xr-x» (mit d für Ordner). */
export function permText(m: number, dir: boolean) {
  const r = [6, 3, 0].map((s) => ["r", "w", "x"].map((ch, i) => ((m >> (s + 2 - i)) & 1 ? ch : "-")).join("")).join("");
  return (dir ? "d" : "-") + r;
}
