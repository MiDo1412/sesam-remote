/** Zentrale Symbol-Schicht: ein Name je Aktion (ICON.refresh …), Quelle je Plattform.
 *  Windows und Linux: Fluent System Icons von Microsoft (MIT, @fluentui/svg-icons), Regular 20 px,
 *  als Inline-SVG mit currentColor; nur die gebrauchten Dateien kommen ins Bundle. macOS: SF Symbols (unten).
 *  Betriebssystem-Logos: siehe unten. Lizenzen: docs/lizenzen/README.md. */
import fAdd from "@fluentui/svg-icons/icons/add_20_regular.svg?raw";
import fArrowClockwise from "@fluentui/svg-icons/icons/arrow_clockwise_20_regular.svg?raw";
import fArrowDownload from "@fluentui/svg-icons/icons/arrow_download_20_regular.svg?raw";
import fArrowLeft from "@fluentui/svg-icons/icons/arrow_left_20_regular.svg?raw";
import fArrowMaximize from "@fluentui/svg-icons/icons/arrow_maximize_20_regular.svg?raw";
import fArrowRight from "@fluentui/svg-icons/icons/arrow_right_20_regular.svg?raw";
import fArrowSwap from "@fluentui/svg-icons/icons/arrow_swap_20_regular.svg?raw";
import fArrowUp from "@fluentui/svg-icons/icons/arrow_up_20_regular.svg?raw";
import fArrowUpload from "@fluentui/svg-icons/icons/arrow_upload_20_regular.svg?raw";
import fCamera from "@fluentui/svg-icons/icons/camera_20_regular.svg?raw";
import fClipboardTextLtr from "@fluentui/svg-icons/icons/clipboard_text_ltr_20_regular.svg?raw";
import fCopy from "@fluentui/svg-icons/icons/copy_20_regular.svg?raw";
import fDelete from "@fluentui/svg-icons/icons/delete_20_regular.svg?raw";
import fDesktop from "@fluentui/svg-icons/icons/desktop_20_regular.svg?raw";
import fDismiss from "@fluentui/svg-icons/icons/dismiss_20_regular.svg?raw";
import fEdit from "@fluentui/svg-icons/icons/edit_20_regular.svg?raw";
import fFlash from "@fluentui/svg-icons/icons/flash_20_regular.svg?raw";
import fFolder from "@fluentui/svg-icons/icons/folder_20_regular.svg?raw";
import fFolderAdd from "@fluentui/svg-icons/icons/folder_add_20_regular.svg?raw";
import fFullScreenMaximize from "@fluentui/svg-icons/icons/full_screen_maximize_20_regular.svg?raw";
import fFullScreenMinimize from "@fluentui/svg-icons/icons/full_screen_minimize_20_regular.svg?raw";
import fGlobe from "@fluentui/svg-icons/icons/globe_20_regular.svg?raw";
import fGrid from "@fluentui/svg-icons/icons/grid_20_regular.svg?raw";
import fHandLeft from "@fluentui/svg-icons/icons/hand_left_20_regular.svg?raw";
import fHome from "@fluentui/svg-icons/icons/home_20_regular.svg?raw";
import fKeyboard from "@fluentui/svg-icons/icons/keyboard_20_regular.svg?raw";
import fMoreHorizontal from "@fluentui/svg-icons/icons/more_horizontal_20_regular.svg?raw";
import fOpen from "@fluentui/svg-icons/icons/open_20_regular.svg?raw";
import fPin from "@fluentui/svg-icons/icons/pin_20_regular.svg?raw";
import fPlugDisconnected from "@fluentui/svg-icons/icons/plug_disconnected_20_regular.svg?raw";
import fQuestionCircle from "@fluentui/svg-icons/icons/question_circle_20_regular.svg?raw";
import fSave from "@fluentui/svg-icons/icons/save_20_regular.svg?raw";
import fSearch from "@fluentui/svg-icons/icons/search_20_regular.svg?raw";
import fSettings from "@fluentui/svg-icons/icons/settings_20_regular.svg?raw";
import fShareScreenStart from "@fluentui/svg-icons/icons/share_screen_start_20_regular.svg?raw";
import fSplitHorizontal from "@fluentui/svg-icons/icons/split_horizontal_20_regular.svg?raw";
import fSplitVertical from "@fluentui/svg-icons/icons/split_vertical_20_regular.svg?raw";
import fSubtract from "@fluentui/svg-icons/icons/subtract_20_regular.svg?raw";
import fTextBulletListLtr from "@fluentui/svg-icons/icons/text_bullet_list_ltr_20_regular.svg?raw";
import fTextFont from "@fluentui/svg-icons/icons/text_font_20_regular.svg?raw";
import fWindowConsole from "@fluentui/svg-icons/icons/window_console_20_regular.svg?raw";
import fZoomIn from "@fluentui/svg-icons/icons/zoom_in_20_regular.svg?raw";
// Betriebssystem-Logos einfarbig aus Simple Icons (CC0), gleiche 24-px-Box wie die Fluent-Symbole.
import linuxLogo from "simple-icons/icons/linux.svg?raw";
import appleLogo from "simple-icons/icons/apple.svg?raw";
import ubuntuLogo from "simple-icons/icons/ubuntu.svg?raw";
import piLogo from "simple-icons/icons/raspberrypi.svg?raw";
import desktopTower from "@fluentui/svg-icons/icons/desktop_tower_20_regular.svg?raw";
/** Windows: Simple Icons führt das Logo nicht mehr (Markenrecht); das schlichte Vierfelder-Zeichen selbst gezeichnet. */
const windowsLogo = `<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M2 2h9.5v9.5H2zm10.5 0H22v9.5h-9.5zM2 12.5h9.5V22H2zm10.5 0H22V22h-9.5z"/></svg>`;
/** Simple-Icons-SVG ohne <title> (sonst erscheint beim Darüberfahren ein Browser-Tooltip). */
const logo = (s: string, n: number) => fl(s.replace(/<title>[^<]*<\/title>/, ""), n);

