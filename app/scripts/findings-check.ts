// Self-check for Check setup: every finding the backend can report (doctor.* in en.json) offers a fix,
// or can be hidden (its severity lets "It's fine" through). Run: node scripts/findings-check.ts
import { readFileSync } from "node:fs";
import { remedies, severity, type Fixes } from "../src/lib/findings.ts";

const en = JSON.parse(readFileSync(new URL("../src/lib/locales/en.json", import.meta.url), "utf8"));
const noop = async () => {};
const fixes: Fixes = {
  isMod: () => true, canLoad: () => true, itemOf: () => "1", install: noop, setMods: noop, uninstall: noop, sort: noop, trust: noop, setBuild: noop, zbSetup: noop, zbRemove: noop,
  update: noop, findB42: noop, unblock: noop, folderAuto: noop, installItem: noop, unsubscribe: noop, go: () => {}, openUrl: () => {}, launch: noop,
};
// the vars a fix may need, filled in as the backend does
const vars = { req: "R", other: "O", item: "1", items: "1, 2", theirs: "41", sha: "a".repeat(64), key: "zomboid", file: "f", version: "42.1", game: "42.0" };
const errors: string[] = [];
for (const key of Object.keys(en.doctor)) {
  for (const level of ["error", "warning"] as const) {
    const f = { level, subject: "M", text: "", key, vars, action: key.startsWith("java") || key.startsWith("lua") ? { dismiss: "a".repeat(64) } : null, hidden: false };
    const s = severity(f);
    const fix = remedies(f, ["M"], fixes);
    if (!fix.length && s !== "check" && s !== "low") errors.push(`${key} (${level}, ${s}): no fix, and it can't be hidden`);
  }
}
if (errors.length) {
  console.error(errors.join("\n"));
  console.error(`findings-check: ${errors.length} problem(s)`);
  process.exit(1);
}
console.log(`findings-check: ok (${Object.keys(en.doctor).length} findings)`);
