/** Vorschau wie Quick Look (Leertaste im Dateifenster): Text, Bilder und PDF; anderes mit Grösse und Hinweis.
 *  Leertaste oder Esc schliesst. */
const IMG: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp", svg: "image/svg+xml", bmp: "image/bmp", ico: "image/x-icon", heic: "image/heic" };

/** Text? Keine Nullbytes in den ersten 8 KB und gültiges UTF-8. */
export function asText(bytes: Uint8Array): string | null {
  if (bytes.subarray(0, 8192).includes(0)) return null;
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

export function showPreview(name: string, data: ArrayBuffer, total: number, truncated: boolean) {
  const ext = name.toLowerCase().split(".").pop() ?? "";
  const bytes = new Uint8Array(data);
  const dlg = document.createElement("dialog");
  dlg.className = "preview";
  dlg.setAttribute("aria-label", `Vorschau ${name}`);
  // Einheitliches Blatt: Titel im Blatt, Inhalt, Fussleiste mit Hinweis links und Knopf rechts.
  const title = document.createElement("h2");
  title.textContent = name;
  const close = document.createElement("button");
  close.type = "button";
  close.className = "primary";
  close.textContent = "Schliessen";
  close.onclick = () => dlg.close();
  const body = document.createElement("div");
  body.className = "preview-body";
  let url = "";
  const mime = IMG[ext] ?? (ext === "pdf" ? "application/pdf" : "");
  if (mime) {
    url = URL.createObjectURL(new Blob([bytes], { type: mime }));
    const view = document.createElement(ext === "pdf" ? "iframe" : "img") as HTMLImageElement | HTMLIFrameElement;
    view.src = url;
    if (view instanceof HTMLImageElement) view.alt = name;
    else view.title = name;
    body.append(view);
  } else {
    const text = asText(bytes);
    const pre = document.createElement("pre");
    pre.textContent = text ?? "Keine Vorschau für diese Art von Datei.";
    if (text === null) pre.className = "preview-none";
    body.append(pre);
  }
  const foot = document.createElement("div");
  foot.className = "actions preview-foot";
  const note = document.createElement("span");
  note.className = "note";
  note.textContent = truncated ? `Gezeigt werden die ersten ${Math.round(data.byteLength / 1024)} KB von ${Math.round(total / 1024)} KB.` : "";
  foot.append(note, close);
  dlg.append(title, body, foot);
  dlg.addEventListener("keydown", (e) => e.key === " " && (e.preventDefault(), dlg.close()));
  dlg.onclose = () => (url && URL.revokeObjectURL(url), dlg.remove());
  document.body.append(dlg);
  dlg.showModal();
  close.focus();
}

if (import.meta.env?.DEV) {
  console.assert(asText(new TextEncoder().encode("Grüezi")) === "Grüezi", "Text erkannt");
  console.assert(asText(new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0, 1])) === null, "Binär erkannt");
}
