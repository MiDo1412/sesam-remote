/** Phosphor Icons (MIT), nur die gebrauchten, als Inline-SVG mit currentColor. */
import arrowClockwise from "@phosphor-icons/core/assets/regular/arrow-clockwise.svg?raw";
import arrowSquareOut from "@phosphor-icons/core/assets/regular/arrow-square-out.svg?raw";
import arrowsOut from "@phosphor-icons/core/assets/regular/arrows-out.svg?raw";
import camera from "@phosphor-icons/core/assets/regular/camera.svg?raw";
import clipboardText from "@phosphor-icons/core/assets/regular/clipboard-text.svg?raw";
import copy from "@phosphor-icons/core/assets/regular/copy.svg?raw";
import downloadSimple from "@phosphor-icons/core/assets/regular/download-simple.svg?raw";
import folder from "@phosphor-icons/core/assets/regular/folder.svg?raw";
import folderPlus from "@phosphor-icons/core/assets/regular/folder-plus.svg?raw";
import handTap from "@phosphor-icons/core/assets/regular/hand-tap.svg?raw";
import frameCorners from "@phosphor-icons/core/assets/regular/frame-corners.svg?raw";
import keyboard from "@phosphor-icons/core/assets/regular/keyboard.svg?raw";
import linuxLogo from "@phosphor-icons/core/assets/regular/linux-logo.svg?raw";
import appleLogo from "@phosphor-icons/core/assets/regular/apple-logo.svg?raw";
import windowsLogo from "@phosphor-icons/core/assets/regular/windows-logo.svg?raw";
import cpu from "@phosphor-icons/core/assets/regular/cpu.svg?raw";
import desktopTower from "@phosphor-icons/core/assets/regular/desktop-tower.svg?raw";
import lightning from "@phosphor-icons/core/assets/regular/lightning.svg?raw";
import floppy from "@phosphor-icons/core/assets/regular/floppy-disk.svg?raw";
import magnifyingGlass from "@phosphor-icons/core/assets/regular/magnifying-glass.svg?raw";
import magnifyingGlassPlus from "@phosphor-icons/core/assets/regular/magnifying-glass-plus.svg?raw";
import monitor from "@phosphor-icons/core/assets/regular/monitor.svg?raw";
import pencilSimple from "@phosphor-icons/core/assets/regular/pencil-simple.svg?raw";
import plug from "@phosphor-icons/core/assets/regular/plug.svg?raw";
import plus from "@phosphor-icons/core/assets/regular/plus.svg?raw";
import terminalWindow from "@phosphor-icons/core/assets/regular/terminal-window.svg?raw";
import textAa from "@phosphor-icons/core/assets/regular/text-aa.svg?raw";
import trash from "@phosphor-icons/core/assets/regular/trash.svg?raw";
import uploadSimple from "@phosphor-icons/core/assets/regular/upload-simple.svg?raw";
import x from "@phosphor-icons/core/assets/regular/x.svg?raw";

const sized = (s: string, n = 16) => s.replace("<svg ", `<svg width="${n}" height="${n}" aria-hidden="true" focusable="false" `);

export const ICON = {
  refresh: sized(arrowClockwise),
  detach: sized(arrowSquareOut),
  span: sized(arrowsOut),
  screenshot: sized(camera),
  typeClipboard: sized(clipboardText),
  copy: sized(copy),
  download: sized(downloadSimple),
  folder: sized(folder),
  newFolder: sized(folderPlus),
  fullscreen: sized(frameCorners),
  handTap: sized(handTap),
  keyboard: sized(keyboard),
  osLinux: sized(linuxLogo, 40),
  osMac: sized(appleLogo, 40),
  osWindows: sized(windowsLogo, 40),
  osPi: sized(cpu, 40),
  osGeneric: sized(desktopTower, 40),
  quick: sized(lightning),
  save: sized(floppy),
  search: sized(magnifyingGlass),
  magnify: sized(magnifyingGlassPlus),
  desktop: sized(monitor),
  rename: sized(pencilSimple),
  disconnect: sized(plug),
  plus: sized(plus),
  shell: sized(terminalWindow),
  font: sized(textAa),
  trash: sized(trash),
  upload: sized(uploadSimple),
  close: sized(x, 12),
};
