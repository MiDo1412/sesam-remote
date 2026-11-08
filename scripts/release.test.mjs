// node --test scripts/release.test.mjs
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { cut, entry, next, pending, setCargoLockVersion, setCargoVersion, setJsonVersion, today } from "./release.mjs";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "release.mjs");

const NOTES = `# Versionshinweise

Vorspann.

## In Arbeit

Ein Satz zur Version.

### Neu

- **Neu:** eine Zeile
  mit Fortsetzung.

### Betrieb

- **Handgriff:** einmal ausführen.

## 0.1.0 — 2026-10-01

- Alt.
`;
const WAITED = "Ein Satz zur Version.\n\n### Neu\n\n- **Neu:** eine Zeile\n  mit Fortsetzung.\n\n### Betrieb\n\n- **Handgriff:** einmal ausführen.";

test("the number follows the calendar: the month's next, or its first", () => {
  assert.equal(next("0.1.0", "2026-10-06"), "26.10.1"); // a project's first release
  assert.equal(next("1.4.7", "2026-10-06"), "26.10.1"); // the step away from a number per commit
  assert.equal(next("0.9.1-beta", "2026-10-06"), "26.10.1");
  assert.equal(next("26.10.1", "2026-10-06"), "26.10.2"); // twice on one day
  assert.equal(next("26.10.9", "2026-10-31"), "26.10.10");
  assert.equal(next("26.10.4", "2026-11-02"), "26.11.1");
  assert.equal(next("26.12.3", "2027-01-04"), "27.1.1");
  // a clock that went back never lowers the number
  assert.throws(() => next("26.11.1", "2026-10-30"), /nicht höher/);
});

test("the day is Zurich's", () => {
  assert.equal(today(new Date("2026-10-31T23:30:00Z")), "2026-11-01");
});

test("a release moves what waited under its number and leaves «In Arbeit» empty", () => {
  const out = cut(NOTES, "26.10.1", "2026-10-09");
  assert.equal(pending(out).body, "");
  assert.equal(entry(out, "26.10.1"), WAITED);
  assert.equal(entry(out, "0.1.0"), "- Alt.");
  assert.ok(out.indexOf("## In Arbeit") < out.indexOf("## 26.10.1 — 2026-10-09"));
  assert.ok(out.indexOf("## 26.10.1 — ") < out.indexOf("## 0.1.0 — "));
  assert.ok(out.startsWith("# Versionshinweise\n\nVorspann.\n"));
});

test("nothing waiting is no release", () => {
  const empty = NOTES.replace(/Ein Satz[\s\S]*ausführen\.\n/, "");
  assert.throws(() => cut(empty, "26.10.1", "2026-10-09"), /nichts zu veröffentlichen/);
  assert.throws(() => cut("# Ohne Abschnitt\n", "26.10.1", "2026-10-09"), /fehlt/);
  assert.throws(() => entry(NOTES, "26.9.1"), /Kein Eintrag/);
});

/** A made-up project beside its made-up origin; nothing of this repository or of GitHub is
 * touched. `gh` is a script that writes down how it was called. */
function project() {
  const base = mkdtempSync(join(tmpdir(), "release-test-"));
  const repo = join(base, "project");
  const git = (...args) => execFileSync("git", ["-C", repo, ...args], { encoding: "utf8" }).trim();
  const put = (path, text) => {
    mkdirSync(dirname(join(repo, path)), { recursive: true });
    writeFileSync(join(repo, path), text);
  };
  execFileSync("git", ["init", "--quiet", "--bare", "--initial-branch=main", join(base, "origin.git")]);
  execFileSync("git", ["clone", "--quiet", join(base, "origin.git"), repo], { stdio: "ignore" });
  git("config", "user.email", "test@example.ch");
  git("config", "user.name", "Test");
  git("config", "commit.gpgsign", "false");
  git("checkout", "--quiet", "-B", "main");
  put("VERSION", "0.1.0\n");
  put("docs/release-notes.md", NOTES);
  put("package.json", `${JSON.stringify({ name: "app", version: "0.1.0" }, null, 2)}\n`);
  put("package-lock.json", `${JSON.stringify({ name: "app", version: "0.1.0", packages: { "": { name: "app", version: "0.1.0" }, "node_modules/x": { version: "4.5.0" } } }, null, 2)}\n`);
  put("src-tauri/tauri.conf.json", `{\n  "productName": "App",\n  "version": "0.1.0",\n  "plugins": { "x": { "version": "9" } }\n}\n`);
  put("src-tauri/Cargo.toml", `[package]\nname = "app"\nversion = "0.1.0"\n\n[dependencies]\nx = { version = "1" }\n`);
  put("src-tauri/Cargo.lock", `[[package]]\nname = "aaa"\nversion = "0.1.0"\n\n[[package]]\nname = "app"\nversion = "0.1.0"\n`);
  git("add", "--all");
  git("commit", "--quiet", "-m", "Projekt eingerichtet");
  git("push", "--quiet", "origin", "main");
  mkdirSync(join(base, "bin"));
  const calls = join(base, "gh.log");
  // `release view` succeeds once `release create` ran: the second publish finds the release
  writeFileSync(join(base, "bin", "gh"), `#!/bin/sh\necho "$@" >> "${calls}"\n[ "$2" = view ] && { [ -f "${calls}.made" ]; exit $?; }\n[ "$2" = create ] && touch "${calls}.made"\ncat > "${calls}.body"\n`);
  chmodSync(join(base, "bin", "gh"), 0o755);
  const run = (...args) => spawnSync("node", [SCRIPT, ...args], { cwd: repo, encoding: "utf8", env: { ...process.env, PATH: `${join(base, "bin")}${delimiter}${process.env.PATH}` } });
  return { git, put, run, calls, read: (path) => readFileSync(join(repo, path), "utf8") };
}

