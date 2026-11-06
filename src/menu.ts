/** Menüs im Stil von macOS: Pull-down (Aktionen) und Pop-up mit Häkchen (Auswahlwerte).
 *  Ersetzt native <select>, die unter Windows/WebView2 fremd aussehen.
 *  Tastatur: Pfeile, Pos1/Ende, Enter/Leertaste, Esc, Tippen springt zum Eintrag. */

export interface MenuItem {
  value: string;
  label: string;
  /** Tastenkürzel rechts, nur zur Anzeige. */
  shortcut?: string;
  /** Inline-SVG (24×24, currentColor). */
  icon?: string;
  disabled?: boolean;
  /** Abschnittstitel vor diesem Eintrag. */
  section?: string;
  /** Trennlinie vor diesem Eintrag. */
  separator?: boolean;
}

const svg = (d: string) =>
  `<svg viewBox="0 0 256 256" width="16" height="16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="16" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`;
export const ICON_CHEVRON = svg('<polyline points="208 96 128 176 48 96"/>');
export const ICON_CHECK = svg('<polyline points="40 144 96 200 224 72"/>');

let current: { close: (value: string | null) => void } | null = null;

/** Menü an `anchor` öffnen; liefert den gewählten Wert oder null. */
export function openMenu(anchor: HTMLElement, items: MenuItem[], opts: { checked?: string; listbox?: boolean } = {}): Promise<string | null> {
  current?.close(null);
  return new Promise((resolve) => {
    const menu = document.createElement("div");
    menu.className = "menu";
    menu.setAttribute("role", opts.listbox ? "listbox" : "menu");
    menu.tabIndex = -1;
    const rows: HTMLElement[] = [];
    for (const it of items) {
      if (it.separator) menu.append(Object.assign(document.createElement("div"), { className: "menu-sep", role: "separator" }));
      if (it.section) menu.append(Object.assign(document.createElement("div"), { className: "menu-section", textContent: it.section }));
      const row = document.createElement("div");
      row.className = "menu-item";
      row.setAttribute("role", opts.listbox ? "option" : opts.checked !== undefined ? "menuitemradio" : "menuitem");
      if (opts.checked !== undefined) row.setAttribute(opts.listbox ? "aria-selected" : "aria-checked", String(it.value === opts.checked));
      if (it.disabled) row.setAttribute("aria-disabled", "true");
      row.dataset.value = it.value;
      row.innerHTML = `<span class="menu-check">${opts.checked !== undefined && it.value === opts.checked ? ICON_CHECK : ""}</span>${
        it.icon ? `<span class="menu-icon">${it.icon}</span>` : ""
      }<span class="menu-label"></span>${it.shortcut ? `<span class="menu-key"></span>` : ""}`;
      row.querySelector(".menu-label")!.textContent = it.label;
      if (it.shortcut) row.querySelector(".menu-key")!.textContent = it.shortcut;
      row.onpointermove = () => focus(rows.indexOf(row));
      row.onclick = () => !it.disabled && close(it.value);
      rows.push(row);
      menu.append(row);
    }
    // In einem offenen Dialog dorthin: modale Dialoge liegen in der obersten Ebene, alles andere ist inaktiv.
    (anchor.closest("dialog[open]") ?? document.body).append(menu);

    // Platzierung: unter dem Knopf, sonst darüber; nach links, wenn rechts kein Platz.
    const a = anchor.getBoundingClientRect();
    const m = menu.getBoundingClientRect();
    const below = innerHeight - a.bottom - 8 >= m.height || a.top < m.height + 8;
    menu.style.top = `${below ? a.bottom + 4 : a.top - m.height - 4}px`;
    menu.style.left = `${Math.max(8, Math.min(a.left, innerWidth - m.width - 8))}px`;
    menu.style.minWidth = `${a.width}px`;
    menu.style.transformOrigin = below ? "top" : "bottom";
    if (!below) menu.style.maxHeight = `${a.top - 12}px`;

    let active = -1;
    const enabled = (i: number) => rows[i] && rows[i].getAttribute("aria-disabled") !== "true";
    function focus(i: number) {
      if (!enabled(i)) return;
      rows[active]?.classList.remove("active");
      active = i;
      rows[i].classList.add("active");
      rows[i].scrollIntoView({ block: "nearest" });
    }
    const step = (d: number) => {
      for (let k = 1; k <= rows.length; k++) {
        const i = (active + d * k + rows.length * 2) % rows.length;
        if (enabled(i)) return focus(i);
      }
    };
    let typed = "";
    let typedAt = 0;
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === "ArrowDown") step(1);
      else if (e.key === "ArrowUp") step(-1);
      else if (e.key === "Home") focus(rows.findIndex((_, i) => enabled(i)));
      else if (e.key === "End") focus(rows.length - 1 - [...rows].reverse().findIndex((_, i) => enabled(rows.length - 1 - i)));
      else if (e.key === "Enter" || e.key === " ") {
        if (active >= 0) close(rows[active].dataset.value!);
      } else if (e.key === "Escape" || e.key === "Tab") close(null);
      else if (e.key.length === 1) {
        typed = (Date.now() - typedAt > 700 ? "" : typed) + e.key.toLowerCase();
        typedAt = Date.now();
        const i = items.findIndex((it, j) => enabled(j) && it.label.toLowerCase().startsWith(typed));
        if (i >= 0) focus(i);
      } else return;
      e.preventDefault();
    };
    const onDown = (e: PointerEvent) => {
      if (!menu.contains(e.target as Node) && !anchor.contains(e.target as Node)) close(null);
    };
    menu.addEventListener("keydown", onKey);
    setTimeout(() => document.addEventListener("pointerdown", onDown, true));
    addEventListener("blur", () => close(null), { once: true });

    function close(value: string | null) {
      if (!menu.isConnected) return;
      document.removeEventListener("pointerdown", onDown, true);
      menu.classList.add("closing");
      setTimeout(() => menu.remove(), matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 90);
      anchor.setAttribute("aria-expanded", "false");
      anchor.focus();
      current = null;
      resolve(value);
    }
    current = { close };
    anchor.setAttribute("aria-expanded", "true");
    menu.focus();
    focus(Math.max(0, items.findIndex((it) => it.value === opts.checked)));
  });
}

