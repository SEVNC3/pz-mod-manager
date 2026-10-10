// Self-check for src/lib/order.ts. Run: node scripts/order-check.ts
import assert from "node:assert/strict";
import { autosort, bisectRound, findProblems, groupByCategory, loadList, moveSection, orderOf, orphanedRequirements, placeInOrder, placeInSections, sortLoadOrder, sortMaps, autosortMaps, syncOrder } from "../src/lib/order.ts";
import type { ModRow, Row } from "../src/lib/api.ts";

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
assert.deepEqual(p.get("App"), { key: "problems.needsDisabled", id: "Lib", fixable: true });
assert.deepEqual(p.get("Patch"), { key: "problems.loadAfter", id: "Big", fixable: true });
assert.deepEqual(p.get("Early"), { key: "problems.loadBefore", id: "App", fixable: true });

const sorted = sortLoadOrder(rows);
assert.equal(ids(sorted), "#Top +Big +Patch +Lib +Early +App -Off"); // fewest moves: only App shifts later
assert.equal(findProblems(sorted).size, 0);
assert.deepEqual(sortLoadOrder(sorted), sorted, "sorting is stable once valid");

// incompatible is reported, not "fixed"
const inc = [mod("A", true, { incompatible: ["B"] }), mod("B")];
assert.deepEqual(findProblems(inc).get("A"), { key: "problems.incompatible", id: "B", fixable: false });

// a cycle keeps its order instead of looping
const cyc = [mod("X", true, { load_after: ["Y"] }), mod("Y", true, { load_after: ["X"] })];
assert.equal(ids(sortLoadOrder(cyc)), "+X +Y");

// bisect: first half of the suspects plus their requirements; innocent mods stay on
const list = [mod("Keep"), mod("S1", true, { requires: ["Lib"] }), mod("S2"), mod("S3"), mod("Lib")];
const req = (id: string) => (id === "S1" ? ["Lib"] : []);
const r1 = bisectRound(list, ["S1", "S2", "S3", "Lib"], req);
assert.deepEqual(r1.testing, ["S1", "S2"]);
assert.deepEqual([...r1.on].sort(), ["Keep", "Lib", "S1", "S2"]);

// the library and the load order: the order follows the library's on/off, never its positions
const lib: Row[] = [{ kind: "sep", name: "S" }, mod("A"), mod("B", false), mod("C"), mod("D")];
assert.deepEqual(syncOrder(lib, ["D", "B", "X", "C", "D"]), ["D", "C", "A"], "disabled and unknown ids dropped, missing ones appended");
assert.equal(ids(loadList(lib, ["C", "A"])), "+C +A +D -B");
assert.equal(findProblems(loadList([mod("App", true, { requires: ["Lib"] }), mod("Lib")], ["Lib", "App"])).size, 0, "rules follow the load order, not the library");

// enabling places the id where Autosort would: after what it needs, before what needs it, by tier
const known = [mod("New", true, { requires: ["Lib"] }), mod("User", true, { load_after: ["New"] })] as ModRow[];
const look = (id: string) => known.find((m) => m.id === id);
assert.deepEqual(placeInOrder(["A", "Lib", "B", "User", "C"], "New", look), ["A", "Lib", "B", "New", "User", "C"]);
assert.deepEqual(placeInOrder(["A", "B"], "X", look), ["A", "B", "X"], "no rules: last");
const tier = (id: string) => (id === "Fw" ? 0 : id === "Tr" ? 2 : 1);
assert.deepEqual(placeInOrder(["Fw", "A", "Tr"], "B", look, tier), ["Fw", "A", "B", "Tr"], "before translations");
assert.deepEqual(placeInOrder(["Fw", "A", "Tr"], "Fw2", look, (id) => (id.startsWith("Fw") ? 0 : tier(id))), ["Fw", "Fw2", "A", "Tr"], "with the frameworks");

// categories: sections in order, own separators dropped; the library ignores load rules
const order = ["Libs", "Gameplay", "Maps", "Other"];
const of = { App: "Gameplay", Lib: "Maps", Town: "Maps", Off: "Gameplay" };
const grouped = groupByCategory([{ kind: "sep", name: "Mine" }, mod("Town"), mod("Off", false), mod("App", true, { requires: ["Lib"] }), mod("Lib"), mod("Gone")], of, order);
assert.equal(ids(grouped), "#Gameplay -Off +App #Maps +Town +Lib #Other +Gone");

