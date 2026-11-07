/** Entfernte Auflösung aus der Anzeigefläche: in Bildschirmpixeln (HiDPI), gerade Werte, sinnvolle Mindestgrösse.
 *  Hochformat: 9:16 in der Höhe der Fläche (mindestens 720 × 1280), statt Breite und Höhe zu tauschen –
 *  ein Tausch machte aus einem flachen Fenster einen 300 Pixel schmalen Desktop. */
export function remoteSize(cssW: number, cssH: number, dpr: number, orientation: "landscape" | "portrait"): [number, number] {
  const even = (n: number) => Math.min(8192, Math.round(n)) & ~1;
  const w = cssW * dpr, h = cssH * dpr;
  if (orientation === "portrait") {
    const tall = Math.max(h, 1280);
    return [even(tall * 9 / 16), even(tall)];
  }
  return [even(Math.max(w, 640)), even(Math.max(h, 480))];
}

// Selbstprüfung im Entwicklungsbuild (Fall aus Mikes Log: Fenster 1296×300 CSS-Pixel, Hochformat).
if (import.meta.env.DEV) {
  const [pw, ph] = remoteSize(1296, 300, 2, "portrait");
  console.assert(pw === 720 && ph === 1280, "Hochformat nie schmaler als 720", pw, ph);
  const [lw, lh] = remoteSize(1288, 860, 2, "landscape");
  console.assert(lw === 2576 && lh === 1720, "HiDPI in Bildschirmpixeln", lw, lh);
  console.assert(remoteSize(100, 100, 1, "landscape").join() === "640,480", "Mindestgrösse");
}
