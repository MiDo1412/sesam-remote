/** Eigener Dialog statt alert/confirm: die nativen fehlen in WKWebView (macOS/iPad). */
export function ask(title: string, text: string, ok = "OK", cancel: string | null = "Abbrechen"): Promise<boolean> {
  const dlg = document.getElementById("ask") as HTMLDialogElement;
  document.getElementById("ask-title")!.textContent = title;
  document.getElementById("ask-text")!.textContent = text;
  document.getElementById("ask-ok")!.textContent = ok;
  const c = document.getElementById("ask-cancel")!;
  c.hidden = cancel === null;
  c.textContent = cancel ?? "";
  dlg.returnValue = "";
  dlg.showModal();
  return new Promise((resolve) => dlg.addEventListener("close", () => resolve(dlg.returnValue === "ok"), { once: true }));
}

export const tell = (title: string, text: string) => ask(title, text, "OK", null).then(() => {});