// a new mod goes to the end of its section, or a new section in its place
const placed = placeInSections([{ kind: "sep", name: "Gameplay" }, mod("A"), { kind: "sep", name: "Other" }, mod("B"), mod("New"), mod("Map1")], { New: "Gameplay", Map1: "Maps" }, order, ["New", "Map1"]);
assert.equal(ids(placed), "#Gameplay +A +New #Maps +Map1 #Other +B");

// autosort (a load list): turns on a needed mod, frameworks first, translations last, loadModBefore honoured
const auto = autosort([mod("Tr"), mod("Ui", true, { load_before: ["Core"] }), mod("Core"), mod("App", true, { requires: ["Dep"] }), mod("Dep", false), mod("Off", false)],
  { Tr: "Translations", Ui: "Gameplay", Core: "Libs", App: "Gameplay", Dep: "Gameplay" }, order);
assert.equal(ids(auto), "+Ui +Core +Dep +App +Tr -Off");
assert.deepEqual(orderOf(auto), ["Ui", "Core", "Dep", "App", "Tr"]);
assert.equal(findProblems(auto).size, 0);

// the server's maps: mod maps before Muldraugh, otherwise as arranged
assert.deepEqual(sortMaps(["B", "Muldraugh, KY", "A"]), ["B", "A", "Muldraugh, KY"]);
// autosort: a map that builds on another (its lots= lists it) goes before it; the rest keep their order
const lots: Record<string, string[]> = { "Fort Rock": ["Rosewood Exp"], "Rosewood Exp": ["Muldraugh, KY"], Loop1: ["Loop2"], Loop2: ["Loop1"] };
const lotsOf = (f: string) => lots[f] ?? [];
assert.deepEqual(autosortMaps(["Town", "Rosewood Exp", "Other", "Fort Rock", "Muldraugh, KY"], lotsOf), ["Town", "Fort Rock", "Rosewood Exp", "Other", "Muldraugh, KY"]);
assert.deepEqual(autosortMaps(["Muldraugh, KY", "B", "A"], lotsOf), ["B", "A", "Muldraugh, KY"], "no lots: the order stays, Muldraugh last");
assert.deepEqual(autosortMaps(["C", "B", "A"], (f) => ({ A: ["B"], B: ["C"] })[f] ?? []), ["A", "B", "C"], "a chain: each before what it builds on");
assert.deepEqual(autosortMaps(["Loop2", "X", "Loop1"], lotsOf), ["Loop2", "X", "Loop1"], "a loop keeps the order");

// dragging a whole section: it moves with its mods and lands on section boundaries
const secs: Row[] = [mod("Top"), { kind: "sep", name: "A" }, mod("a1"), mod("a2"), { kind: "sep", name: "B" }, mod("b1"), { kind: "sep", name: "C" }, mod("c1")];
assert.equal(ids(moveSection(secs, 1, 5)!), "+Top #B +b1 #A +a1 +a2 #C +c1", "dragged down onto B: lands after B");
assert.equal(ids(moveSection(secs, 6, 2)!), "+Top #C +c1 #A +a1 +a2 #B +b1", "dropped inside A: lands before A");
assert.equal(ids(moveSection(secs, 1, 8)!), "+Top #B +b1 #C +c1 #A +a1 +a2", "dropped at the end");
assert.equal(moveSection(secs, 1, 3), null, "inside itself");
assert.equal(moveSection(secs, 1, 4), null, "right where it is");

// removing a mod: requirements nothing else uses go too, through chains; shared ones stay
const deps = [mod("A", true, { requires: ["Lib", "Patch"] }), mod("Patch", true, { requires: ["Lib", "Core"] }), mod("Lib"), mod("Core"),
  mod("B", false, { requires: ["Shared", "Gone"] }), mod("Shared"), mod("C", true, { requires: ["Shared"] })] as ModRow[];
assert.deepEqual(orphanedRequirements(["A"], deps), ["Patch", "Lib", "Core"]);
assert.deepEqual(orphanedRequirements(["B"], deps), [], "Shared is still needed by C; Gone isn't installed");
assert.deepEqual(orphanedRequirements(["B", "C"], deps), ["Shared"]);

console.log("order-check: ok");
