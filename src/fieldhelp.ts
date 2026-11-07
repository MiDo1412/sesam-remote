/** Kleine Hilfe pro Feld: Fragezeichen rechts neben der Beschriftung, Popover bei Klick oder Hover (Text aus data-tip). */
import { ICON } from "./icons";
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
    b.innerHTML = ICON.question;
    const name = label.firstChild?.textContent?.trim() ?? "";
    b.setAttribute("aria-label", `Hilfe zu ${name}`);
    const show = () => {
      if (open?.dataset.for === name) return;
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
    let byHover = false;
    b.onclick = (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (open?.dataset.for === name && !byHover) return close();
      byHover = false;
      show();
    };
    b.onmouseenter = () => !open && ((byHover = true), show());
    b.onmouseleave = () => byHover && ((byHover = false), close());
    // Beschriftung und Knopf auf eine Zeile (die Beschriftung ist der erste Textknoten).
    const line = document.createElement("span");
    line.className = "label-line";
    const text = label.firstChild;
    if (text?.nodeType === Node.TEXT_NODE) line.append(text.textContent!.trim()), text.remove();
    line.append(b);
    label.prepend(line);
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
