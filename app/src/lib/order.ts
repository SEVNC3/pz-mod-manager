// Load-order rules from mod.info: require, incompatible, loadModAfter, loadModBefore.
// Pure functions so they can be checked outside the app (scripts/order-check.ts).
// Two lists: the library (`Row[]`, organised by the player, order is cosmetic) and the
// load order (enabled mod ids). Rule checks and sorts run on a load list: the library's
// mods with the enabled ones first, in load order (see `loadList`).
import type { GameServer, ModRow, Row, ServerMods } from "./api";

/** `key`: its text in the locale files, about mod `id`. */
export type Problem = { key: string; id: string; fixable: boolean };

const isMod = (r: Row): r is ModRow => r.kind === "mod";

/** `order` kept to the library's enabled mods, plus any it lacks at the end (in library order). */
export function syncOrder(rows: Row[], order: string[]): string[] {
  const on = rows.flatMap((r) => (isMod(r) && r.on ? [r.id] : []));
  const set = new Set(on);
  return [...new Set([...order.filter((id) => set.has(id)), ...on])];
}

/** The library's mods, enabled ones first in load order, then the disabled ones. */
export function loadList(rows: Row[], order: string[]): ModRow[] {
  const mods = new Map(rows.filter(isMod).map((r) => [r.id, r]));
  return [...syncOrder(rows, order).map((id) => mods.get(id)!), ...rows.filter((r): r is ModRow => isMod(r) && !r.on)];
}

/** The enabled ids of a load list, in order. */
export const orderOf = (list: Row[]) => list.flatMap((r) => (isMod(r) && r.on ? [r.id] : []));

/** Autosort's tiers from the mods' categories: frameworks (the first category) 0, translations 2, the rest 1. */
export const tiers = (of: Record<string, string>, order: string[]) => (id: string) => (of[id] === order[0] ? 0 : of[id] === "Translations" ? 2 : 1);

/**
 * `order` with `id` inserted where Autosort would put it, moving nothing else: after what it
 * must load after, before what must load after it, and in that room after the last mod of
 * its tier or an earlier one (without tiers: the end of the room).
 */
export function placeInOrder(order: string[], id: string, mod: (id: string) => ModRow | undefined, tier: (id: string) => number = () => 1): string[] {
  const rest = order.filter((o) => o !== id);
  // a must load after b
  const after = (a: string, b: string) =>
    [...(mod(a)?.requires ?? []), ...(mod(a)?.load_after ?? [])].includes(b) || (mod(b)?.load_before ?? []).includes(a);
  const lo = rest.findLastIndex((o) => after(id, o)) + 1;
  const first = rest.findIndex((o) => after(o, id));
  const hi = Math.max(lo, first < 0 ? rest.length : first); // conflicting rules: its own needs win
  let at = lo;
  for (let i = lo; i < hi; i++) if (tier(rest[i]) <= tier(id)) at = i + 1;
  rest.splice(at, 0, id);
  return rest;
}

/** The first problem of each enabled mod, by mod id. */
export function findProblems(rows: Row[]): Map<string, Problem> {
  const pos = new Map<string, number>();
  const mods = new Map<string, ModRow>();
  rows.forEach((r, i) => {
    if (isMod(r)) {
      pos.set(r.id, i);
      mods.set(r.id, r);
    }
  });
  const enabled = (id: string) => {
    const m = mods.get(id);
    return !!m && m.on && !m.missing;
  };
  const check = (r: ModRow, i: number): Problem | null => {
    for (const req of r.requires ?? []) {
      const dep = mods.get(req);
      if (!dep || dep.missing) return { key: "problems.needsMissing", id: req, fixable: false };
      if (!dep.on) return { key: "problems.needsDisabled", id: req, fixable: true };
      if (pos.get(req)! > i) return { key: "problems.loadFirst", id: req, fixable: true };
    }
    for (const x of r.incompatible ?? []) if (enabled(x)) return { key: "problems.incompatible", id: x, fixable: false };
    for (const a of r.load_after ?? []) if (enabled(a) && pos.get(a)! > i) return { key: "problems.loadAfter", id: a, fixable: true };
    for (const b of r.load_before ?? []) if (enabled(b) && pos.get(b)! < i) return { key: "problems.loadBefore", id: b, fixable: true };
    return null;
  };
  const out = new Map<string, Problem>();
  rows.forEach((r, i) => {
    if (!isMod(r) || !r.on || r.missing) return;
    const p = check(r, i);
    if (p) out.set(r.id, p);
  });
  return out;
}

