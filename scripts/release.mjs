#!/usr/bin/env node
/**
 * Cuts a release (docs/versionierung.md). A commit changes no number: every
 * change writes its lines under «## In Arbeit» in docs/release-notes.md, and a release turns
 * that section into the entry of the next number, YY.M.N — year, month, running number of the
 * month (26.10.1, 26.10.2, 26.11.1). Runs on every OS.
 *
 *   node scripts/release.mjs status     what waits under «In Arbeit» and the number it would get
 *   node scripts/release.mjs            cut: VERSION, package.json (and its lockfile),
 *                                       src-tauri/tauri.conf.json, src-tauri/Cargo.toml (and
 *                                       Cargo.lock) and the release notes; staged, not committed
 *   node scripts/release.mjs check      pre-commit: every mirror carries the number in VERSION
 *   node scripts/release.mjs publish    after the push: moves the branch `release` — what
 *                                       production deploys — to that commit, creates the tag
 *                                       v<VERSION> and the GitHub release with the entry's lines
 *                                       (needs `gh`) with the installers of that version as
 *                                       assets; run again it brings the release's text up to the
 *                                       entry and replaces the assets
 */
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const PENDING = "## In Arbeit";
const NUMBER = /^(\d+)\.(\d+)\.(\d+)$/;

/** The day in Zurich as YYYY-MM-DD: a release cut late in the evening belongs to that day. */
export function today(now = new Date()) {
  return new Intl.DateTimeFormat("sv-SE", { timeZone: "Europe/Zurich" }).format(now);
}

/** The number after `current` for a release on `date`: the month's next, or its first. */
export function next(current, date) {
  const [year, month] = date.split("-").map(Number);
  const [yy, m] = [year % 100, month];
  const found = NUMBER.exec(current.trim());
  const n = found && Number(found[1]) === yy && Number(found[2]) === m ? Number(found[3]) + 1 : 1;
  const version = `${yy}.${m}.${n}`;
  if (found && !above(version, current.trim())) {
    throw new Error(`${version} wäre nicht höher als ${current.trim()}: stimmt das Datum (${date})?`);
  }
  return version;
}

function above(a, b) {
  const [x, y] = [a, b].map((v) => NUMBER.exec(v).slice(1).map(Number));
  return x[0] !== y[0] ? x[0] > y[0] : x[1] !== y[1] ? x[1] > y[1] : x[2] > y[2];
}

/** The lines that wait under «In Arbeit»: from its heading to the next `## ` heading. */
export function pending(text) {
  const lines = text.split("\n");
  const start = lines.findIndex((line) => line.trim() === PENDING);
  if (start < 0) throw new Error(`«${PENDING}» fehlt.`);
  let end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
  if (end < 0) end = lines.length;
  const body = lines.slice(start + 1, end).join("\n").trim();
  return { lines, start, end, body };
}

/** The file with «In Arbeit» emptied and its lines under the heading of `version`. */
export function cut(text, version, date) {
  const { lines, start, end, body } = pending(text);
  if (!/^- /m.test(body)) throw new Error("Unter «In Arbeit» steht keine Zeile: nichts zu veröffentlichen.");
  const entry = [PENDING, "", `## ${version} — ${date}`, "", body, ""];
  return [...lines.slice(0, start), ...entry, ...lines.slice(end)].join("\n");
}

/** The lines of one released entry, for the GitHub release. */
export function entry(text, version) {
  const lines = text.split("\n");
  const start = lines.findIndex((line) => line.startsWith(`## ${version} — `));
  if (start < 0) throw new Error(`Kein Eintrag «## ${version} — …».`);
  let end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
  if (end < 0) end = lines.length;
  return lines.slice(start + 1, end).join("\n").trim();
}

