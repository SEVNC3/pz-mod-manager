// Self-check for src/lib/order.ts. Run: node scripts/order-check.ts
import assert from "node:assert/strict";
import { bisectRound, findProblems, sortLoadOrder } from "../src/lib/order.ts";
import type { Row } from "../src/lib/api.ts";

const mod = (id: string, on = true, extra: Partial<Record<"requires" | "incompatible" | "load_after" | "load_before", string[]>> = {}): Row => ({
  kind: "mod", id, name: id, source: "", on, missing: false,
  requires: [], incompatible: [], load_after: [], load_before: [], ...extra,
});
const ids = (rows: Row[]) => rows.map((r) => (r.kind === "mod" ? `${r.on ? "+" : "-"}${r.id}` : `#${r.name}`)).join(" ");

// requirement after its dependent, a disabled requirement, and ordering hints
const rows: Row[] = [
  { kind: "sep", name: "Top" },
  mod("App", true, { requires: ["Lib"] }),
  mod("Patch", true, { load_after: ["Big"] }),
  mod("Big"),
  mod("Lib", false),
  mod("Early", true, { load_before: ["App"] }),
  mod("Off", false),
];
const p = findProblems(rows);
assert.equal(p.get("App")?.text, "Needs Lib (disabled)");
assert.equal(p.get("Patch")?.text, "Should load after Big");
assert.equal(p.get("Early")?.text, "Should load before App");

const sorted = sortLoadOrder(rows);
assert.equal(ids(sorted), "#Top +Big +Patch +Lib +Early +App -Off"); // fewest moves: only App shifts later
assert.equal(findProblems(sorted).size, 0);
assert.deepEqual(sortLoadOrder(sorted), sorted, "sorting is stable once valid");

// incompatible is reported, not "fixed"
const inc = [mod("A", true, { incompatible: ["B"] }), mod("B")];
assert.deepEqual(findProblems(inc).get("A"), { text: "Incompatible with B", fixable: false });

// a cycle keeps its order instead of looping
const cyc = [mod("X", true, { load_after: ["Y"] }), mod("Y", true, { load_after: ["X"] })];
assert.equal(ids(sortLoadOrder(cyc)), "+X +Y");

// bisect: first half of the suspects plus their requirements; innocent mods stay on
const list = [mod("Keep"), mod("S1", true, { requires: ["Lib"] }), mod("S2"), mod("S3"), mod("Lib")];
const req = (id: string) => (id === "S1" ? ["Lib"] : []);
const r1 = bisectRound(list, ["S1", "S2", "S3", "Lib"], req);
assert.deepEqual(r1.testing, ["S1", "S2"]);
assert.deepEqual([...r1.on].sort(), ["Keep", "Lib", "S1", "S2"]);

console.log("order-check: ok");
