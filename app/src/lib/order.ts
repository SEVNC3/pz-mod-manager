// Load-order rules from mod.info: require, incompatible, loadModAfter, loadModBefore.
// Pure functions so they can be checked outside the app (scripts/order-check.ts).
// Two lists: the library (`Row[]`, organised by the player, order is cosmetic) and the
// load order (enabled mod ids). Rule checks and sorts run on a load list: the library's
// mods with the enabled ones first, in load order (see `loadList`).
import type { ModRow, Row } from "./api";

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
export function sortLoadOrder(rows: Row[]): Row[] {
  const next = enableRequirements(rows);

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
function enableRequirements(rows: Row[]): Row[] {
  const next: Row[] = rows.map((r) => ({ ...r }));
  const byId = new Map(next.filter(isMod).map((r) => [r.id, r]));
  for (let changed = true; changed; ) {
    changed = false;
    for (const r of next) {
      if (!isMod(r) || !r.on) continue;
      for (const req of r.requires ?? []) {
        const dep = byId.get(req);
        if (dep && !dep.on && !dep.missing) {
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
 * frameworks first and translations last (by category), then satisfy every require /
 * loadModAfter / loadModBefore rule while keeping your order everywhere else.
 */
export function autosort(list: Row[], of: Record<string, string>, order: string[]): Row[] {
  const next = enableRequirements(list);
  const tier = tiers(of, order);
  const on = next.filter((r) => isMod(r) && r.on).sort((a, b) => tier((a as ModRow).id) - tier((b as ModRow).id)); // stable
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
      if (sep < 0) sep = next.length;
      next.splice(sep, 0, { kind: "sep", name });
    }
    const end = next.findIndex((r, i) => i > sep && r.kind === "sep");
    next.splice(end < 0 ? next.length : end, 0, m);
  }
  return next;
}

/** The server's Map= list sorted: mod maps keep their order, before the game's own map. */
export const sortMaps = (maps: string[], base = "Muldraugh, KY") => [...maps.filter((m) => m !== base), ...maps.filter((m) => m === base)];

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
 * One bisect test: everything enabled that isn't a suspect stays on, plus the first half
 * of the suspects and (transitively) what they require. Returns the ids to enable and
 * the half under test.
 */
export function bisectRound(original: Row[], suspects: string[], requires: (id: string) => string[]): { on: Set<string>; testing: string[] } {
  const testing = suspects.slice(0, Math.ceil(suspects.length / 2));
  const suspect = new Set(suspects);
  const on = new Set(original.filter((r): r is ModRow => isMod(r) && r.on && !suspect.has(r.id)).map((r) => r.id));
  const add = (id: string) => {
    if (on.has(id)) return;
    on.add(id);
    requires(id).forEach(add);
  };
  testing.forEach(add);
  return { on, testing };
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
