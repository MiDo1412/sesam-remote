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
};

/** "auto" = Sesam Hell/Dunkel nach System. */
export function resolveTheme(id: string): ITheme {
  if (id === "auto" || !TERM_THEMES[id]) {
    return TERM_THEMES[matchMedia("(prefers-color-scheme: dark)").matches ? "sesam-dark" : "sesam-light"].theme;
  }
  return TERM_THEMES[id].theme;
}

/** Vorgeschlagene Schrift eines Designs (gilt, solange keine eigene Schriftart eingestellt ist). */
export const themeFont = (id: string) => TERM_THEMES[id]?.font ?? "";

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
