/** iPad: Dateien aus der App «Dateien» (Split View, Slide Over) auf eine Sitzung ziehen. Die Webansicht kennt nur den
 *  Inhalt, nicht den Pfad: Sesam legt jede Datei zuerst in den Cache der App (stage_file) und gibt die Pfade dann an
 *  dieselbe Stelle wie das Ziehen aus Finder/Explorer auf dem Desktop. Ordner gehen so nicht, nur Dateien. */
import { invoke } from "@tauri-apps/api/core";
import { isMobile } from "./titlebar";

export function dropFiles(target: HTMLElement, isActive: () => boolean, onPaths: (paths: string[]) => void | Promise<void>) {
  if (!isMobile) return;
  const files = (e: DragEvent) => !!e.dataTransfer?.types.includes("Files") && isActive();
  const over = (e: DragEvent) => {
    if (!files(e)) return;
    e.preventDefault();
    e.dataTransfer!.dropEffect = "copy";
    target.classList.add("drop");
  };
  target.addEventListener("dragenter", over);
  target.addEventListener("dragover", over);
  target.addEventListener("dragleave", (e) => !target.contains(e.relatedTarget as Node) && target.classList.remove("drop"));
  target.addEventListener("drop", async (e) => {
    target.classList.remove("drop");
    if (!files(e)) return;
    e.preventDefault();
    const paths: string[] = [];
    for (const f of e.dataTransfer!.files) {
      paths.push(await invoke<string>("stage_file", new Uint8Array(await f.arrayBuffer()), { headers: { "x-name": encodeURIComponent(f.name) } }));
    }
    if (paths.length) await onPaths(paths);
  });
}
