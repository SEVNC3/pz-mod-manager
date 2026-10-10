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
