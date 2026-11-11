/** Rückfrage vor dem Überschreiben im Dateifenster: beide Fassungen nebeneinander, Wahl für dieses oder alle
 *  weiteren Elemente. Sesam überschreibt nie still; der Papierkorb auf dem Server bleibt als Sicherheitsnetz. */
import type { Entry } from "./api";

export type Choice = "replace" | "skip" | "keep" | "newer" | "merge";
export interface Decision {
  choice: Choice | null; // null = abgebrochen
  all: boolean;
}

const size = (n: number) => {
  if (n < 1024) return `${n} B`;
  const u = ["KB", "MB", "GB", "TB"];
  let i = -1;
  do (n /= 1024), i++;
  while (n >= 1024 && i < u.length - 1);
  return `${n.toFixed(n < 10 ? 1 : 0)} ${u[i]}`;
};
const when = (s: number | null) => (s ? new Date(s * 1000).toLocaleString("de-CH", { dateStyle: "medium", timeStyle: "short" }) : "unbekannt");

/** `src`/`dst`: Quelle und vorhandenes Ziel; `rest`: weitere Konflikte nach diesem. */
export function askConflict(src: Entry, dst: Entry, from: string, to: string, dir: string, rest: number): Promise<Decision> {
  const folder = src.dir && dst.dir;
  const dlg = document.createElement("dialog");
  dlg.className = "conflict";
  dlg.setAttribute("aria-labelledby", "conflict-title");
  const newer = (src.modified ?? 0) > (dst.modified ?? 0) ? "src" : (src.modified ?? 0) < (dst.modified ?? 0) ? "dst" : "";
  const side = (label: string, e: Entry, isNew: boolean) => `
    <div class="conflict-side${isNew ? " newer" : ""}">
      <div class="conflict-where"></div>
      <div class="conflict-label">${label}${isNew ? ' <span class="conflict-badge">neuer</span>' : ""}</div>
      <div class="conflict-meta">${e.dir ? "Ordner" : size(e.size)} · ${when(e.modified)}</div>
    </div>`;
  dlg.innerHTML = `<form class="sheet" method="dialog">
      <h2 id="conflict-title"></h2>
      <p class="note conflict-text"></p>
      <div class="conflict-sides">${side("Vorhanden", dst, newer === "dst")}${side("Neu", src, newer === "src")}</div>
      <label class="conflict-all check"${rest ? "" : " hidden"}><input type="checkbox" /> <span>Für alle übernehmen (noch ${rest} ${rest === 1 ? "Konflikt" : "Konflikte"})</span></label>
      <div class="row actions conflict-actions">
        <button type="submit" value="">Abbrechen</button>
        <span></span>
        <button type="submit" value="skip">Überspringen</button>
        <button type="submit" value="keep">Beide behalten</button>
        ${folder ? "" : '<button type="submit" value="newer">Nur wenn neuer</button>'}
        <button type="submit" value="${folder ? "merge" : "replace"}" class="primary">${folder ? "Zusammenführen" : "Ersetzen"}</button>
      </div>
    </form>`;
  dlg.querySelector("h2")!.textContent = folder ? `Ordner «${src.name}» gibt es schon` : `«${src.name}» gibt es schon`;
  dlg.querySelector(".conflict-text")!.textContent = folder
    ? `In ${dir || "/"} gibt es bereits einen Ordner mit diesem Namen. «Zusammenführen» kopiert den Inhalt hinein und fragt bei jeder vorhandenen Datei nach; «Beide behalten» legt den neuen Ordner mit Nummer daneben.`
    : `In ${dir || "/"} liegt bereits eine Datei mit diesem Namen. «Ersetzen» überschreibt sie erst, wenn die neue Fassung vollständig da ist; die alte Fassung wird vorher gesichert (SFTP: Papierkorb auf dem Server, FTP: auf diesem Computer), mit «Rückgängig»; «Beide behalten» legt die neue mit Nummer daneben.`;
  const wheres = dlg.querySelectorAll<HTMLElement>(".conflict-where");
  wheres[0].textContent = to;
  wheres[1].textContent = from;
  document.body.append(dlg);
  dlg.showModal();
  (dlg.querySelector<HTMLButtonElement>("button.primary") ?? dlg.querySelector("button"))!.focus();
  return new Promise((resolve) =>
    dlg.addEventListener(
      "close",
      () => {
        const all = dlg.querySelector<HTMLInputElement>(".conflict-all input")!.checked;
        dlg.remove();
        resolve({ choice: (dlg.returnValue || null) as Choice | null, all });
      },
      { once: true },
    ),
  );
}