/**
 * Enable disabled requirements of enabled mods, then reorder the enabled mods so every
 * requirement and loadModAfter/loadModBefore hint holds, moving as little as possible
 * (a stable topological sort: ties keep the current order; a cycle keeps its order).
 * Separators and disabled mods stay where they are; enabled mods refill the same slots.
 */
export function sortLoadOrder(rows: Row[], canLoad: (r: ModRow) => boolean = () => true): Row[] {
  const next = enableRequirements(rows, canLoad);

  const slots: number[] = [];
  const enabled: ModRow[] = [];
  next.forEach((r, i) => {
    if (isMod(r) && r.on) {
      slots.push(i);
      enabled.push(r);
    }
  });
  const index = new Map(enabled.map((r, i) => [r.id, i]));
  const before = enabled.map(() => new Set<number>()); // before[i]: mods that must precede i
  enabled.forEach((r, i) => {
    for (const a of [...(r.requires ?? []), ...(r.load_after ?? [])]) {
      const j = index.get(a);
      if (j !== undefined && j !== i) before[i].add(j);
    }
    for (const b of r.load_before ?? []) {
      const j = index.get(b);
      if (j !== undefined && j !== i) before[j].add(i);
    }
  });
  const placed = new Set<number>();
  const order: number[] = [];
  while (order.length < enabled.length) {
    let pick = enabled.findIndex((_, i) => !placed.has(i) && [...before[i]].every((j) => placed.has(j)));
    if (pick < 0) pick = enabled.findIndex((_, i) => !placed.has(i)); // cycle: keep the current order
    placed.add(pick);
    order.push(pick);
  }
  order.forEach((ei, k) => (next[slots[k]] = enabled[ei]));
  return next;
}

/** A copy with the disabled requirements of enabled mods turned on (and theirs, and so on). */
/** `canLoad`: a requirement the game can't load on this build (a B41-only mod on B42) stays off. */
function enableRequirements(rows: Row[], canLoad: (r: ModRow) => boolean): Row[] {
  const next: Row[] = rows.map((r) => ({ ...r }));
  const byId = new Map(next.filter(isMod).map((r) => [r.id, r]));
  for (let changed = true; changed; ) {
    changed = false;
    for (const r of next) {
      if (!isMod(r) || !r.on) continue;
      for (const req of r.requires ?? []) {
        const dep = byId.get(req);
        if (dep && !dep.on && !dep.missing && canLoad(dep)) {
          dep.on = true;
          changed = true;
        }
      }
    }
  }
  return next;
}

/**
 * Move what each enabled mod needs first (requirements, loadModAfter) from below it to
 * just above it, and the mod itself above anything it must load before.
 */
function pullUp(rows: Row[]): Row[] {
  const next = [...rows];
  const at = new Map<string, number>(); // enabled mod id -> index, rebuilt after each move
  const reindex = () => {
    at.clear();
    next.forEach((x, k) => isMod(x) && x.on && at.set(x.id, k));
  };
  // ponytail: a cycle (A needs B, B loads after A) would swap forever; 50 passes is plenty for real lists
  for (let pass = 0, moved = true; moved && pass < 50; pass++) {
    moved = false;
    reindex();
    for (let i = 0; i < next.length; i++) {
      const r = next[i];
      if (!isMod(r) || !r.on) continue;
      for (const dep of [...(r.requires ?? []), ...(r.load_after ?? [])]) {
        const j = at.get(dep) ?? -1;
        if (j <= i) continue;
        next.splice(i, 0, ...next.splice(j, 1));
        i++;
        moved = true;
        reindex();
      }
      const before = (r.load_before ?? []).map((b) => at.get(b) ?? -1).filter((j) => j >= 0 && j < i);
      if (before.length) {
        next.splice(Math.min(...before), 0, ...next.splice(i, 1));
        moved = true;
        reindex();
      }
    }
  }
  return next;
}

/**
 * The library regrouped under one separator per category, sections in `order`. Mods keep
 * their relative order within a section and the list's own separators are dropped.
 */
export function groupByCategory(rows: Row[], of: Record<string, string>, order: string[]): Row[] {
  const cat = (m: ModRow) => (order.includes(of[m.id]) ? of[m.id] : order[order.length - 1]);
  const mods = rows.filter(isMod);
  const out: Row[] = [];
  for (const name of order) {
    const section = mods.filter((m) => cat(m) === name);
    if (section.length) out.push({ kind: "sep", name }, ...section);
  }
  return out;
}

/**
 * The most stable load order PZMM knows, for a load list: turn on what enabled mods need,
 * frameworks first and translations last (by category), Mod Load Order Sorter's loadFirst /
 * loadLast ("on": the whole list, "category": within its tier), then satisfy every require /
 * loadModAfter / loadModBefore rule while keeping your order everywhere else.
 */
