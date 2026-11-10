import type { ITheme } from "@xterm/xterm";

/** Farbschemata für die Shell (Werte aus den bekannten offenen Schemata). */
const pal = (p: string[]) => ({
  black: p[0], red: p[1], green: p[2], yellow: p[3], blue: p[4], magenta: p[5], cyan: p[6], white: p[7],
  brightBlack: p[8], brightRed: p[9], brightGreen: p[10], brightYellow: p[11], brightBlue: p[12], brightMagenta: p[13], brightCyan: p[14], brightWhite: p[15],
});
const solarized = pal(["#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5", "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3"]);
const tango = pal(["#2e3436", "#cc0000", "#4e9a06", "#c4a000", "#3465a4", "#75507b", "#06989a", "#d3d7cf", "#555753", "#ef2929", "#8ae234", "#fce94f", "#729fcf", "#ad7fa8", "#34e2e2", "#eeeeec"]);

// Standardpaletten der bekannten Terminals (Originalwerte), damit sich alle wie zu Hause fühlen.
const campbell = pal(["#0c0c0c", "#c50f1f", "#13a10e", "#c19c00", "#0037da", "#881798", "#3a96dd", "#cccccc", "#767676", "#e74856", "#16c60c", "#f9f1a5", "#3b78ff", "#b4009e", "#61d6d6", "#f2f2f2"]);
const conhost = pal(["#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#c0c0c0", "#808080", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff"]);
const macTerminal = pal(["#000000", "#990000", "#00a600", "#999900", "#0000b2", "#b200b2", "#00a6b2", "#bfbfbf", "#666666", "#e50000", "#00d900", "#e5e500", "#0000ff", "#e500e5", "#00e5e5", "#e5e5e5"]);
const gnome = pal(["#171421", "#c01c28", "#26a269", "#a2734c", "#12488b", "#a347ba", "#2aa1b3", "#d0cfcc", "#5e5c64", "#f66151", "#33da7a", "#e9ad0c", "#2a7bde", "#c061cb", "#33c7de", "#ffffff"]);
const vga = pal(["#000000", "#aa0000", "#00aa00", "#aa5500", "#0000aa", "#aa00aa", "#00aaaa", "#aaaaaa", "#555555", "#ff5555", "#55ff55", "#ffff55", "#5555ff", "#ff55ff", "#55ffff", "#ffffff"]);
const WIN_FONT = "'Cascadia Mono', Consolas";
const MAC_FONT = "'SF Mono', Menlo";
const LINUX_FONT = "'Ubuntu Mono', 'DejaVu Sans Mono', 'JetBrains Mono Variable'";

export const TERM_THEMES: Record<string, { label: string; theme: ITheme; font?: string }> = {
  "windows-terminal": { label: "Windows Terminal (Campbell)", font: WIN_FONT, theme: { background: "#0c0c0c", foreground: "#cccccc", cursor: "#ffffff", selectionBackground: "#ffffff40", ...campbell } },
  cmd: { label: "Eingabeaufforderung klassisch", font: "Consolas", theme: { background: "#000000", foreground: "#c0c0c0", cursor: "#c0c0c0", selectionBackground: "#c0c0c060", ...conhost } },
  powershell: { label: "PowerShell", font: WIN_FONT, theme: { background: "#012456", foreground: "#cccccc", cursor: "#ffffff", selectionBackground: "#ffffff40", ...campbell } },
  "mac-basic": { label: "macOS Terminal Standard", font: MAC_FONT, theme: { background: "#ffffff", foreground: "#000000", cursor: "#7f7f7f", selectionBackground: "#a4cdff", ...macTerminal } },
  "mac-pro": { label: "macOS Terminal Pro", font: MAC_FONT, theme: { background: "#000000", foreground: "#f2f2f2", cursor: "#4d4d4d", selectionBackground: "#414141", ...macTerminal } },
  ubuntu: { label: "Ubuntu", font: LINUX_FONT, theme: { background: "#300a24", foreground: "#ffffff", cursor: "#ffffff", selectionBackground: "#ffffff40", ...tango } },
  gnome: { label: "GNOME Terminal Standard", font: LINUX_FONT, theme: { background: "#171421", foreground: "#d0cfcc", cursor: "#d0cfcc", selectionBackground: "#d0cfcc40", ...gnome } },
  "linux-console": { label: "Linux-Konsole klassisch", font: LINUX_FONT, theme: { background: "#000000", foreground: "#aaaaaa", cursor: "#aaaaaa", selectionBackground: "#aaaaaa60", ...vga } },
  "sesam-dark": { label: "Sesam Dunkel", theme: { background: "#161618", foreground: "#e5e5ea", cursor: "#4cd08f", selectionBackground: "#4cd08f55", ...tango } },
  "sesam-light": { label: "Sesam Hell", theme: { background: "#fbfbfb", foreground: "#1a1a1a", cursor: "#157a5a", selectionBackground: "#157a5a33", ...tango, black: "#1a1a1a", white: "#c0c0c0", brightWhite: "#6e6e73" } },
  "solarized-dark": { label: "Solarized Dunkel", theme: { background: "#002b36", foreground: "#839496", cursor: "#93a1a1", ...solarized } },
  "solarized-light": { label: "Solarized Hell", theme: { background: "#fdf6e3", foreground: "#657b83", cursor: "#586e75", ...solarized } },
  dracula: {
    label: "Dracula",
    theme: { background: "#282a36", foreground: "#f8f8f2", cursor: "#f8f8f2", selectionBackground: "#44475a", ...pal(["#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2", "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff"]) },
  },
  nord: {
    label: "Nord",
    theme: { background: "#2e3440", foreground: "#d8dee9", cursor: "#d8dee9", selectionBackground: "#434c5e", ...pal(["#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0", "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4"]) },
  },
  "one-dark": {
    label: "One Dark",
    theme: { background: "#282c34", foreground: "#abb2bf", cursor: "#528bff", selectionBackground: "#3e4451", ...pal(["#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf", "#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff"]) },
  },
  "gruvbox-dark": {
    label: "Gruvbox Dunkel",
    theme: { background: "#282828", foreground: "#ebdbb2", cursor: "#ebdbb2", selectionBackground: "#504945", ...pal(["#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984", "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2"]) },
  },
  tango: { label: "Tango", theme: { background: "#2e3436", foreground: "#d3d7cf", cursor: "#d3d7cf", ...tango } },
  // Weitere offene Schemata (Lizenzen: docs/lizenzen/README.md), Werte aus den Originalen.
  "one-light": {
    label: "One Light",
    theme: { background: "#fafafa", foreground: "#383a42", cursor: "#526fff", selectionBackground: "#e5e5e6", ...pal(["#383a42", "#e45649", "#50a14f", "#c18401", "#0184bc", "#a626a4", "#0997b3", "#fafafa", "#4f525e", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff"]) },
  },
  "gruvbox-light": {
    label: "Gruvbox Hell",
    theme: { background: "#fbf1c7", foreground: "#3c3836", cursor: "#3c3836", selectionBackground: "#d5c4a1", ...pal(["#fbf1c7", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#7c6f64", "#928374", "#9d0006", "#79740e", "#b57614", "#076678", "#8f3f71", "#427b58", "#3c3836"]) },
  },
  "tokyo-night": {
    label: "Tokyo Night",
    theme: { background: "#1a1b26", foreground: "#c0caf5", cursor: "#c0caf5", selectionBackground: "#283457", ...pal(["#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6", "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5"]) },
  },
  "tokyo-night-day": {
    label: "Tokyo Night Day",
    theme: { background: "#e1e2e7", foreground: "#3760bf", cursor: "#3760bf", selectionBackground: "#b7c1e3", ...pal(["#e9e9ed", "#f52a65", "#587539", "#8c6c3e", "#2e7de9", "#9854f1", "#007197", "#6172b0", "#a1a6c5", "#f52a65", "#587539", "#8c6c3e", "#2e7de9", "#9854f1", "#007197", "#3760bf"]) },
  },
  "catppuccin-mocha": {
    label: "Catppuccin Mocha",
    theme: { background: "#1e1e2e", foreground: "#cdd6f4", cursor: "#f5e0dc", selectionBackground: "#585b70", ...pal(["#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de", "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8"]) },
  },
  "catppuccin-latte": {
    label: "Catppuccin Latte",
    theme: { background: "#eff1f5", foreground: "#4c4f69", cursor: "#dc8a78", selectionBackground: "#acb0be", ...pal(["#5c5f77", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#acb0be", "#6c6f85", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#bcc0cc"]) },
  },
  "github-dark": {
    label: "GitHub Dunkel",
    theme: { background: "#0d1117", foreground: "#e6edf3", cursor: "#2f81f7", selectionBackground: "#264f78", ...pal(["#484f58", "#ff7b72", "#3fb950", "#d29922", "#58a6ff", "#bc8cff", "#39c5cf", "#b1bac4", "#6e7681", "#ffa198", "#56d364", "#e3b341", "#79c0ff", "#d2a8ff", "#56d4dd", "#ffffff"]) },
  },
  "github-light": {
    label: "GitHub Hell",
    theme: { background: "#ffffff", foreground: "#1f2328", cursor: "#0969da", selectionBackground: "#b6e3ff", ...pal(["#24292f", "#cf222e", "#116329", "#4d2d00", "#0969da", "#8250df", "#1b7c83", "#6e7781", "#57606a", "#a40e26", "#1a7f37", "#633c01", "#218bff", "#a475f9", "#3192aa", "#8c959f"]) },
  },
};

/** «Automatisch»-Designs: je ein helles und ein dunkles, das dem Erscheinungsbild folgt. */
export const AUTO_PAIRS: Record<string, { label: string; light: string; dark: string }> = {
  auto: { label: "Sesam (automatisch)", light: "sesam-light", dark: "sesam-dark" },
  "auto:mac": { label: "macOS Terminal (automatisch)", light: "mac-basic", dark: "mac-pro" },
  "auto:solarized": { label: "Solarized (automatisch)", light: "solarized-light", dark: "solarized-dark" },
  "auto:one": { label: "One (automatisch)", light: "one-light", dark: "one-dark" },
  "auto:gruvbox": { label: "Gruvbox (automatisch)", light: "gruvbox-light", dark: "gruvbox-dark" },
  "auto:tokyo": { label: "Tokyo Night (automatisch)", light: "tokyo-night-day", dark: "tokyo-night" },
  "auto:catppuccin": { label: "Catppuccin (automatisch)", light: "catppuccin-latte", dark: "catppuccin-mocha" },
  "auto:github": { label: "GitHub (automatisch)", light: "github-light", dark: "github-dark" },
};

/** Ist ein Hintergrund dunkel? (relative Helligkeit nach WCAG) */
export function isDark(bg = "#000000") {
  const n = parseInt(bg.slice(1, 7), 16);
  const lin = (c: number) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(n >> 16) + 0.7152 * lin((n >> 8) & 255) + 0.0722 * lin(n & 255) < 0.4;
}

// ---------- Eigene Designs ----------
const CUSTOM_KEY = "sesam.customThemes";
type Custom = Record<string, { label: string; theme: ITheme }>;
const customs = (): Custom => {
  try {
    return JSON.parse(localStorage.getItem(CUSTOM_KEY) ?? "{}");
  } catch {
    return {};
  }
};
/** Eigene Designs in die Liste aufnehmen (beim Start und nach Änderungen). */
export function loadCustomThemes() {
  for (const k of Object.keys(TERM_THEMES)) if (k.startsWith("custom:")) delete TERM_THEMES[k];
  Object.assign(TERM_THEMES, customs());
}
export function saveCustomTheme(id: string, label: string, theme: ITheme) {
  const all = customs();
  all[id] = { label, theme };
  localStorage.setItem(CUSTOM_KEY, JSON.stringify(all));
  loadCustomThemes();
  dispatchEvent(new CustomEvent("sesam:terminal-settings"));
}
export function deleteCustomTheme(id: string) {
  const all = customs();
  delete all[id];
  localStorage.setItem(CUSTOM_KEY, JSON.stringify(all));
  loadCustomThemes();
  dispatchEvent(new CustomEvent("sesam:terminal-settings"));
}
loadCustomThemes();
addEventListener("storage", (e) => e.key === CUSTOM_KEY && (loadCustomThemes(), dispatchEvent(new CustomEvent("sesam:terminal-settings"))));

const ANSI_NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite"] as const;
const hex = (n: number) => Math.round(Math.max(0, Math.min(1, n)) * 255).toString(16).padStart(2, "0");

/** .itermcolors (XML-Plist, Farben als 0–1-Komponenten) in ein Design. */
export function parseItermcolors(xml: string): ITheme {
  const doc = new DOMParser().parseFromString(xml, "application/xml");
  const top = doc.querySelector("plist > dict");
  if (!top) throw new Error("Keine .itermcolors-Datei");
  const color = (key: string) => {
    const k = [...top.children].find((e) => e.tagName === "key" && e.textContent === key);
    const d = k?.nextElementSibling;
    if (!d || d.tagName !== "dict") return undefined;
    const comp = (n: string) => Number([...d.children].find((e) => e.tagName === "key" && e.textContent === n)?.nextElementSibling?.textContent ?? "0");
    return `#${hex(comp("Red Component"))}${hex(comp("Green Component"))}${hex(comp("Blue Component"))}`;
  };
  const t: Record<string, string | undefined> = { background: color("Background Color"), foreground: color("Foreground Color"), cursor: color("Cursor Color"), selectionBackground: color("Selection Color") };
  ANSI_NAMES.forEach((n, i) => (t[n] = color(`Ansi ${i} Color`)));
  if (!t.background || !t.foreground) throw new Error("Hintergrund oder Vordergrund fehlt");
  return t as ITheme;
}

/** Windows-Terminal-Schema (JSON aus «schemes»; Namen wie purple, cursorColor). */
export function parseWindowsTerminal(json: string): { name: string; theme: ITheme } {
  const o = JSON.parse(json);
  const s = Array.isArray(o.schemes) ? o.schemes[0] : o;
  const map: Record<string, string> = { purple: "magenta", brightPurple: "brightMagenta", cursorColor: "cursor", selectionBackground: "selectionBackground", background: "background", foreground: "foreground" };
  const t: Record<string, string> = {};
  for (const [k, v] of Object.entries(s)) if (typeof v === "string" && /^#[0-9a-f]{6}$/i.test(v)) t[map[k] ?? k] = v;
  if (!t.background || !t.foreground) throw new Error("Kein Windows-Terminal-Schema");
  return { name: String(s.name ?? "Importiert"), theme: t as ITheme };
}

/** Als Windows-Terminal-Schema ausgeben (gängig, von vielen Programmen gelesen). */
export function exportWindowsTerminal(name: string, t: ITheme) {
  const back: Record<string, string> = { magenta: "purple", brightMagenta: "brightPurple", cursor: "cursorColor" };
  const o: Record<string, string> = { name };
  for (const k of ["background", "foreground", "cursor", "selectionBackground", ...ANSI_NAMES]) if (t[k as keyof ITheme]) o[back[k] ?? k] = String(t[k as keyof ITheme]);
  return JSON.stringify(o, null, 2);
}

/** "auto" = Sesam Hell/Dunkel nach System. */
export function resolveTheme(id: string): ITheme {
  const dark = matchMedia("(prefers-color-scheme: dark)").matches;
  const pair = AUTO_PAIRS[id];
  if (pair) return resolveTheme(dark ? pair.dark : pair.light);
  if (!TERM_THEMES[id]) return TERM_THEMES[dark ? "sesam-dark" : "sesam-light"].theme;
  // Wie Terminal.app: das Profil «Standard» folgt dem Erscheinungsbild des Systems.
  if (id === "mac-basic" && dark) return { ...TERM_THEMES[id].theme, background: "#1e1e1e", foreground: "#ffffff", cursor: "#a0a0a0", selectionBackground: "#3f638b" };
  return TERM_THEMES[id].theme;
}

/** Vorgeschlagene Schrift eines Designs (gilt, solange keine eigene Schriftart eingestellt ist). */
export const themeFont = (id: string) => TERM_THEMES[AUTO_PAIRS[id] ? AUTO_PAIRS[id].dark : id]?.font ?? "";

export interface TermSettings {
  theme: string;
  fontSize: number;
  fontFamily: string;
}

const KEY = "sesam.terminal";
export function termSettings(): TermSettings {
  try {
    return { theme: "auto", fontSize: 13, fontFamily: "", ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { theme: "auto", fontSize: 13, fontFamily: "" };
  }
}
export function saveTermSettings(s: TermSettings) {
  localStorage.setItem(KEY, JSON.stringify(s));
  dispatchEvent(new CustomEvent("sesam:terminal-settings"));
}