/** Ersetzt ein <select> durch einen Knopf mit Menü. Der Wert bleibt im (versteckten) select,
 *  Formulare und `change`-Handler funktionieren weiter.
 *  pulldown: Aktionsmenü mit festem Titel (z. B. «Tasten senden»), sonst Pop-up mit Häkchen. */
export function enhanceSelect(select: HTMLSelectElement, opts: { pulldown?: { label: string; icon?: string }; shortcuts?: Record<string, string> } = {}) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = opts.pulldown ? "menu-button pulldown" : "menu-button";
  btn.setAttribute("aria-haspopup", opts.pulldown ? "menu" : "listbox");
  btn.setAttribute("aria-expanded", "false");
  const label = select.getAttribute("aria-label") ?? select.closest("label")?.firstChild?.textContent?.trim();
  if (label) btn.setAttribute("aria-label", label);
  const sync = () => {
    const text = opts.pulldown ? opts.pulldown.label : (select.selectedOptions[0]?.textContent ?? "");
    btn.innerHTML = `${opts.pulldown?.icon ? `<span class="menu-icon">${opts.pulldown.icon}</span>` : ""}<span class="menu-button-label"></span>${ICON_CHEVRON}`;
    btn.querySelector(".menu-button-label")!.textContent = text;
  };
  sync();
  select.hidden = true;
  select.after(btn);
  btn.onclick = async () => {
    const items: MenuItem[] = [...select.options]
      .filter((o) => !(opts.pulldown && o.value === ""))
      .map((o) => ({ value: o.value, label: o.textContent ?? "", disabled: o.disabled, shortcut: opts.shortcuts?.[o.value] }));
    const v = await openMenu(btn, items, opts.pulldown ? {} : { checked: select.value, listbox: true });
    if (v === null) return;
    select.value = v;
    select.dispatchEvent(new Event("change", { bubbles: true }));
    sync();
  };
  // Programmatische Wertänderungen (z. B. beim Öffnen des Sheets) übernehmen.
  select.addEventListener("change", sync);
  return { sync };
}
