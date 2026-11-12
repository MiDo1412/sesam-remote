/** Dateinamen der Release-Dateien auf GitHub (gemeinsam für portable.mjs und updates.mjs). */

export const REPO = "MiDo1412/sesam-remote";

/** GitHub-Name einer Datei und der feste Name ohne Version (oder null). */
export function githubNames(file, version) {
  if (file.endsWith(".dmg")) return [`Sesam-Remote_${version}_macOS_aarch64.dmg`, "Sesam-Remote_macOS.dmg"];
  if (file.endsWith(".msi")) return [`Sesam-Remote_${version}_Windows_x64.msi`, "Sesam-Remote_Windows.msi"];
  if (file.endsWith(".AppImage")) return [`Sesam-Remote_${version}_Linux_x64.AppImage`, "Sesam-Remote_Linux.AppImage"];
  if (file.endsWith(".deb")) return [`Sesam-Remote_${version}_Linux_amd64.deb`, "Sesam-Remote_Linux.deb"];
  const portable = /_(macOS|Windows_x64|Linux_x64)_portabel\.zip$/.exec(file);
  if (portable) return [file, `Sesam-Remote_${portable[1]}_portabel.zip`];
  return [file.replaceAll(" ", "-"), null];
}


/** Download-Adresse einer Datei im GitHub-Release `v<version>`. */
export const githubUrl = (version, name) => `https://github.com/${REPO}/releases/download/v${version}/${encodeURIComponent(name)}`;