/** Fluent-SVGs haben eigene width/height und keine Füllfarbe: ersetzen bzw. currentColor setzen. */
export const fl = (s: string, n = 16) => s.replace(/<svg[^>]*?(viewBox="[^"]+")[^>]*>/, `<svg xmlns="http://www.w3.org/2000/svg" $1 width="${n}" height="${n}" fill="currentColor" aria-hidden="true" focusable="false">`);

export const ICON = {
  refresh: fl(fArrowClockwise),
  back: fl(fArrowLeft),
  forward: fl(fArrowRight),
  up: fl(fArrowUp),
  compare: fl(fArrowSwap),
  home: fl(fHome),
  detach: fl(fOpen),
  span: fl(fArrowMaximize),
  screenshot: fl(fCamera),
  typeClipboard: fl(fClipboardTextLtr),
  copy: fl(fCopy),
  download: fl(fArrowDownload),
  folder: fl(fFolder),
  newFolder: fl(fFolderAdd),
  fullscreen: fl(fFullScreenMaximize),
  globe: fl(fGlobe),
  handTap: fl(fHandLeft),
  keyboard: fl(fKeyboard),
  quick: fl(fFlash),
  save: fl(fSave),
  search: fl(fSearch),
  magnify: fl(fZoomIn),
  desktop: fl(fDesktop),
  rename: fl(fEdit),
  disconnect: fl(fPlugDisconnected),
  plus: fl(fAdd),
  shell: fl(fWindowConsole),
  font: fl(fTextFont),
  trash: fl(fDelete),
  upload: fl(fArrowUpload),
  close: fl(fDismiss, 12),
  minimize: fl(fSubtract, 12),
  splitRight: fl(fSplitVertical),
  splitDown: fl(fSplitHorizontal),
  screencast: fl(fShareScreenStart),
  question: fl(fQuestionCircle, 14),
  viewGrid: fl(fGrid),
  viewList: fl(fTextBulletListLtr),
  settings: fl(fSettings),
  pin: fl(fPin),
  windowed: fl(fFullScreenMinimize),
  more: fl(fMoreHorizontal),
  osLinux: logo(linuxLogo, 40),
  osUbuntu: logo(ubuntuLogo, 40),
  osMac: logo(appleLogo, 40),
  osWindows: logo(windowsLogo, 40),
  osPi: logo(piLogo, 40),
  osGeneric: fl(desktopTower, 40),
};

/** macOS/iPadOS: dieselben Namen als SF-Symbol-Namen (nur auf Apple-Geräten erlaubt). */
export const SF_SYMBOL: Partial<Record<keyof typeof ICON, string>> = {
  refresh: "arrow.clockwise", back: "chevron.backward", forward: "chevron.forward", up: "arrow.up", compare: "arrow.left.arrow.right",
  home: "house", detach: "macwindow.on.rectangle", span: "rectangle.split.3x1", screenshot: "camera", typeClipboard: "doc.on.clipboard",
  copy: "doc.on.doc", download: "arrow.down.circle", folder: "folder", newFolder: "folder.badge.plus", fullscreen: "arrow.up.left.and.arrow.down.right",
  globe: "globe", handTap: "hand.tap", keyboard: "keyboard", quick: "bolt", save: "square.and.arrow.down", search: "magnifyingglass",
  magnify: "plus.magnifyingglass", desktop: "display", rename: "pencil", disconnect: "powerplug", plus: "plus", shell: "terminal",
  font: "textformat", trash: "trash", upload: "arrow.up.circle", close: "xmark", minimize: "minus", splitRight: "rectangle.split.2x1",
  splitDown: "rectangle.split.1x2", screencast: "rectangle.on.rectangle", question: "questionmark.circle", viewGrid: "square.grid.2x2",
  viewList: "list.bullet", settings: "gearshape", pin: "pin", windowed: "arrow.down.right.and.arrow.up.left", more: "ellipsis",
};

/** macOS: SF Symbols statt Fluent. Das System zeichnet sie (src-tauri/src/sfsymbol.rs); als leeres <svg> mit Maske,
 *  damit Grössen- und Farbregeln für «svg» in styles.css weiter greifen. */
export const onApple = navigator.userAgent.includes("Mac");
export const sf = (name: string, n: number | string = 16) =>
  `<svg class="sf" xmlns="http://www.w3.org/2000/svg" width="${n}" height="${n}" style="--sf:url('sfsymbol://localhost/${name}?${n}')" aria-hidden="true" focusable="false"></svg>`;
if (onApple) for (const [k, name] of Object.entries(SF_SYMBOL) as [keyof typeof ICON, string][]) ICON[k] = sf(name, /width="(\d+)"/.exec(ICON[k])?.[1]);
