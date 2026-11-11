#!/usr/bin/env node
/**
 * Stellt die Update-Dateien einer Version für die Website zusammen (docs/release.md):
 * test-build/release/<VERSION>/updates/latest.json und <VERSION>/<Dateien samt .sig>, bereit zum Hochladen nach
 * https://sesam-remote.app/updates/. Nimmt, was auf diesem System signiert gebaut wurde, und ergänzt eine schon
 * vorhandene latest.json (Windows und Linux kommen von anderen Rechnern dazu).
 *
 *   node scripts/updates.mjs                     Artefakte aus src-tauri/target/release/bundle übernehmen
 *   node scripts/updates.mjs <plattform> <datei> eine einzelne signierte Datei (mit <datei>.sig daneben),
 *                                                z. B. windows-x86_64-portable sesam-remote.exe
 *
 * Signieren geschieht beim Bauen (TAURI_SIGNING_PRIVATE_KEY/-PASSWORD, `createUpdaterArtifacts`) bzw. mit
 * `npx tauri signer sign`; dieses Skript liest nur die .sig-Dateien.
 */
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

export const BASE = "https://sesam-remote.app/updates";
const root = join(fileURLToPath(import.meta.url), "..", "..");

/** Plattform-Schlüssel des Tauri-Updaters für eine gebaute Datei, sonst null. */
export function platformOf(file, arch = process.arch === "arm64" ? "aarch64" : "x86_64") {
  if (file.endsWith(".app.tar.gz")) return `darwin-${arch}`;
  if (file.endsWith(".msi")) return "windows-x86_64";
  if (file.endsWith(".AppImage")) return "linux-x86_64";
  return null;
}

/** Text der Version aus den Release-Notes (Abschnitt «## <Version> — …»), für das Feld notes. */
export function notesFor(md, version) {
  const start = md.indexOf(`\n## ${version} `);
  if (start < 0) return "";
  const rest = md.slice(start + 1);
  const end = rest.indexOf("\n## ", 3);
  return (end < 0 ? rest : rest.slice(0, end)).split("\n").slice(1).join("\n").trim();
}

/** latest.json zusammenführen: neue Plattformen dazu, gleiche ersetzen. */
export function merge(old, version, notes, entries, now = new Date()) {
  const base = old && old.version === version ? old : { version, notes, pub_date: now.toISOString(), platforms: {} };
  return { ...base, notes: base.notes || notes, platforms: { ...base.platforms, ...entries } };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const version = readFileSync(join(root, "VERSION"), "utf8").trim();
  const out = join(root, "test-build", "release", version, "updates");
  const files = join(out, version);
  mkdirSync(files, { recursive: true });
  const found = [];
  if (process.argv[2]) found.push([process.argv[2], process.argv[3]]);
  else {
    const bundle = join(root, "src-tauri", "target", "release", "bundle");
    for (const dir of ["macos", "msi", "appimage"]) {
      const d = join(bundle, dir);
      if (!existsSync(d)) continue;
      for (const f of readdirSync(d)) {
        const p = platformOf(f);
        if (p && existsSync(join(d, `${f}.sig`)) && (dir === "macos" || f.includes(`_${version}_`))) found.push([p, join(d, f)]);
      }
    }
  }
  if (!found.length) throw new Error("Keine signierten Artefakte gefunden (mit TAURI_SIGNING_PRIVATE_KEY und createUpdaterArtifacts bauen, docs/release.md)");
  const entries = {};
  for (const [platform, file] of found) {
    // Ohne Leerzeichen im Namen, damit die URL schlicht bleibt; die Version steht im Ordner.
    const name = basename(file).replaceAll(" ", "-");
    copyFileSync(file, join(files, name));
    copyFileSync(`${file}.sig`, join(files, `${name}.sig`));
    entries[platform] = { signature: readFileSync(`${file}.sig`, "utf8").trim(), url: `${BASE}/${version}/${encodeURIComponent(name)}` };
  }
  const latest = join(out, "latest.json");
  const old = existsSync(latest) ? JSON.parse(readFileSync(latest, "utf8")) : null;
  const notes = notesFor(readFileSync(join(root, "docs", "release-notes.md"), "utf8"), version);
  writeFileSync(latest, JSON.stringify(merge(old, version, notes, entries), null, 2) + "\n");
  console.log(`latest.json: ${latest}\n${Object.keys(entries).map((p) => `  ${p}`).join("\n")}`);
}
