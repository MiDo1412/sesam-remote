window.__desktopCanvas = (theme = window.__deskTheme || "ubuntu") => {
  const W = window.__deskW || 2880, H = window.__deskH || 1700, cv = document.createElement("canvas"); cv.width = W; cv.height = H;
  const g = cv.getContext("2d");
  const T = {
    ubuntu: ["#2c0f2a", "#77216f", "#e95420"], windows: ["#0b2a5b", "#1f5fbf", "#7fb2f0"],
    mac: ["#1d3b53", "#3f7c85", "#f0b67f"], pi: ["#3a0d1c", "#8c1b3f", "#c7375f"],
  }[theme];
  const bg = g.createLinearGradient(0, 0, W, H);
  bg.addColorStop(0, T[0]); bg.addColorStop(.55, T[1]); bg.addColorStop(1, T[2]);
  g.fillStyle = bg; g.fillRect(0, 0, W, H);
  g.globalAlpha = .16; g.fillStyle = "#fff";
  g.beginPath(); g.arc(W*.8, H*.25, H*.36, 0, 7); g.fill();
  g.beginPath(); g.arc(W*.15, H*.95, H*.3, 0, 7); g.fill(); g.globalAlpha = 1;
  const F = (w, s) => `${w} ${s}px Inter, sans-serif`;
  let x = W*.13, y = H*.11, w = W*.74, h = H*.74, title = "Wetterstation – Dashboard", accent = "#e95420";
  if (theme === "ubuntu" || theme === "pi") {
    g.fillStyle = "#000"; g.fillRect(0, 0, W, 56);
    g.fillStyle = "#fff"; g.font = F(600, 26); g.textAlign = "center"; g.fillText("Mi 11. Nov.  16:20", W/2, 37);
    g.textAlign = "right"; g.font = F(400, 26); g.fillText("de   ⏻", W - 28, 37); g.textAlign = "left";
    if (theme === "ubuntu") {
      g.fillStyle = "rgba(20,20,20,.82)"; g.fillRect(0, 56, 128, H - 56);
      ["#e95420", "#3584e4", "#f6d32d", "#33d17a", "#9141ac", "#c0bfbc"].forEach((c, i) => { g.fillStyle = c; g.beginPath(); g.roundRect(28, 100 + i * 108, 72, 72, 18); g.fill(); });
    } else { accent = "#c51a4a"; title = "Thonny – messung.py"; }
  } else if (theme === "windows") {
    g.fillStyle = "rgba(240,240,245,.9)"; g.fillRect(0, H - 96, W, 96);
    ["#0078d4", "#f2b01e", "#107c10", "#5c2d91", "#e81123", "#00a4ef"].forEach((c, i) => { g.fillStyle = c; g.beginPath(); g.roundRect(W/2 - 330 + i*110, H - 76, 56, 56, 12); g.fill(); });
    g.fillStyle = "#222"; g.font = F(400, 24); g.textAlign = "right"; g.fillText("16:20", W - 40, H - 56); g.fillText("11.11.2026", W - 40, H - 24); g.textAlign = "left";
    accent = "#0078d4"; title = "Buchhaltung – Offene Posten.xlsx"; y = H*.07;
  } else if (theme === "mac") {
    g.fillStyle = "rgba(255,255,255,.55)"; g.fillRect(0, 0, W, 48);
    g.fillStyle = "#111"; g.font = F(600, 26); g.fillText("  Vorschau    Ablage    Bearbeiten    Darstellung", 30, 34);
    g.textAlign = "right"; g.font = F(400, 26); g.fillText("Mi. 11. Nov.  16:20", W - 30, 34); g.textAlign = "left";
    g.fillStyle = "rgba(255,255,255,.4)"; g.beginPath(); g.roundRect(W/2 - 420, H - 120, 840, 104, 30); g.fill();
    ["#0a84ff", "#30d158", "#ff9f0a", "#ff375f", "#bf5af2", "#8e8e93"].forEach((c, i) => { g.fillStyle = c; g.beginPath(); g.roundRect(W/2 - 380 + i*130, H - 104, 72, 72, 18); g.fill(); });
    accent = "#0a84ff"; title = "Fotoauswahl Herbst";
  }
  g.shadowColor = "rgba(0,0,0,.45)"; g.shadowBlur = 60; g.shadowOffsetY = 20;
  g.fillStyle = "#fafafa"; g.beginPath(); g.roundRect(x, y, w, h, 24); g.fill(); g.shadowColor = "transparent";
  g.fillStyle = "#ebebeb"; g.beginPath(); g.roundRect(x, y, w, 92, [24, 24, 0, 0]); g.fill();
  g.fillStyle = "#222"; g.font = F(600, 28); g.textAlign = "center"; g.fillText(title, x + w/2, y + 56); g.textAlign = "left";
  if (theme === "mac") ["#ff5f57", "#febc2e", "#28c840"].forEach((c, i) => { g.fillStyle = c; g.beginPath(); g.arc(x + 46 + i * 40, y + 46, 13, 0, 7); g.fill(); });
  else ["#999", "#999", accent].forEach((c, i) => { g.fillStyle = c; g.beginPath(); g.arc(x + w - 140 + i * 46, y + 46, 14, 0, 7); g.fill(); });
  g.fillStyle = "#f1f1f1"; g.fillRect(x, y + 92, 360, h - 92);
  g.font = F(400, 26);
  const side = theme === "windows" ? ["Übersicht", "Offene Posten", "Kreditoren", "Debitoren", "Auswertung"] : theme === "mac" ? ["Alle Fotos", "Favoriten", "Herbst", "Familie", "Importiert"] : ["Übersicht", "Temperatur", "Luftfeuchtigkeit", "Wind", "Niederschlag", "Export"];
  side.forEach((t, i) => {
    if (i === 1) { g.fillStyle = "#e1e1e1"; g.beginPath(); g.roundRect(x + 20, y + 130 + i * 70, 320, 56, 12); g.fill(); }
    g.fillStyle = "#333"; g.fillText(t, x + 50, y + 168 + i * 70);
  });
  const cx = x + 420, cw = w - 480;
  if (theme === "mac") {
    const cols = 5, s = (cw - 4 * 24) / cols, hues = [28, 200, 12, 140, 45, 260, 90, 330, 175, 20];
    for (let i = 0; i < 10; i++) { const gx = cx + (i % cols) * (s + 24), gy = y + 140 + Math.floor(i / cols) * (s + 24);
      const gr = g.createLinearGradient(gx, gy, gx + s, gy + s); gr.addColorStop(0, `hsl(${hues[i]} 60% 62%)`); gr.addColorStop(1, `hsl(${hues[i]+30} 55% 35%)`);
      g.fillStyle = gr; g.beginPath(); g.roundRect(gx, gy, s, s, 16); g.fill(); }
    return cv;
  }
  if (theme === "windows") {
    g.font = F(600, 26); const cols = ["Beleg", "Kunde", "Fällig", "Betrag CHF"]; const cwid = cw / 4;
    g.fillStyle = "#e8eef8"; g.fillRect(cx, y + 140, cw, 64);
    cols.forEach((c, i) => { g.fillStyle = "#1f3a60"; g.fillText(c, cx + 24 + i * cwid, y + 182); });
    g.font = F(400, 26);
    const rows = [["R-2026-311", "Bäckerei Huber", "15.11.2026", "1 240.00"], ["R-2026-312", "Garage Meier", "18.11.2026", "3 980.50"], ["R-2026-315", "Praxis Dr. Keller", "20.11.2026", "612.40"], ["R-2026-318", "Schreinerei Ott", "25.11.2026", "8 115.00"], ["R-2026-320", "Café Linde", "30.11.2026", "455.90"], ["R-2026-322", "Velo Sutter", "02.12.2026", "1 077.25"], ["R-2026-325", "Hotel Seeblick", "05.12.2026", "12 430.00"]];
    rows.forEach((r, j) => { if (j % 2) { g.fillStyle = "#f4f6fa"; g.fillRect(cx, y + 204 + j * 72, cw, 72); }
      r.forEach((c, i) => { g.fillStyle = "#222"; g.fillText(c, cx + 24 + i * cwid, y + 250 + j * 72); }); });
    return cv;
  }
  [["Aussen", "7,4 °C"], ["Feuchte", "81 %"], ["Wind", "12 km/h"]].forEach(([l, v], i) => {
    const bw = (cw - 80) / 3, bx = cx + i * (bw + 40);
    g.fillStyle = "#fff"; g.strokeStyle = "#e3e3e3"; g.lineWidth = 2; g.beginPath(); g.roundRect(bx, y + 140, bw, 180, 20); g.fill(); g.stroke();
    g.fillStyle = "#666"; g.font = F(400, 26); g.fillText(l, bx + 32, y + 196);
    g.fillStyle = "#111"; g.font = F(600, 64); g.fillText(v, bx + 32, y + 280);
  });
  const ox = cx, oy = y + 380, ch = h - 480;
  g.fillStyle = "#fff"; g.beginPath(); g.roundRect(ox, oy, cw, ch, 20); g.fill(); g.strokeStyle = "#e3e3e3"; g.stroke();
  g.fillStyle = "#111"; g.font = F(600, 30); g.fillText("Temperatur, letzte 24 Stunden", ox + 32, oy + 60);
  g.strokeStyle = "#eee"; for (let i = 0; i < 5; i++) { const ly = oy + 120 + i * (ch - 160) / 4; g.beginPath(); g.moveTo(ox + 32, ly); g.lineTo(ox + cw - 32, ly); g.stroke(); }
  g.strokeStyle = accent; g.lineWidth = 6; g.lineJoin = "round"; g.beginPath();
  for (let i = 0; i <= 48; i++) { const t = 4 * Math.sin((i - 14) / 48 * 2 * Math.PI) + Math.sin(i * 1.7) * .4;
    const px = ox + 40 + i * (cw - 80) / 48, py = oy + 120 + (ch - 160) / 2 - t * (ch - 160) / 10; i ? g.lineTo(px, py) : g.moveTo(px, py); }
  g.stroke();
  return cv;
};
window.__mockHandlers.thumb_get = async (a) => {
  const th = { "1":"mac", "2":"windows", "4":"ubuntu", "5":"pi" }[a.id]; if (!th) return null;
  const save = [window.__deskW, window.__deskH]; window.__deskW = 2880; window.__deskH = 1700;
  const big = window.__desktopCanvas(th); [window.__deskW, window.__deskH] = save;
  const cv = Object.assign(document.createElement("canvas"), { width: 720, height: 425 }); cv.getContext("2d").drawImage(big, 0, 0, 720, 425);
  const b = await new Promise(r => cv.toBlob(r, "image/jpeg", .85));
  return Array.from(new Uint8Array(await b.arrayBuffer()));
};
