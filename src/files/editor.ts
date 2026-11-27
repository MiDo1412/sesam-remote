// Eingebauter Editor für Textdateien auf dem Server (auch iPad/iPhone): bearbeiten, vor dem Sichern die Änderungen
// zeilenweise ansehen, dann hochladen. Für Konfigurationsdateien gedacht, nicht als ausgewachsene IDE.
import { hunks, lineDiff } from "./diff";
import { ask } from "../ui";

/** Öffnet `text` zum Bearbeiten. `save` bekommt den neuen Text (gleiche Zeilenenden wie vorher) und lädt hoch. */
export function editText(title: string, text: string, save: (text: string) => Promise<void>): Promise<void> {
  const crlf = text.includes("\r\n");
  const before = crlf ? text.split("\r\n").join("\n") : text;
  const dlg = document.createElement("dialog");
  dlg.className = "text-editor";
  dlg.setAttribute("aria-label", title);
  const head = Object.assign(document.createElement("h2"), { textContent: title });
  const area = Object.assign(document.createElement("textarea"), { className: "mono", value: before, spellcheck: false, wrap: "off" });
  area.setAttribute("autocapitalize", "off");
  area.setAttribute("autocorrect", "off");
  const diffView = Object.assign(document.createElement("pre"), { className: "text-diff mono", hidden: true });
  const note = Object.assign(document.createElement("p"), { className: "note" });
  const cancel = Object.assign(document.createElement("button"), { type: "button", textContent: "Abbrechen" });
  const back = Object.assign(document.createElement("button"), { type: "button", textContent: "Weiter bearbeiten", hidden: true });
  const ok = Object.assign(document.createElement("button"), { type: "button", className: "primary", textContent: "Änderungen ansehen" });
  const actions = Object.assign(document.createElement("div"), { className: "row actions" });
  actions.append(note, cancel, back, ok);
  dlg.append(head, area, diffView, actions);
  document.body.append(dlg);

  // Tabulator rückt ein statt den Fokus zu verlassen (Konfigurationsdateien, Makefiles).
  area.onkeydown = (e) => {
    if (e.key === "Tab" && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      area.setRangeText("\t", area.selectionStart, area.selectionEnd, "end");
    }
  };
  const showDiff = (on: boolean) => {
    area.hidden = on;
    diffView.hidden = !on;
    back.hidden = !on;
    ok.textContent = on ? "Hochladen" : "Änderungen ansehen";
    if (!on) return area.focus();
    const d = lineDiff(before, area.value);
    const added = d.filter((l) => l.op === "+").length;
    const removed = d.filter((l) => l.op === "-").length;
    note.textContent = `${added} Zeile(n) neu, ${removed} entfernt`;
    diffView.replaceChildren(
      ...hunks(d).map((l) => {
        const row = document.createElement("span");
        row.className = l ? (l.op === "+" ? "add" : l.op === "-" ? "del" : "") : "gap";
        row.textContent = l ? `${l.op} ${l.text}` : "…";
        return row;
      }),
    );
    ok.focus();
  };
  return new Promise((resolve) => {
    const finish = () => (dlg.close(), dlg.remove(), resolve());
    const leave = async () => {
      if (area.value === before || (await ask("Änderungen verwerfen?", "Was du geändert hast, geht verloren.", "Verwerfen"))) finish();
    };
    dlg.oncancel = (e) => (e.preventDefault(), void leave());
    cancel.onclick = () => void leave();
    back.onclick = () => showDiff(false);
    ok.onclick = async () => {
      if (area.value === before) return finish();
      if (area.hidden) {
        ok.disabled = true;
        note.textContent = "Lädt hoch …";
        try {
          await save(crlf ? area.value.split("\n").join("\r\n") : area.value);
          finish();
        } catch (e) {
          note.textContent = `Nicht gesichert: ${e}`;
          ok.disabled = false;
        }
      } else showDiff(true);
    };
    dlg.showModal();
    area.focus();
  });
}
