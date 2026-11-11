#!/usr/bin/env node
/**
 * Packt die portable Fassung des aktuellen Systems aus dem fertigen Release-Build (docs/portabel.md) und legt
 * sie samt Installern und SHA-256-Prüfsummen nach test-build/release/<VERSION>/ (nicht im Git).
 *
 *   npx tauri build              (vorher; unter Windows: --bundles msi, macOS: --bundles app,dmg,
 *                                 Linux: --bundles appimage,deb)
 *   node scripts/portable.mjs
 *
 * Inhalt der portablen Pakete: die App, die Datei `portable` (schaltet den portablen Betrieb ein) und die
 * Lizenzen. Packen mit Werkzeugen des Systems: macOS `ditto`, Windows `tar` (bsdtar, ab Windows 10 dabei),
 * Linux `zip`.
 */
import { execFileSync } from "node:child_process";
import { copyFileSync, createReadStream, existsSync, mkdirSync, readdirSync, rmSync, writeFileSync, cpSync } from "node:fs";
import { createHash } from "node:crypto";
import { join, basename } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(import.meta.url), "..", "..");
const version = (await import("node:fs")).readFileSync(join(root, "VERSION"), "utf8").trim();
const bundle = join(root, "src-tauri", "target", "release", "bundle");
const out = join(root, "test-build", "release", version);
const stage = join(out, ".stage");
mkdirSync(out, { recursive: true });
rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });

const MARKER = "Diese Datei schaltet den portablen Betrieb ein: Daten liegen im Ordner «Sesam-Daten» daneben.\n";
const licenses = () => {
  const d = join(stage, "Lizenzen");
  mkdirSync(d, { recursive: true });
  copyFileSync(join(root, "LICENSE"), join(d, "LICENSE.txt"));
  for (const f of readdirSync(join(root, "docs", "lizenzen"))) copyFileSync(join(root, "docs", "lizenzen", f), join(d, f));
};
const files = (dir, test) => (existsSync(dir) ? readdirSync(dir).filter(test).map((f) => join(dir, f)) : []);
const done = [];

if (process.platform === "darwin") {
  const app = join(bundle, "macos", "Sesam Remote.app");
  if (!existsSync(app)) throw new Error("Kein Release-Build: zuerst «npx tauri build --bundles app,dmg»");
  cpSync(app, join(stage, "Sesam Remote.app"), { recursive: true, verbatimSymlinks: true });
  writeFileSync(join(stage, "portable"), MARKER);
  licenses();
  const zip = join(out, `Sesam-Remote_${version}_macOS_portabel.zip`);
  rmSync(zip, { force: true });
  execFileSync("ditto", ["-c", "-k", "--sequesterRsrc", stage, zip]);
  done.push(zip);
  for (const f of files(join(bundle, "dmg"), (f) => f.endsWith(".dmg") && f.includes(`_${version}_`))) {
    copyFileSync(f, join(out, basename(f)));
    done.push(join(out, basename(f)));
  }
} else if (process.platform === "win32") {
  const exe = join(root, "src-tauri", "target", "release", "sesam-remote.exe");
  if (!existsSync(exe)) throw new Error("Kein Release-Build: zuerst «npx tauri build --bundles msi»");
  copyFileSync(exe, join(stage, "sesam-remote.exe"));
  writeFileSync(join(stage, "portable"), MARKER);
  licenses();
  const zip = join(out, `Sesam-Remote_${version}_Windows_x64_portabel.zip`);
  rmSync(zip, { force: true });
  execFileSync("tar", ["-a", "-c", "-f", zip, "-C", stage, "."]);
  done.push(zip);
  for (const f of files(join(bundle, "msi"), (f) => f.endsWith(".msi") && f.includes(`_${version}_`))) {
    copyFileSync(f, join(out, basename(f)));
    done.push(join(out, basename(f)));
  }
} else {
  const img = files(join(bundle, "appimage"), (f) => f.endsWith(".AppImage") && f.includes(`_${version}_`))[0];
  if (!img) throw new Error("Kein AppImage: zuerst «npx tauri build --bundles appimage,deb»");
  copyFileSync(img, join(stage, basename(img)));
  writeFileSync(join(stage, "portable"), MARKER);
  licenses();
  const zip = join(out, `Sesam-Remote_${version}_Linux_x64_portabel.zip`);
  rmSync(zip, { force: true });
  execFileSync("zip", ["-r", "-q", zip, "."], { cwd: stage });
  done.push(zip);
  for (const f of [img, ...files(join(bundle, "deb"), (f) => f.endsWith(".deb") && f.includes(`_${version}_`))]) {
    copyFileSync(f, join(out, basename(f)));
    done.push(join(out, basename(f)));
  }
}
rmSync(stage, { recursive: true, force: true });

// Prüfsummen: eine Zeile je Datei, im Format von «shasum -a 256» / «sha256sum».
const sha = (f) => new Promise((ok) => {
  const h = createHash("sha256");
  createReadStream(f).on("data", (d) => h.update(d)).on("end", () => ok(h.digest("hex")));
});
const sumsFile = join(out, "SHA256SUMS.txt");
const old = existsSync(sumsFile) ? (await import("node:fs")).readFileSync(sumsFile, "utf8").split("\n").filter(Boolean) : [];
const lines = new Map(old.map((l) => [l.slice(66), l]));
for (const f of done) lines.set(basename(f), `${await sha(f)}  ${basename(f)}`);
writeFileSync(sumsFile, [...lines.values()].sort((a, b) => a.slice(66).localeCompare(b.slice(66))).join("\n") + "\n");
console.log([...done.map((f) => `fertig: ${f}`), `Prüfsummen: ${sumsFile}`].join("\n"));