export function autosort(list: Row[], of: Record<string, string>, order: string[], canLoad: (r: ModRow) => boolean = () => true): Row[] {
  const next = enableRequirements(list, canLoad);
  const tier = tiers(of, order);
  const edge = (m: ModRow, scope: string) => (m.load_first === scope ? 0 : m.load_last === scope ? 2 : 1);
  const rank = (m: ModRow) => edge(m, "on") * 100 + tier(m.id) * 10 + edge(m, "category");
  const on = next.filter((r): r is ModRow => isMod(r) && r.on).sort((a, b) => rank(a) - rank(b)); // stable
  return pullUp([...on, ...next.filter((r) => !(isMod(r) && r.on))]);
}

/**
 * Move each of `ids` to the end of its category's section in the library, adding the
 * section (in `order`) when the list doesn't have it yet.
 */
export function placeInSections(rows: Row[], of: Record<string, string>, order: string[], ids: string[]): Row[] {
  const next = [...rows];
  for (const id of ids) {
    const name = of[id];
    const from = next.findIndex((r) => isMod(r) && r.id === id);
    if (!order.includes(name) || from < 0) continue;
    const [m] = next.splice(from, 1);
    let sep = next.findIndex((r) => r.kind === "sep" && r.name === name);
    if (sep < 0) {
      sep = next.findIndex((r) => r.kind === "sep" && order.indexOf(r.name) > order.indexOf(name));
      // last section: before the loose mods at the bottom (after the break), which stay last
      const last = next.findLastIndex((r) => r.kind === "sep" && r.name !== "");
      if (sep < 0) sep = next.findIndex((r, i) => i > last && r.kind === "sep");
      if (sep < 0) sep = next.length;
      next.splice(sep, 0, { kind: "sep", name });
    }
    const end = next.findIndex((r, i) => i > sep && r.kind === "sep");
    next.splice(end < 0 ? next.length : end, 0, m);
  }
  return next;
}

/** The game's own map, which mod maps go before (`server::BASE_MAP`). */
export const BASE_MAP = "Muldraugh, KY";

/** The server's Map= list sorted: mod maps keep their order, before the game's own map. */
export const sortMaps = (maps: string[]) => [...maps.filter((m) => m !== BASE_MAP), ...maps.filter((m) => m === BASE_MAP)];

/**
 * Autosort a Map= list: a map whose map.info `lots=` names another listed map builds on it, so
 * it moves to just before that map; everything else keeps its order; Muldraugh last. Maps that
 * build on each other in a loop can't be sorted: the order is kept.
 */
export function autosortMaps(maps: string[], lotsOf: (folder: string) => string[]): string[] {
  const out = sortMaps(maps);
  for (let pass = 0; pass <= out.length; pass++) {
    let moved = false;
    for (let i = 0; i < out.length; i++) {
      const lots = lotsOf(out[i]);
      const j = out.findIndex((m, k) => k < i && lots.includes(m));
      if (j >= 0) {
        out.splice(j, 0, ...out.splice(i, 1));
        moved = true;
      }
    }
    if (!moved) return out;
  }
  return sortMaps(maps);
}

/**
 * Where a background picture lies, from two spots on it (`img`, in pixels) and where each is in
 * the world (`world`, in tiles): a scale and offset per axis (world = img * scale + offset, no
 * rotation). Null when the spots are under a pixel apart across or down, or the picture would be
 * mirrored or flat on an axis.
 */
export function solveBackground(a: { img: number[]; world: number[] }, b: { img: number[]; world: number[] }): { scale: [number, number]; offset: [number, number] } | null {
  const scale = [0, 1].map((i) => (b.world[i] - a.world[i]) / (b.img[i] - a.img[i]));
  if ([0, 1].some((i) => Math.abs(b.img[i] - a.img[i]) < 1 || !(scale[i] > 0) || !isFinite(scale[i]))) return null;
  return { scale: [scale[0], scale[1]], offset: [a.world[0] - scale[0] * a.img[0], a.world[1] - scale[1] * a.img[1]] };
}

/**
 * Where a section dragged from `from` lands when dropped on row `to`: past the hovered
 * section when dragging down, before it when dragging up (the index it's inserted before).
 */
export function sectionDrop(rows: Row[], from: number, to: number): number {
  let t = Math.min(to, rows.length);
  if (t > from) while (t < rows.length && rows[t].kind !== "sep") t++;
  else while (t > 0 && rows[t].kind !== "sep") t--;
  return t;
}

