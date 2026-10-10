// Load-order rules from mod.info: require, incompatible, loadModAfter, loadModBefore.
// Pure functions so they can be checked outside the app (scripts/order-check.ts).
import type { ModRow, Row } from "./api";

export type Problem = { text: string; fixable: boolean };

const isMod = (r: Row): r is ModRow => r.kind === "mod";

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
      if (!dep || dep.missing) return { text: `Needs ${req} (not installed)`, fixable: false };
      if (!dep.on) return { text: `Needs ${req} (disabled)`, fixable: true };
      if (pos.get(req)! > i) return { text: `${req} must load first`, fixable: true };
    }
    for (const x of r.incompatible ?? []) if (enabled(x)) return { text: `Incompatible with ${x}`, fixable: false };
    for (const a of r.load_after ?? []) if (enabled(a) && pos.get(a)! > i) return { text: `Should load after ${a}`, fixable: true };
    for (const b of r.load_before ?? []) if (enabled(b) && pos.get(b)! < i) return { text: `Should load before ${b}`, fixable: true };
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
 * The list regrouped under one separator per category, sections in `order`. Mods keep
 * their relative order within a section and the list's own separators are dropped. A
 * requirement in a later section moves up to just above the mod that needs it.
 */
export function groupByCategory(rows: Row[], of: Record<string, string>, order: string[]): Row[] {
  const cat = (m: ModRow) => (order.includes(of[m.id]) ? of[m.id] : order[order.length - 1]);
  const mods = rows.filter(isMod);
  const out: Row[] = [];
  for (const name of order) {
    const section = mods.filter((m) => cat(m) === name);
    if (section.length) out.push({ kind: "sep", name }, ...section);
  }
  return pullUp(out);
}

/**
 * The most stable order PZMM knows: turn on what enabled mods need, group by category
 * (frameworks first, translations last), then satisfy every require / loadModAfter /
 * loadModBefore rule while keeping your order everywhere else.
 */
export function autosort(rows: Row[], of: Record<string, string>, order: string[]): Row[] {
  return groupByCategory(enableRequirements(rows), of, order);
}

/**
 * Move each of `ids` to the end of its category's section, adding the section (in
 * `order`) when the list doesn't have it yet.
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
  return pullUp(next);
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