/** Tauri mirrors the number in tauri.conf.json, Cargo.toml (its [package]) and Cargo.lock (the package `name`). */
export function setJsonVersion(text, version) {
  return text.replace(/("version"\s*:\s*")[^"]*(")/, `$1${version}$2`);
}
export function setCargoVersion(text, version) {
  return text.replace(/(\[package\][^[]*?\nversion\s*=\s*")[^"]*(")/, `$1${version}$2`);
}
export function setCargoLockVersion(text, name, version) {
  return text.replace(new RegExp(`(\\nname = "${name}"\\r?\\nversion = ")[^"]*(")`), `$1${version}$2`);
}

/** The installers of `version` that the Tauri build left (NSIS setup.exe, MSI). */
export function installers(root, version) {
  const found = [];
  for (const kind of ["nsis", "msi"]) {
    const dir = join(root, "src-tauri", "target", "release", "bundle", kind);
    if (!existsSync(dir)) continue;
    for (const file of readdirSync(dir)) if (file.includes(`_${version}_`) && /\.(exe|msi)$/.test(file)) found.push(join(dir, file));
  }
  return found;
}

function main() {
  const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
  const root = git("rev-parse", "--show-toplevel");
  const at = (...parts) => join(root, ...parts);
  const read = (...parts) => readFileSync(at(...parts), "utf8");
  const current = read("VERSION").trim();
  const notes = read("docs", "release-notes.md");
  const mode = process.argv[2] ?? "cut";

  if (mode === "status") {
    const waiting = pending(notes).body;
    console.log(`Letztes Release: ${current}. Das nächste hiesse ${next(current, today())}.`);
    console.log(waiting ? `\nIn Arbeit:\n${waiting}` : "\nUnter «In Arbeit» steht nichts.");
    return;
  }

  if (mode === "check") {
    // pre-commit: every mirror carries VERSION; a number set by hand in one of them is caught here
    const mirrors = {
      "package.json": existsSync(at("package.json")) && JSON.parse(read("package.json")).version,
      "src-tauri/tauri.conf.json": existsSync(at("src-tauri", "tauri.conf.json")) && JSON.parse(read("src-tauri", "tauri.conf.json")).version,
      "src-tauri/Cargo.toml": existsSync(at("src-tauri", "Cargo.toml")) && /\[package\][^[]*?\nversion\s*=\s*"([^"]*)"/.exec(read("src-tauri", "Cargo.toml"))?.[1],
    };
    const off = Object.entries(mirrors).filter(([, v]) => v !== false && v !== current.split("-")[0]);
    if (off.length) throw new Error(`VERSION ist ${current}, aber ${off.map(([f, v]) => `${f} hat ${v}`).join(", ")}. Nummern setzt nur «node scripts/release.mjs».`);
    return;
  }

  if (mode === "publish") {
    // the release is the commit that set VERSION, wherever HEAD has moved since: run again
    // later, this never carries unreleased commits onto `release`. Only what is on GitHub's
    // main is released, so the tag names a commit everybody can fetch.
    git("fetch", "--quiet", "origin", "main");
    const head = git("log", "-1", "--format=%H", "--", "VERSION");
    if (!head || git("show", `${head}:VERSION`).trim() !== current) throw new Error("VERSION ist nicht committet.");
    try {
      git("merge-base", "--is-ancestor", head, "origin/main");
    } catch {
      throw new Error("Der Release-Commit ist noch nicht auf origin/main: zuerst pushen.");
    }
    const tag = `v${current}`;
    const body = entry(notes, current);
    // production deploys the branch `release`, never `main`: it only ever moves here, forward
    // (no force), to a commit that is a release
    execFileSync("git", ["push", "origin", `${head}:refs/heads/release`], { stdio: "inherit" });
    let there = true;
    try {
      execFileSync("gh", ["release", "view", tag], { stdio: "ignore" });
    } catch {
      there = false;
    }
    const assets = installers(root, current);
    if (!assets.length) console.warn(`Kein Installer für ${current} gefunden (zuerst «npx tauri build»); das Release bekommt nur den Text.`);
    if (there) {
      // the entry is the source: a reworded line reaches the release that is already there
      execFileSync("gh", ["release", "edit", tag, "--notes-file", "-"], { input: `${body}\n`, stdio: ["pipe", "inherit", "inherit"] });
      console.log(`Das Release ${tag} gibt es schon; seine Hinweise entsprechen dem Eintrag.`);
    } else {
      execFileSync("gh", ["release", "create", tag, "--target", head, "--title", current, "--notes-file", "-"], {
        input: `${body}\n`,
        stdio: ["pipe", "inherit", "inherit"],
      });
    }
    if (assets.length) execFileSync("gh", ["release", "upload", tag, ...assets, "--clobber"], { stdio: "inherit" });
    return;
  }

  if (mode !== "cut") {
    console.error("usage: release.mjs [cut|publish|status|check]");
    process.exit(2);
  }

  const date = today();
  const version = next(current, date);
  writeFileSync(at("docs", "release-notes.md"), cut(notes, version, date));
  writeFileSync(at("VERSION"), `${version}\n`);
  const staged = ["VERSION", "docs/release-notes.md"];
  if (existsSync(at("package.json"))) {
    const pkg = JSON.parse(read("package.json"));
    pkg.version = version;
    writeFileSync(at("package.json"), `${JSON.stringify(pkg, null, 2)}\n`);
    staged.push("package.json");
    if (existsSync(at("package-lock.json"))) {
      // the lockfile names the package's version twice; both follow package.json
      const lock = JSON.parse(read("package-lock.json"));
      lock.version = version;
      if (lock.packages?.[""]) lock.packages[""].version = version;
      writeFileSync(at("package-lock.json"), `${JSON.stringify(lock, null, 2)}\n`);
      staged.push("package-lock.json");
    }
  }
  if (existsSync(at("src-tauri", "tauri.conf.json"))) {
    writeFileSync(at("src-tauri", "tauri.conf.json"), setJsonVersion(read("src-tauri", "tauri.conf.json"), version));
    staged.push("src-tauri/tauri.conf.json");
  }
  if (existsSync(at("src-tauri", "Cargo.toml"))) {
    const toml = read("src-tauri", "Cargo.toml");
    writeFileSync(at("src-tauri", "Cargo.toml"), setCargoVersion(toml, version));
    staged.push("src-tauri/Cargo.toml");
    const name = /\[package\][^[]*?\nname\s*=\s*"([^"]+)"/.exec(toml)?.[1];
    if (name && existsSync(at("src-tauri", "Cargo.lock"))) {
      writeFileSync(at("src-tauri", "Cargo.lock"), setCargoLockVersion(read("src-tauri", "Cargo.lock"), name, version));
      staged.push("src-tauri/Cargo.lock");
    }
  }
  git("add", ...staged);
  console.log(`Release ${current} → ${version} (${date}).`);
  console.log(`Weiter: prüfen, committen («Release ${version}»), pushen, dann «node scripts/release.mjs publish» (Branch release, Tag, GitHub-Release).`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