/** Move the section that starts at separator `from` (it and its mods) to drop row `to`; null when nothing moves. */
export function moveSection(rows: Row[], from: number, to: number): Row[] | null {
  let end = from + 1;
  while (end < rows.length && rows[end].kind !== "sep") end++;
  const t = sectionDrop(rows, from, to);
  if (t >= from && t <= end) return null;
  const block = rows.slice(from, end);
  const rest = [...rows.slice(0, from), ...rows.slice(end)];
  rest.splice(t > from ? t - block.length : t, 0, ...block);
  return rest;
}

/**
 * The groups Find the broken mod tests together: `enabled` (in load order) joined by each requirement that is
 * enabled too, or by the enabled mods a requirement that's off needs (one not installed joins nothing). Each connected set is a group, in load order, groups by
 * their first member, so a mod is never on without the enabled mods it needs. The game makes the same groups
 * (tests/fixtures/bisect-groups.json).
 */
export function bisectGroups(enabled: string[], requires: (id: string) => string[]): string[][] {
  const at = new Map(enabled.map((id, i) => [id, i]));
  const root = enabled.map((_, i) => i);
  const find = (i: number): number => (root[i] === i ? i : (root[i] = find(root[i])));
  enabled.forEach((id, i) => {
    // a requirement that's off is followed through to the enabled mods it needs (each mod once: cycles end)
    const seen = new Set([id]);
    const follow = (r: string) => {
      if (seen.has(r)) return;
      seen.add(r);
      const j = at.get(r);
      if (j !== undefined) root[find(i)] = find(j);
      else requires(r).forEach(follow);
    };
    requires(id).forEach(follow);
  });
  const groups = new Map<number, string[]>();
  enabled.forEach((id, i) => {
    const k = find(i);
    const g = groups.get(k);
    if (g) g.push(id);
    else groups.set(k, [id]);
  });
  return [...groups.values()];
}

/**
 * One bisect test: every enabled mod outside the suspect groups stays on, plus the first half (or the first
 * `count`) of the suspect groups; `alone`: only the suspect groups (the check on the last one). `keep` stays on in
 * every round, alone too (mods this build can't load; a search from before groups: everything not a suspect, as
 * its suspects aren't closed over requirements). Returns the ids to enable and the groups under test.
 */
export function bisectRound(original: Row[], suspects: string[][], count = Math.ceil(suspects.length / 2), alone = false, keep: string[] = []): { on: Set<string>; testing: string[][] } {
  if (alone) return { on: new Set([...keep, ...suspects.flat()]), testing: suspects };
  const testing = suspects.slice(0, count);
  const suspect = new Set(suspects.flat());
  const on = new Set(original.filter((r): r is ModRow => isMod(r) && r.on && !suspect.has(r.id)).map((r) => r.id));
  testing.flat().forEach((id) => on.add(id));
  return { on, testing };
}

/** A search saved before groups (lists of ids): its suspects grouped, testing the groups it was testing, and every
 * other mod that was on kept on (an old suspect can need one of them). */
export function bisectUpgrade<T extends { suspects: unknown[]; testing: unknown[]; original: { rows: Row[] } }>(b: T, requires: (id: string) => string[]): T & { suspects: string[][]; testing: string[][]; keep?: string[] } {
  if (!b.suspects.some((s) => typeof s === "string")) return b as T & { suspects: string[][]; testing: string[][] };
  const was = new Set((b.testing as (string | string[])[]).flat());
  const old = new Set(b.suspects as string[]);
  const suspects = bisectGroups(b.suspects as string[], requires);
  const keep = b.original.rows.filter((r): r is ModRow => isMod(r) && r.on && !old.has(r.id)).map((r) => r.id);
  return { ...b, suspects, testing: suspects.filter((g) => g.some((id) => was.has(id))), keep };
}

/**
 * After a test: still broken narrows to the groups tested, fixed to the others. One group left because it was
 * tested and still broken is the answer; one left only because the others were cleared is tested alone first
 * (`confirm`), and if it alone is fine, it breaks only together with mods in another group (`together`: turning
 * it off still fixes the list played).
 */
export function bisectNext(s: { suspects: string[][]; testing: string[][]; confirm?: boolean }, stillBroken: boolean): { found: string[] } | { together: string[] } | { none: true } | { suspects: string[][]; confirm: boolean } {
  if (s.confirm) return stillBroken ? { found: s.suspects[0] } : { together: s.suspects[0] };
  const tested = new Set(s.testing.flat());
  const next = stillBroken ? s.testing : s.suspects.filter((g) => !g.some((id) => tested.has(id)));
  if (!next.length) return { none: true };
  if (next.length === 1 && stillBroken) return { found: next[0] };
  return { suspects: next, confirm: next.length === 1 };
}

