/** Kleine Hilfe pro Feld: «?» neben der Beschriftung öffnet ein Popover mit Text aus data-tip. */
let open: HTMLElement | null = null;

function close() {
  open?.remove();
  open = null;
}

export function initFieldHelp(root: HTMLElement) {
  for (const label of root.querySelectorAll<HTMLElement>("[data-tip]")) {
    if (label.querySelector(".tip-btn")) continue;
    const b = document.createElement("button");
    b.type = "button";
    b.className = "tip-btn";
    b.textContent = "?";
    const name = label.firstChild?.textContent?.trim() ?? "";
    b.setAttribute("aria-label", `Hilfe zu ${name}`);
    b.onclick = (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (open?.dataset.for === name) return close();
      close();
      const pop = document.createElement("div");
      pop.className = "popover";
      pop.setAttribute("role", "tooltip");
      pop.dataset.for = name;
      pop.textContent = label.dataset.tip ?? "";
      (b.closest("dialog") ?? document.body).append(pop);
      const r = b.getBoundingClientRect();
      const p = pop.getBoundingClientRect();
      pop.style.left = `${Math.max(8, Math.min(r.left - 12, innerWidth - p.width - 8))}px`;
      pop.style.top = `${r.bottom + 6 + p.height > innerHeight ? r.top - p.height - 6 : r.bottom + 6}px`;
      open = pop;
    };
    // Erster Textknoten ist die Beschriftung: Knopf direkt dahinter.
    label.firstChild ? label.firstChild.after(b) : label.prepend(b);
  }
}

addEventListener("pointerdown", (e) => {
  if (open && !(e.target as HTMLElement).closest(".tip-btn, .popover")) close();
});
addEventListener("keydown", (e) => {
  if (e.key === "Escape" && open) {
    e.stopPropagation();
    close();
  }
}, true);