test("a cut sets the number everywhere it is mirrored, stages it and commits nothing", () => {
  const { git, run, read } = project();
  const version = next("0.1.0", today());
  assert.match(run("status").stdout, new RegExp(`Letztes Release: 0\\.1\\.0\\. Das nächste hiesse ${version.replaceAll(".", "\\.")}`));
  const done = run();
  assert.equal(done.status, 0, done.stderr);
  assert.equal(read("VERSION"), `${version}\n`);
  assert.equal(JSON.parse(read("package.json")).version, version);
  assert.equal(JSON.parse(read("src-tauri/tauri.conf.json")).version, version);
  assert.equal(JSON.parse(read("src-tauri/tauri.conf.json")).plugins.x.version, "9");
  assert.match(read("src-tauri/Cargo.toml"), new RegExp(`\\[package\\]\\nname = "app"\\nversion = "${version}"\\n[\\s\\S]*x = \\{ version = "1" \\}`));
  assert.equal(read("src-tauri/Cargo.lock"), `[[package]]\nname = "aaa"\nversion = "0.1.0"\n\n[[package]]\nname = "app"\nversion = "${version}"\n`);
  assert.equal(run("check").status, 0);
  const lock = JSON.parse(read("package-lock.json"));
  assert.deepEqual([lock.version, lock.packages[""].version, lock.packages["node_modules/x"].version], [version, version, "4.5.0"]);
  assert.equal(entry(read("docs/release-notes.md"), version), WAITED);
  assert.deepEqual(git("diff", "--cached", "--name-only").split("\n"), ["VERSION", "docs/release-notes.md", "package-lock.json", "package.json", "src-tauri/Cargo.lock", "src-tauri/Cargo.toml", "src-tauri/tauri.conf.json"]);
  assert.equal(git("rev-list", "--count", "HEAD"), "1");
  // nothing waits any more: a second cut is refused and leaves the number alone
  const again = run();
  assert.equal(again.status, 1);
  assert.match(again.stderr, /nichts zu veröffentlichen/);
  assert.equal(read("VERSION"), `${version}\n`);
});

test("publishing moves `release` to the release commit, never further, and may run again", { skip: process.platform === "win32" }, () => {
  const { git, put, run, calls, read } = project();
  run();
  const version = read("VERSION").trim();
  const early = run("publish");
  assert.equal(early.status, 1);
  assert.match(early.stderr, /nicht committet/);
  git("commit", "--quiet", "-m", `Release ${version}`);
  const release = git("rev-parse", "HEAD");
  const unpushed = run("publish");
  assert.equal(unpushed.status, 1);
  assert.match(unpushed.stderr, /zuerst pushen/);
  assert.equal(existsSync(calls), false);
  assert.equal(git("ls-remote", "--heads", "origin", "release"), "");
  // work goes on after the release: production gets the release, not what came after
  put("docs/release-notes.md", read("docs/release-notes.md").replace("## In Arbeit\n", "## In Arbeit\n\n- Schon das Nächste.\n"));
  git("commit", "--quiet", "-am", "Weiter");
  git("push", "--quiet", "origin", "main");
  const first = run("publish");
  assert.equal(first.status, 0, first.stderr);
  assert.equal(git("ls-remote", "--heads", "origin", "release").split("\t")[0], release);
  assert.deepEqual(readFileSync(calls, "utf8").trim().split("\n"), [`release view v${version}`, `release create v${version} --target ${release} --title ${version} --notes-file -`]);
  assert.equal(readFileSync(`${calls}.body`, "utf8"), `${WAITED}\n`);
  const second = run("publish");
  assert.equal(second.status, 0, second.stderr);
  assert.equal(git("ls-remote", "--heads", "origin", "release").split("\t")[0], release);
  assert.deepEqual(readFileSync(calls, "utf8").trim().split("\n").slice(2), [`release view v${version}`, `release edit v${version} --notes-file -`]);
});

test("the project's files are what the script expects", () => {
  const root = join(dirname(SCRIPT), "..");
  const read = (...parts) => readFileSync(join(root, ...parts), "utf8");
  const version = read("VERSION").trim();
  const notes = read("docs", "release-notes.md");
  assert.ok(pending(notes).start < notes.split("\n").findIndex((line) => line.startsWith(`## ${version} — `)), "«In Arbeit» stands above the last release");
  // a number from before the calendar may carry a suffix the package never had (0.9.1-beta)
  assert.equal(JSON.parse(read("package.json")).version, version.split("-")[0]);
  assert.equal(JSON.parse(read("src-tauri", "tauri.conf.json")).version, version.split("-")[0]);
  // the cut rewrites the lockfile: only the number may move (line ends as Git checks them out)
  const lock = read("package-lock.json").replaceAll("\r\n", "\n");
  assert.equal(`${JSON.stringify(JSON.parse(lock), null, 2)}\n`, lock);
});

test("a hand-set number in a mirror is caught, others stay untouched", () => {
  assert.equal(setJsonVersion('{\n  "version": "0.1.0",\n  "x": { "version": "9" }\n}', "26.11.1"), '{\n  "version": "26.11.1",\n  "x": { "version": "9" }\n}');
  assert.equal(setCargoVersion('[package]\nname = "s"\nversion = "0.1.0"\n\n[dependencies]\nx = { version = "1" }\n', "26.11.1"), '[package]\nname = "s"\nversion = "26.11.1"\n\n[dependencies]\nx = { version = "1" }\n');
  assert.equal(setCargoLockVersion('\nname = "s"\r\nversion = "0.1.0"\r\n', "s", "26.11.1"), '\nname = "s"\r\nversion = "26.11.1"\r\n');
});