/** Mods that only `ids` require (directly or through each other): removing `ids` leaves them unused. */
export function orphanedRequirements(ids: string[], mods: ModRow[]): string[] {
  const gone = new Set(ids);
  const installed = new Set(mods.map((m) => m.id));
  const out: string[] = [];
  for (let i = 0; i < out.length + ids.length; i++) {
    const id = i < ids.length ? ids[i] : out[i - ids.length];
    for (const r of mods.find((m) => m.id === id)?.requires ?? []) {
      if (gone.has(r) || !installed.has(r) || mods.some((m) => !gone.has(m.id) && m.requires.includes(r))) continue;
      gone.add(r);
      out.push(r);
    }
  }
  return out;
}

/** A preset as the game's mod menu shares it (`name:A;B;C`, Build 42 `name:\A;\B`), or null when the text isn't
 * one: the list must use the game's separators (a `;`, or ids starting with `\`) and every id must look like a mod
 * id (a letter in it, no path or link characters), so a pasted link or "Server IP: 192.168.1.5" isn't taken for one. */
export function parsePreset(text: string): { name: string; mods: string[] } | null {
  const at = text.indexOf(":");
  const name = text.slice(0, Math.max(0, at)).trim();
  const list = text.slice(at + 1).trim();
  const mods = list.split(/[;,\n]/).map((m) => m.trim().replace(/^\\/, "")).filter(Boolean);
  const listed = list.includes(";") || list.startsWith("\\");
  return name && listed && mods.length && mods.every((m) => /\p{L}/u.test(m) && !/[/\\:<>"|?*]/.test(m)) ? { name, mods } : null;
}

/** For each mod, the enabled mods that require it directly (the "Needed by" badge). */
export function neededBy(rows: Row[]): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const r of rows) if (isMod(r) && r.on) for (const req of r.requires ?? []) if (req !== r.id) out.set(req, [...(out.get(req) ?? []), r.id]);
  return out;
}

/** The enabled mods that need any of `ids`, directly or through each other (`ids` themselves left out). */
export function dependants(ids: string[], needed: Map<string, string[]>): string[] {
  const seen = new Set(ids);
  const out: string[] = [];
  const visit = (id: string) => {
    for (const d of needed.get(id) ?? []) if (!seen.has(d)) seen.add(d), out.push(d), visit(d);
  };
  ids.forEach(visit);
  return out;
}

/** The load list A-Z by name (loadFirst mods above, loadLast below), then moved only as far as requirements and load rules need. */
export function sortByName(list: ModRow[], canLoad: (r: ModRow) => boolean = () => true): Row[] {
  const rank = (m: ModRow) => (m.load_first === "on" ? 0 : m.load_last === "on" ? 2 : 1);
  const name = (m: ModRow) => (m.name || m.id).toLowerCase();
  // in the app's language (the page's lang), not the system's
  const { compare } = new Intl.Collator(globalThis.document?.documentElement.lang || undefined);
  const on = list.filter((m) => m.on).sort((a, b) => rank(a) - rank(b) || compare(name(a), name(b)));
  return sortLoadOrder([...on, ...list.filter((m) => !m.on)], canLoad);
}

/** A server's mods against the installed ones: how many it needs that aren't installed (`get`), out of
 * `total`, and how many it runs but doesn't list (`unlisted`: outside the game only the first ~127
 * characters of its Mods= line show, so they're unknown). Its full PZMM list, once worked out, counts instead. */
export function modNeed(server: GameServer, installed: Set<string>, mods?: ServerMods | null) {
  if (mods?.list === "full") {
    const need = mods.mods.filter((m) => !mods.server_only.includes(m.id) && !mods.optional.includes(m.id));
    return { get: need.filter((m) => !installed.has(m.id)).length, total: need.length, unlisted: 0 };
  }
  return { get: server.mods.filter((m) => !installed.has(m)).length, total: server.mod_count, unlisted: Math.max(0, server.mod_count - server.mods.length) };
}

/** Home's Popular list as kept between runs (`text`: what was stored), or null for anything not in its shape. */
export function keptServers(text: string | null): { at: number; steam: boolean; list: GameServer[] } | null {
  try {
    const p = JSON.parse(text ?? "null");
    const ok = p && typeof p.at === "number" && Array.isArray(p.list) && p.list.every((s: GameServer) => s && typeof s.addr === "string" && typeof s.name === "string" && Array.isArray(s.mods));
    return ok ? { at: p.at, steam: p.steam === true, list: p.list } : null;
  } catch {
    return null;
  }
}
