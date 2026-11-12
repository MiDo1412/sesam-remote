import { test } from "node:test";
import assert from "node:assert/strict";
import { merge, notesFor, platformOf } from "./updates.mjs";

test("Plattform aus dem Dateinamen", () => {
  assert.equal(platformOf("Sesam Remote.app.tar.gz", "aarch64"), "darwin-aarch64");
  assert.equal(platformOf("Sesam Remote_26.11.2_x64_de-DE.msi"), "windows-x86_64");
  assert.equal(platformOf("sesam-remote_26.11.2_amd64.AppImage"), "linux-x86_64");
  assert.equal(platformOf("Sesam Remote_26.11.2_aarch64.dmg"), null);
});

test("Notizen der Version aus den Release-Notes", () => {
  const md = "# X\n\n## In Arbeit\n\n## 26.11.2 — 2026-11-11\n\nNeu: A\n\n## 26.11.1 — 2026-11-10\n\nAlt\n";
  assert.equal(notesFor(md, "26.11.2"), "Neu: A");
  assert.equal(notesFor(md, "26.12.1"), "");
});

test("latest.json zusammenführen", () => {
  const now = new Date("2026-11-11T20:00:00Z");
  const a = merge(null, "26.11.2", "N", { "darwin-aarch64": { signature: "s", url: "u" } }, now);
  const b = merge(a, "26.11.2", "", { "windows-x86_64": { signature: "w", url: "v" } }, now);
  assert.deepEqual(Object.keys(b.platforms), ["darwin-aarch64", "windows-x86_64"]);
  assert.equal(b.notes, "N");
  const c = merge(b, "26.11.3", "M", { "darwin-aarch64": { signature: "t", url: "x" } }, now);
  assert.deepEqual(Object.keys(c.platforms), ["darwin-aarch64"], "neue Version beginnt leer");
});

import { githubNames, githubUrl } from "./release-files.mjs";

test("Namen fürs GitHub-Release: mit Version und fest", () => {
  assert.deepEqual(githubNames("Sesam Remote_26.11.2_aarch64.dmg", "26.11.2"), ["Sesam-Remote_26.11.2_macOS_aarch64.dmg", "Sesam-Remote_macOS.dmg"]);
  assert.deepEqual(githubNames("Sesam Remote_26.11.2_x64_de-DE.msi", "26.11.2"), ["Sesam-Remote_26.11.2_Windows_x64.msi", "Sesam-Remote_Windows.msi"]);
  assert.deepEqual(githubNames("Sesam-Remote_26.11.2_Windows_x64_portabel.zip", "26.11.2"), ["Sesam-Remote_26.11.2_Windows_x64_portabel.zip", "Sesam-Remote_Windows_x64_portabel.zip"]);
  assert.deepEqual(githubNames("Sesam Remote.app.tar.gz", "26.11.2"), ["Sesam-Remote.app.tar.gz", null]);
  assert.deepEqual(githubNames("sesam-remote_26.11.2_amd64.deb", "26.11.2"), ["Sesam-Remote_26.11.2_Linux_amd64.deb", "Sesam-Remote_Linux.deb"]);
  assert.equal(githubUrl("26.11.2", "Sesam-Remote.app.tar.gz"), "https://github.com/MiDo1412/sesam-remote/releases/download/v26.11.2/Sesam-Remote.app.tar.gz");
});
