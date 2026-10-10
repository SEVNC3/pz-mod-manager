// Self-check for src/lib/order.ts. Run: node scripts/order-check.ts
import assert from "node:assert/strict";
import { autosort, bisectRound, findProblems, groupByCategory, moveSection, placeInSections, sortLoadOrder } from "../src/lib/order.ts";
import type { Row } from "../src/lib/api.ts";

const mod = (id: string, on = true, extra: Partial<Record<"requires" | "incompatible" | "load_after" | "load_before", string[]>> = {}): Row => ({
  kind: "mod", id, name: id, source: "", on, missing: false,
  requires: [], incompatible: [], load_after: [], load_before: [], only41: false, only42: false, ...extra,
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

// categories: sections in order, own separators dropped, a later-section requirement moves up
const order = ["Libs", "Gameplay", "Maps", "Other"];
const of = { App: "Gameplay", Lib: "Maps", Town: "Maps", Off: "Gameplay" };
const grouped = groupByCategory([{ kind: "sep", name: "Mine" }, mod("Town"), mod("Off", false), mod("App", true, { requires: ["Lib"] }), mod("Lib"), mod("Gone")], of, order);
assert.equal(ids(grouped), "#Gameplay -Off +Lib +App #Maps +Town #Other +Gone");
assert.equal(findProblems(grouped).size, 0);

// a new mod goes to the end of its section, or a new section in its place
const placed = placeInSections([{ kind: "sep", name: "Gameplay" }, mod("A"), { kind: "sep", name: "Other" }, mod("B"), mod("New"), mod("Map1")], { New: "Gameplay", Map1: "Maps" }, order, ["New", "Map1"]);
assert.equal(ids(placed), "#Gameplay +A +New #Maps +Map1 #Other +B");

// autosort: turns on a needed mod, honours loadModBefore across the list
const auto = autosort([mod("Ui", true, { load_before: ["Core"] }), mod("Core"), mod("App", true, { requires: ["Dep"] }), mod("Dep", false)],
  { Ui: "Gameplay", Core: "Libs", App: "Gameplay", Dep: "Gameplay" }, order);
assert.equal(ids(auto), "#Libs +Ui +Core #Gameplay +Dep +App");
assert.equal(findProblems(auto).size, 0);

// dragging a whole section: it moves with its mods and lands on section boundaries
const secs: Row[] = [mod("Top"), { kind: "sep", name: "A" }, mod("a1"), mod("a2"), { kind: "sep", name: "B" }, mod("b1"), { kind: "sep", name: "C" }, mod("c1")];
assert.equal(ids(moveSection(secs, 1, 5)!), "+Top #B +b1 #A +a1 +a2 #C +c1", "dragged down onto B: lands after B");
assert.equal(ids(moveSection(secs, 6, 2)!), "+Top #C +c1 #A +a1 +a2 #B +b1", "dropped inside A: lands before A");
assert.equal(ids(moveSection(secs, 1, 8)!), "+Top #B +b1 #C +c1 #A +a1 +a2", "dropped at the end");
assert.equal(moveSection(secs, 1, 3), null, "inside itself");
assert.equal(moveSection(secs, 1, 4), null, "right where it is");

console.log("order-check: ok");
