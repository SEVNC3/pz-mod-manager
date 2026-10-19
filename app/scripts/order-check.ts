// Self-check for src/lib/order.ts. Run: node scripts/order-check.ts
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { modNeed, keptServers, autosort, bisectGroups, bisectNext, bisectRound, bisectUpgrade, dependants, findProblems, neededBy, sortByName, groupByCategory, loadList, moveSection, orderOf, orphanedRequirements, parsePreset, placeInOrder, placeInSections, sortLoadOrder, sortMaps, autosortMaps, solveBackground, syncOrder } from "../src/lib/order.ts";
import type { GameServer, ModRow, Row, ServerMods } from "../src/lib/api.ts";

const mod = (id: string, on = true, extra: Partial<ModRow> = {}): Row => ({
  kind: "mod", id, name: id, source: "", on, missing: false,
  requires: [], incompatible: [], load_after: [], load_before: [], load_first: "", load_last: "", pinned: false, author: "", only41: false, only42: false, ...extra,
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

// Find the broken mod: the shared fixture's groups, then whole searches for every culprit in every case
type BisectCase = { name: string; enabled: string[]; requires: Record<string, string[]>; groups: string[][] };
type BisectSearch = { case: string; why: string; broken: string[]; lead?: string[]; expect: { result: string; ids: string[] }; rounds: number };
const fixture: { cases: BisectCase[]; searches?: BisectSearch[] } = JSON.parse(readFileSync(new URL("../../tests/fixtures/bisect-groups.json", import.meta.url), "utf8"));
/** A whole search, the answers given by `broken(on)`: what it ends in, after checking each round's list. */
function search(c: BisectCase, broken: (on: Set<string>) => boolean, first: string[] = []): { found?: string[]; together?: string[]; none?: true; rounds: number } {
  const need = (id: string) => c.requires[id] ?? [];
  const original = c.enabled.map((id) => mod(id, true, { requires: need(id) }));
  const groups = bisectGroups(c.enabled, need);
  const lead = groups.filter((g) => g.some((id) => first.includes(id)));
  let s: { suspects: string[][]; testing: string[][]; confirm?: boolean } = { suspects: [...lead, ...groups.filter((g) => !lead.includes(g))], testing: [] };
  if (s.suspects.length < 2) return { found: s.suspects[0], rounds: 0 }; // one group: said at the start
  for (let round = 1; round <= 2 * c.enabled.length + 3; round++) {
    // as +page.svelte's bisectApply: round 1 all suspects off, round 2 the logs' groups
    const count = round === 1 ? 0 : round === 2 && lead.length && lead.length < s.suspects.length ? lead.length : undefined;
    const { on, testing } = bisectRound(original, s.suspects, count, s.confirm);
    // never a mod on while an enabled mod it needs is off; the load order kept
    for (const id of on) for (const r of need(id)) assert.ok(!c.enabled.includes(r) || on.has(r), `${c.name}: ${id} on without ${r}`);
    const order = c.enabled.filter((id) => on.has(id));
    assert.deepEqual(order, [...on].sort((a, b) => c.enabled.indexOf(a) - c.enabled.indexOf(b)), `${c.name}: load order`);
    const next = bisectNext({ ...s, testing }, broken(on));
    if ("found" in next) return { found: next.found, rounds: round };
    if ("together" in next) return { together: next.together, rounds: round };
    if ("none" in next) return { none: true, rounds: round };
    s = { ...next, testing: [] };
  }
  assert.fail(`${c.name}: the search didn't end`);
}
for (const c of fixture.cases) {
  assert.deepEqual(bisectGroups(c.enabled, (id) => c.requires[id] ?? []), c.groups, c.name);
  const groupOf = (id: string) => c.groups.find((g) => g.includes(id))!;
  // any one mod at fault (a leaf, a framework, a requirement pulled in): its group, and every round checked above
  for (const culprit of c.enabled) {
    assert.deepEqual(search(c, (on) => on.has(culprit)).found, groupOf(culprit), `${c.name}: ${culprit}`);
    assert.deepEqual(search(c, (on) => on.has(culprit), [culprit]).found, groupOf(culprit), `${c.name}: ${culprit}, from the logs`);
  }
}
// the shared whole searches (the game replays the same ones): result, ids and how many answers it took
for (const x of fixture.searches ?? []) {
  const c = fixture.cases.find((k) => k.name === x.case)!;
  const r = search(c, (on) => x.broken.every((id) => on.has(id)), x.lead ?? []);
  const result = r.rounds === 0 ? "oneGroup" : r.found ? "found" : r.together ? "together" : "none";
  assert.deepEqual({ result, ids: r.found ?? r.together ?? [], rounds: r.rounds }, { ...x.expect, rounds: x.rounds }, `${x.case}: ${x.why}`);
}
// the reporter's chain: Viewpoint, ZombieBuddy and Viewpoint's add-ons are found together, never apart
const vp = fixture.cases[0];
assert.deepEqual(search(vp, (on) => on.has("BloodFXAddon")).found, ["ZombieBuddy", "Viewpoint", "ViewpointSurface", "ViewpointBloodFX", "BloodFXAddon"]);
// two mods in different groups that break only together: the one left is named, and said to need the other
const pair = search(vp, (on) => on.has("Solo") && on.has("Ki5Car"));
assert.ok(pair.together && (pair.together.includes("Solo") || pair.together.includes("Ki5Car")), "two mods together");
// fixed whatever is on: never loops, and blames nothing on its own
assert.ok(!search(vp, () => false).found);
// broken whatever is on (not a mod at all): round 1, with every suspect off, says so
assert.deepEqual(search(vp, () => true), { none: true, rounds: 1 });
// a search saved before groups (lists of ids) still loads: grouped, testing the groups it was testing
const old = { round: 3, suspects: ["Lib", "Other", "User"], testing: ["User"] };
const up = bisectUpgrade(old, (id) => (id === "User" ? ["Lib"] : []));
assert.deepEqual([up.round, up.suspects, up.testing], [3, [["Lib", "User"], ["Other"]], [["Lib", "User"]]]);
assert.equal(bisectUpgrade(up, () => []), up, "a grouped one is left as it is");

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
// a new last section goes above the loose mods (after the break), which stay at the bottom
const aboveLoose = placeInSections([{ kind: "sep", name: "Gameplay" }, mod("A"), { kind: "sep", name: "" }, mod("Loose", false), mod("New")], { New: "Other" }, order, ["New", "Loose"]);
assert.equal(ids(aboveLoose), "#Gameplay +A #Other +New # -Loose");

// autosort (a load list): turns on a needed mod, frameworks first, translations last, loadModBefore honoured
const auto = autosort([mod("Tr"), mod("Ui", true, { load_before: ["Core"] }), mod("Core"), mod("App", true, { requires: ["Dep"] }), mod("Dep", false), mod("Off", false)],
  { Tr: "Translations", Ui: "Gameplay", Core: "Libs", App: "Gameplay", Dep: "Gameplay" }, order);
assert.equal(ids(auto), "+Ui +Core +Dep +App +Tr -Off");
assert.deepEqual(orderOf(auto), ["Ui", "Core", "Dep", "App", "Tr"]);
assert.equal(findProblems(auto).size, 0);

// Mod Load Order Sorter's loadFirst / loadLast: "on" beats the tiers, "category" works within one; requirements still win
const edges = autosort([mod("Tr"), mod("Last", true, { load_last: "on" }), mod("Core"), mod("App"), mod("First", true, { load_first: "on", requires: ["Dep"] }), mod("Dep"), mod("Top", true, { load_first: "category" })],
  { Tr: "Translations", Last: "Gameplay", Core: "Libs", App: "Gameplay", First: "Gameplay", Dep: "Gameplay", Top: "Gameplay" }, order);
assert.equal(ids(edges), "+Dep +First +Core +Top +App +Tr +Last");

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

// Import: the game's preset text, not just anything with a colon
assert.deepEqual(parsePreset("My run:\\Brita;\\tsarslib,Authentic Z - Current"), { name: "My run", mods: ["Brita", "tsarslib", "Authentic Z - Current"] });
assert.equal(parsePreset("https://github.com/x/y"), null, "a link isn't a preset");
assert.equal(parsePreset(":A;B"), null);
assert.equal(parsePreset("no colon here"), null);
assert.equal(parsePreset("Server IP: 192.168.1.5"), null, "an address isn't a mod list");
assert.equal(parsePreset("Note: back up first"), null, "a sentence uses neither ; nor \\");
assert.equal(parsePreset("IPs: 10.0.0.1;10.0.0.2"), null, "mod ids have a letter in them");
assert.deepEqual(parsePreset("Solo:\\Brita"), { name: "Solo", mods: ["Brita"] }, "one Build 42 id");
assert.deepEqual(parsePreset("Run: A;B"), { name: "Run", mods: ["A", "B"] });

// Needed by: enabled dependants only; disabling one asks about the whole chain
const needs = [mod("Core"), mod("Lib", true, { requires: ["Core"] }), mod("App", true, { requires: ["Lib"] }), mod("Old", false, { requires: ["Core"] })];
const nb = neededBy(needs);
assert.deepEqual([...nb], [["Core", ["Lib"]], ["Lib", ["App"]]]);
assert.deepEqual(dependants(["Core"], nb), ["Lib", "App"]);
assert.deepEqual(dependants(["Core", "Lib"], nb), ["App"], "picked mods aren't listed again");

// Sort A-Z: by name, loadFirst/loadLast kept at the ends, requirements still first
const az = sortByName([mod("zeta", true, { name: "Zeta" }), mod("b", true, { name: "beta", requires: ["z2"] }), mod("z2", true, { name: "Zulu" }),
  mod("last", true, { name: "Aaa", load_last: "on" }), mod("first", true, { name: "Yyy", load_first: "on" }), mod("off", false)] as ModRow[]);
assert.equal(ids(az), "+first +zeta +z2 +b +last -off", "beta waits for Zulu, which it needs");


// a server's mods to get: listed ones not installed; the ones it doesn't list counted apart, never as "ready"
{
  const srv = { mods: ["A", "B"], mod_count: 60 } as unknown as GameServer;
  assert.deepEqual(modNeed(srv, new Set(["A", "B"])), { get: 0, total: 60, unlisted: 58 });
  assert.deepEqual(modNeed(srv, new Set(["A"])), { get: 1, total: 60, unlisted: 58 });
  const m = (id: string) => ({ id, status: "found", item: "", title: "", size: 0, updated: 0, preview: "" });
  const full = { list: "full", mods: [m("A"), m("B"), m("C"), m("S")], server_only: ["S"], optional: [] } as unknown as ServerMods;
  assert.deepEqual(modNeed(srv, new Set(["A"]), full), { get: 2, total: 3, unlisted: 0 }, "a full list counts exactly, server-only left out");
}

// Home's kept Popular list: anything not in its shape is dropped, never drawn
{
  for (const bad of [null, "", "{", "{}", "[1,2]", '{"at":1,"list":{}}', '{"at":"1","list":[]}', '{"at":1,"list":[1]}', '{"at":1,"list":[{"addr":"a:1","name":"x"}]}'])
    assert.equal(keptServers(bad), null, `kept ${bad} is dropped`);
  const s = { addr: "a:1", name: "x", mods: [] };
  assert.deepEqual(keptServers(JSON.stringify({ at: 5, steam: true, list: [s], error: "e" })), { at: 5, steam: true, list: [s] }, "a kept list reads back");
}

// a background picture lined up from two spots: scale and offset per axis, refused when it can't be
{
  const bg = solveBackground({ img: [100, 50], world: [3000, 9000] }, { img: [500, 450], world: [5000, 10000] })!;
  assert.deepEqual(bg, { scale: [5, 2.5], offset: [2500, 8875] });
  const at = (p: number[]) => [p[0] * bg.scale[0] + bg.offset[0], p[1] * bg.scale[1] + bg.offset[1]];
  assert.deepEqual([at([100, 50]), at([500, 450])], [[3000, 9000], [5000, 10000]], "both spots land where they were put");
  assert.equal(solveBackground({ img: [10, 10], world: [0, 0] }, { img: [10.5, 90], world: [300, 300] }), null, "spots in one column");
  assert.equal(solveBackground({ img: [10, 10], world: [300, 0] }, { img: [90, 90], world: [0, 300] }), null, "mirrored across");
  assert.equal(solveBackground({ img: [10, 10], world: [0, 0] }, { img: [90, 90], world: [0, 300] }), null, "flat across");
}

console.log("order-check: ok");

// a requirement the build can't load (a B41-only mod on B42) isn't turned on by sorting
const b41 = [mod("App", true, { requires: ["Old"] }), mod("Old", false)];
assert.equal(ids(sortLoadOrder(b41, (m) => m.id !== "Old")), "+App -Old");
assert.equal(ids(sortLoadOrder(b41)), "+Old +App", "loadable by default");
console.log("order-check: requirements this build can't load stay off");
