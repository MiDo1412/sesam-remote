/** Zwischenablage-Verlauf über alle Sitzungen: nur im Arbeitsspeicher (nie auf die Platte, es können Passwörter darunter sein),
 *  höchstens 25 Texte. Solange Sesam den Fokus hat, wird neue Zwischenablage alle 2 s übernommen (auch Kopien vom entfernten Rechner). */
import { api } from "./api";

const MAX = 25;
const history: string[] = [];

/** Neuen Text vorne einreihen (Duplikate nach vorne, leere und riesige Texte nicht). */
export function remember(text: string) {
  const t = text.replace(/\r\n/g, "\n");
  if (!t.trim() || t.length > 20000) return;
  const i = history.indexOf(t);
  if (i >= 0) history.splice(i, 1);
  history.unshift(t);
  history.length = Math.min(history.length, MAX);
}

export const clipHistory = () => [...history];

export function initClipHistory() {
  let last = "";
  // iPad: jedes Lesen der Zwischenablage fragt um Erlaubnis, darum dort kein laufender Verlauf.
  if (/iPad|iPhone|Android/.test(navigator.userAgent) || (navigator.userAgent.includes("Mac") && navigator.maxTouchPoints > 1)) return;
  setInterval(async () => {
    if (!document.hasFocus()) return;
    const t = await api.clipRead().catch(() => "");
    if (t && t !== last) remember((last = t));
  }, 2000);
}

// Selbstprüfung im Entwicklungsbuild
if (import.meta.env?.DEV) {
  remember("a");
  remember("b");
  remember("a");
  console.assert(history[0] === "a" && history[1] === "b" && history.length === 2, "clipHistory");
  history.length = 0;
}
