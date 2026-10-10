<script lang="ts" module>
  import { listen } from "@tauri-apps/api/event";
  import type { MapScan } from "./api";

  // the Workshop scan outlives the tab (it runs in the backend), so its state lives here
  // `build`: the build it last ran for (switching builds scans again); `stopping`: Stop was clicked
  const scan = $state({ running: false, p: null as MapScan | null, error: "", ticks: 0, started: false, build: 0, stopping: false });
  let listening = false;
  function follow() {
    if (listening) return;
    listening = true;
    listen<MapScan>("maps-scan", (e) => {
      // show what's read so far every 50 items
      if (e.payload.phase === "reading" && e.payload.done !== scan.p?.done && e.payload.done % 50 === 0) scan.ticks++;
      scan.p = e.payload;
    }).catch(() => (listening = false));
  }
</script>

<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, type Cell, type GridCell, type MapCompare, type MapMod, type MapsView, type ModRow, type WorkshopItem } from "./api";
  import Icon from "./Icon.svelte";
  import MapOrder from "./MapOrder.svelte";
  import { compact, fmtSize, workshopUrl } from "./format";
  import { t, th, type Vars } from "./i18n.svelte";
  import { prefs } from "./prefs.svelte";

  let { busy, mods, say, onSetMods, onInstallWorkshop, onShowMod, canUndo, onUndo }: {
    busy: boolean;
    /** The library by mod id: what's installed and on. */
    mods: Map<string, ModRow>;
    /** A line in the activity log. */
    say: (key: string, vars?: Vars, error?: boolean) => void;
    /** Turn mods on (with what they need) or off, through the load order. */
    onSetMods: (ids: string[], on: boolean) => Promise<boolean>;
    onInstallWorkshop: (id: string) => Promise<boolean>;
    /** Open a mod's details in My mods. */
    onShowMod: (id: string) => void;
    /** The app's undo history (enabling and disabling are list edits). */
    canUndo: boolean;
    onUndo: () => void;
  } = $props();

  let data = $state.raw<MapsView | null>(null);
  /** Maps shown on the grid without being enabled. */
  let preview = $state<Set<string>>(new Set());
  /** Clicked cells (shift-click adds): the list shows only the maps covering one of them. */
  let pickedCells = $state<Cell[]>([]);

  const load = () => api.mapsView().then((v) => (data = v)).catch((e) => say(String(e), {}, true));
  // after installs, removals and toggles (busy flips back), like Details; as the scan reads, and
  // when it ends (even if the tab was closed and opened again meanwhile)
  $effect(() => {
    void scan.ticks;
    void scan.running;
    if (!busy) load();
  });

  const itemOf = (source: string) => source.match(/^(?:steam|workshop):(\d+)$/)?.[1];
  const metaOf = (m: MapMod) => data?.items[itemOf(m.source) ?? ""];
  /** Installed maps, then Workshop maps read without installing: hand-checked ones always, the rest with the Workshop source on. */
  const all = $derived(data ? [...data.mods, ...data.checked.filter((m) => prefs.mapsWorkshop || metaOf(m)?.manual)] : []);
  const nameOf = (m: MapMod) => m.name || metaOf(m)?.title || m.id;
  const names = $derived(new Map(all.map((m) => [m.id, nameOf(m)])));
  const isOn = (id: string) => !!mods.get(id)?.on;
  const shown = $derived(all.filter((m) => isOn(m.id) || preview.has(m.id)));
  /** Shown maps left off the grid (to see what's under them); they still count as shown elsewhere. */
  let hidden = $state<Set<string>>(new Set());
  const drawn = $derived(shown.filter((m) => !hidden.has(m.id)));
  const cellCount = (m: MapMod) => m.maps.reduce((n, f) => n + f.cells.length, 0);
  const key = ([x, y]: Cell) => `${x},${y}`;

  let grid = $state.raw<GridCell[]>([]);
  let gridFor = 0;
  $effect(() => {
    const ask = ++gridFor;
    api.mapGrid(data?.vanilla ?? [], data?.water ?? [], drawn).then((g) => ask === gridFor && (grid = g)).catch((e) => say(String(e), {}, true));
  });
  const byCell = $derived(new Map(grid.map((c) => [`${c.x},${c.y}`, c])));
  const vanillaSet = $derived(new Set((data?.vanilla ?? []).map(key)));
  /** Per map (shown or not): the shown maps it shares cells with (and which cells), and its cells over vanilla. */
  const clashes = $derived.by(() => {
    const out = new Map<string, { with: Map<string, Cell[]>; over: number }>();
    for (const m of all) {
      const e = { with: new Map<string, Cell[]>(), over: 0 };
      for (const f of m.maps)
        for (const c of f.cells) {
          if (vanillaSet.has(key(c))) e.over++;
          for (const id of byCell.get(key(c))?.maps ?? []) if (id !== m.id) e.with.set(id, [...(e.with.get(id) ?? []), c]);
        }
      out.set(m.id, e);
    }
    return out;
  });
  const conflictCells = $derived(grid.filter((c) => c.kind === "conflict").length);
  const overCells = $derived(grid.filter((c) => c.kind === "over").length);

  // --- undo: preview changes are kept here, enabling and disabling in the app's history ---
  /** The preview before a change, and whether the change also enabled or disabled mods. */
  type Step = { preview: Set<string>; app: boolean };
  let history = $state.raw<Step[]>([]);
  /** Change the preview (`app`: right after an enable/disable, undone with it). */
  function setPreview(next: Set<string>, app = false) {
    history = [...history.slice(-49), { preview, app }];
    preview = next;
  }
  async function setMods(ids: string[], on: boolean) {
    const ok = await onSetMods(ids, on);
    if (ok) setPreview(preview, true);
    return ok;
  }
  function undo() {
    const last = history.at(-1);
    if (!last) return onUndo(); // edits made in My mods
    history = history.slice(0, -1);
    if (last.app) onUndo();
    preview = last.preview;
  }
  function undoKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
    if (typing || busy || !(e.ctrlKey || e.metaKey) || e.shiftKey || e.key.toLowerCase() !== "z") return;
    e.preventDefault();
    undo();
  }

  function togglePreview(id: string) {
    const next = new Set(preview);
    next.has(id) ? next.delete(id) : next.add(id);
    setPreview(next);
  }
  function toggleHidden(id: string) {
    const next = new Set(hidden);
    next.has(id) ? next.delete(id) : next.add(id);
    hidden = next;
  }

  // --- the list: search, filter, sort ---
  let search = $state("");
  type Filter = "all" | "on" | "installed" | "missing" | "conflict" | "over";
  let filter = $state<Filter>("all");
  type Sort = "name" | "cells" | "conflicts" | "state" | "subs" | "updated" | "newest";
  let sortBy = $state<Sort>("name");
  const FILTERS: [Filter, string][] = [["all", "maps.filter.all"], ["on", "common.enabled"], ["installed", "common.installed"], ["missing", "maps.notInstalled"], ["conflict", "maps.filter.conflict"], ["over", "maps.filter.over"]];
  const stateRank = (m: MapMod) => (isOn(m.id) ? 0 : mods.has(m.id) ? 1 : 2);
  const listed = $derived.by(() => {
    const words = search.toLowerCase().split(/\s+/).filter(Boolean);
    const out = all.filter((m) => {
      const c = clashes.get(m.id);
      const pass = { all: true, on: isOn(m.id), installed: mods.has(m.id), missing: !mods.has(m.id), conflict: !!c?.with.size, over: !!c?.over }[filter];
      if (!pass) return false;
      if (pickedCells.length && !pickedCells.some(([x, y]) => m.maps.some((f) => f.cells.some((c) => c[0] === x && c[1] === y)))) return false;
      if (!words.length) return true;
      const meta = metaOf(m);
      const hay = [nameOf(m), m.id, itemOf(m.source) ?? "", meta?.author ?? "", meta?.title ?? "", ...(meta?.tags ?? []), ...m.maps.map((f) => f.folder)].join("\n").toLowerCase();
      return words.every((w) => hay.includes(w));
    });
    const by: Record<Sort, (a: MapMod, b: MapMod) => number> = {
      name: () => 0,
      cells: (a, b) => cellCount(b) - cellCount(a),
      conflicts: (a, b) => (clashes.get(b.id)?.with.size ?? 0) - (clashes.get(a.id)?.with.size ?? 0),
      state: (a, b) => stateRank(a) - stateRank(b),
      subs: (a, b) => (metaOf(b)?.subscriptions ?? -1) - (metaOf(a)?.subscriptions ?? -1),
      updated: (a, b) => (metaOf(b)?.updated ?? -1) - (metaOf(a)?.updated ?? -1),
      newest: (a, b) => (metaOf(b)?.created ?? -1) - (metaOf(a)?.created ?? -1),
    };
    return out.sort((a, b) => by[sortBy](a, b) || nameOf(a).localeCompare(nameOf(b)));
  });
  // thousands of Workshop maps: draw the list a page at a time
  let limit = $state(150);
  $effect(() => {
    [search, filter, sortBy, pickedCells, prefs.mapsWorkshop];
    limit = 150;
  });

  // --- bulk actions ---
  const previewOff = $derived(shown.filter((m) => preview.has(m.id) && mods.has(m.id) && !isOn(m.id)).map((m) => m.id));
  const listedOn = $derived(listed.filter((m) => isOn(m.id)).map((m) => m.id));
  async function enablePreviewed() {
    const ids = previewOff;
    if (await onSetMods(ids, true)) setPreview(new Set([...preview].filter((id) => !ids.includes(id))), true);
  }
  const previewListed = () => setPreview(new Set([...preview, ...listed.filter((m) => !isOn(m.id)).map((m) => m.id)]));
  const clearListed = () => {
    const ids = new Set(listed.map((m) => m.id));
    setPreview(new Set([...preview].filter((id) => !ids.has(id))));
  };

  /** Preview the most maps (of those listed) that fit around the enabled ones: installed and popular ones win ties. */
  let picking = $state(false);
  async function quickSelect() {
    picking = true;
    try {
      const ranks = Object.fromEntries(listed.map((m) => [m.id, (mods.has(m.id) ? 2 ** 40 : 0) + (metaOf(m)?.subscriptions ?? 0)]));
      // every enabled map, listed or not, keeps its cells
      const pick = await api.mapQuickSelect(listed, all.filter((m) => isOn(m.id)), ranks);
      setPreview(new Set(pick));
      say(pick.length ? "maps.quickSelectDone" : "maps.quickSelectNone", { count: pick.length });
    } catch (e) {
      say(String(e), {}, true);
    } finally {
      picking = false;
    }
  }

  /** Show `m` instead of the shown maps it conflicts with (previewed ones are hidden, enabled ones turned off). */
  async function swap(m: MapMod) {
    const rivals = [...(clashes.get(m.id)?.with.keys() ?? [])];
    const off = rivals.filter(isOn);
    if (off.length && !(await onSetMods(off, false))) return;
    const next = new Set([...preview].filter((id) => !rivals.includes(id)));
    if (!isOn(m.id)) next.add(m.id);
    setPreview(next, off.length > 0); // one undo step
  }

  // --- mod packs and duplicates ---
  const installedPerItem = $derived.by(() => {
    const n = new Map<string, number>();
    for (const r of mods.values()) n.set(r.source, (n.get(r.source) ?? 0) + 1);
    return n;
  });
  /** How many mods `m`'s Workshop item holds (more than one: a mod pack). */
  const packSize = (m: MapMod) => metaOf(m)?.mods || installedPerItem.get(m.source) || 1;
  /** Map folder -> the maps that add it (more than one: only one loads). */
  const folderOwners = $derived.by(() => {
    const out = new Map<string, string[]>();
    for (const m of all) for (const f of m.maps) out.set(f.folder, [...(out.get(f.folder) ?? []), m.id]);
    return out;
  });
  /** Other Workshop items that provide `m`'s mod id. */
  const otherItems = (m: MapMod) => (data?.providers[m.id] ?? []).filter((it) => it !== itemOf(m.source));
  const cellList = (cells: Cell[]) => cells.slice(0, 3).map(([x, y]) => `${x},${y}`).join(" · ") + (cells.length > 3 ? ` +${cells.length - 3}` : "");

  // --- the Workshop source: every Map-tagged item, read from Steam's file lists ---
  async function startScan() {
    follow();
    Object.assign(scan, { running: true, error: "", started: true, stopping: false, build: data?.build ?? 0 });
    try {
      const p = await api.mapScan();
      scan.p = p;
      if (p.failed) say("maps.scan.failed", { count: p.failed }, true);
    } catch (e) {
      scan.error = String(e);
    } finally {
      scan.running = false; // the load effect picks up the last batch
    }
  }
  function stopScan() {
    scan.stopping = true;
    api.mapScanStop().catch(() => {});
  }
  $effect(() => {
    // once per session and build: later scans only read new and updated items
    const build = data?.build;
    if (prefs.mapsWorkshop && build && untrack(() => !scan.running && (!scan.started || scan.build !== build))) startScan();
  });
  const workshopCount = $derived(data ? Object.keys(data.items).length : 0);
  const scanText = $derived.by(() => {
    const p = scan.p;
    if (scan.error) return /429|limiting/i.test(scan.error) ? t("maps.scan.limited") : scan.error.replace(/^Error:\s*/, "");
    if (scan.running && scan.stopping) return t("maps.scan.stopping");
    if (!scan.running || !p) return t("maps.scan.summary", { n: data?.checked.length ?? 0, count: workshopCount });
    if (p.phase === "listing") return t("maps.scan.listing", { done: p.done, total: p.total || "?" });
    return t(p.failed ? "maps.scan.readingFailed" : "maps.scan.reading", { done: p.done, total: p.total, failed: p.failed });
  });

  // --- requirements: required mods (require=) and map folders (lots=) the shown maps need ---
  type Group = "enabled" | "preview" | "missing";
  type Need = { key: string; id: string; folder: string; by: string[]; group: Group };
  const GROUPS: [Group, string][] = [["enabled", "maps.req.groups.enabled"], ["preview", "maps.req.groups.preview"], ["missing", "maps.req.groups.missing"]];
  const mapById = $derived(new Map(all.map((m) => [m.id, m])));
  /** What a mod requires: the installed one's mod.info, else a Workshop map's. */
  const requiresOf = (id: string) => mods.get(id)?.requires ?? mapById.get(id)?.requires ?? [];
  /** Everything `m` needs, directly or through what it needs. */
  function allRequires(m: MapMod) {
    const seen = new Set<string>([m.id]);
    const walk = (ids: string[]): void => {
      for (const r of ids)
        if (!seen.has(r)) {
          seen.add(r);
          walk(requiresOf(r));
        }
    };
    walk(m.requires);
    seen.delete(m.id);
    return [...seen];
  }
  const needs = $derived.by(() => {
    const out = new Map<string, Need>();
    const order: Group[] = ["enabled", "preview", "missing"];
    const add = (key: string, id: string, folder: string, m: MapMod) => {
      const group: Group = isOn(m.id) ? "enabled" : mods.has(m.id) ? "preview" : "missing";
      const n = out.get(key) ?? { key, id, folder, by: [], group };
      if (!n.by.includes(m.id)) n.by.push(m.id);
      if (order.indexOf(group) < order.indexOf(n.group)) n.group = group;
      out.set(key, n);
    };
    for (const m of shown) {
      for (const r of allRequires(m)) add(r, r, "", m);
      for (const lot of m.maps.flatMap((f) => f.lots)) {
        if (data?.vanilla_folders.includes(lot) || m.maps.some((f) => f.folder === lot)) continue;
        const p = all.find((o) => o.maps.some((f) => f.folder === lot));
        add(p?.id ?? `map:${lot}`, p?.id ?? "", lot, m);
      }
    }
    return [...out.values()].sort((a, b) => (a.id || a.folder).localeCompare(b.id || b.folder));
  });
  /** The Requirements card: each requirement and the maps that need it, or each map's tree. */
  let reqView = $state<"list" | "tree">("list");
  /** A requirement's Workshop item: from the installed mod, else a Workshop map that is it. */
  const needItem = (n: Need) => (n.id ? itemOf(mods.get(n.id)?.source ?? "") ?? itemOf(all.find((m) => m.id === n.id)?.source ?? "") : undefined);
  /** Workshop lookups for missing requirements, by mod id: the message's key and vars. */
  let finding = $state<Record<string, [string, Vars]>>({});
  async function installNeed(n: Need) {
    const known = all.find((m) => m.id === n.id && !mods.has(m.id));
    const item = known && itemOf(known.source);
    if (item) return onInstallWorkshop(item);
    finding[n.id] = ["maps.req.looking", { id: n.id }];
    try {
      const list = await api.findRequirement(n.id, n.by.find((b) => mods.has(b)) ?? n.by[0]);
      if (list[0]?.confirmed && list[0]?.fits_build) {
        delete finding[n.id];
        await onInstallWorkshop(list[0].id);
      } else {
        finding[n.id] = [list.length ? "maps.req.noClearResult" : "maps.req.noMatch", { id: n.id }];
      }
    } catch (e) {
      finding[n.id] = [String(e), {}];
    }
  }

  // --- Map= order (kept with the profile) and export ---
  let mapLine = $state<string[]>([]);
  const folders = $derived(shown.flatMap((m) => m.maps.map((f) => f.folder)));
  $effect(() => {
    const f = folders;
    api.mapLine(untrack(() => $state.snapshot(mapLine)), f).then((l) => (mapLine = l)).catch(() => {});
  });
  // saved (and given to the game) once the arranging stops for a moment
  $effect(() => {
    const line = $state.snapshot(mapLine);
    if (!line.length) return;
    const t = setTimeout(() => api.mapOrderSave(line).catch((e) => say(String(e), {}, true)), 600);
    return () => clearTimeout(t);
  });

  async function exportMaps() {
    try {
      await navigator.clipboard.writeText(await api.mapsExport(shown, $state.snapshot(mapLine)));
      say("maps.export.copied", { count: shown.length });
    } catch (e) {
      say(String(e), {}, true);
    }
  }

  // --- presets: named sets of shown maps ---
  let presets = $state<Record<string, string[]>>({});
  let presetName = $state("");
  let presetPick = $state("");
  onMount(() => {
    api.mapPresets().then((p) => (presets = p)).catch(() => {});
  });
  async function savePreset(e: Event) {
    e.preventDefault();
    const name = presetName.trim();
    if (!name) return;
    if (presets[name] && !confirm(t("maps.presets.confirmReplace", { name }))) return;
    try {
      presets = await api.mapPresetSet(name, shown.map((m) => m.id));
      presetPick = name;
      presetName = "";
      say("maps.presets.saved", { count: shown.length, name });
    } catch (err) {
      say(String(err), {}, true);
    }
  }
  function loadPreset() {
    const ids = presets[presetPick] ?? [];
    const known = new Set(all.map((m) => m.id));
    setPreview(new Set(ids.filter((id) => known.has(id) && !isOn(id))));
    const gone = ids.filter((id) => !known.has(id));
    say(gone.length ? "maps.presets.previewingMissing" : "maps.presets.previewing", { name: presetPick, ids: gone.join(", ") }, gone.length > 0);
  }
  async function deletePreset() {
    try {
      presets = await api.mapPresetSet(presetPick, null);
      presetPick = "";
    } catch (err) {
      say(String(err), {}, true);
    }
  }

  // --- compare with a server's Map= ---
  let servers = $state<string[]>([]);
  let server = $state("");
  let compared = $state<MapCompare | null>(null);
  onMount(() => {
    api.serversAndSaves().then((d) => ((servers = d.servers), (server = d.servers[0] ?? ""))).catch(() => {});
  });
  async function compareServer() {
    try {
      compared = await api.mapServerCompare(server, [...new Set(folders)]);
    } catch (err) {
      say(String(err), {}, true);
    }
  }
  const byId = (id: string | null) => (id ? all.find((m) => m.id === id) : undefined);

  // --- check one Workshop map without installing it ---
  let checkText = $state("");
  let checking = $state(false);
  async function check(e: Event) {
    e.preventDefault();
    const id = checkText.trim().match(/(?:id=)?(\d{4,})/)?.[1];
    if (!id) return say("maps.check.pasteId", {}, true);
    checking = true;
    try {
      const { item, members } = await api.workshopInfo(id);
      if (!item) throw new Error(t("maps.check.noItem", { id }));
      if (item.collection) return await checkCollection(item.title, members);
      if (item.have) return say("maps.check.alreadyInstalled", { title: item.title });
      let found;
      try {
        found = await api.mapCheck(id, false);
      } catch (err) {
        if (!confirm(t("maps.check.confirmDownload", { title: item.title, error: String(err).replace(/^Error:\s*/, ""), size: fmtSize(item.file_size) }))) return;
        found = await api.mapCheck(id, true);
      }
      checkText = "";
      setPreview(new Set([...preview, ...found.map((m) => m.id)]));
      say(found.length ? "maps.check.read" : "maps.check.noMaps", { title: item.title, build: String(data?.build) });
      await load();
    } catch (err) {
      say(String(err), {}, true);
    } finally {
      checking = false;
    }
  }

  /** Preview every map in a collection: maps PZMM knows at once, Map-tagged members it doesn't read from Steam (one at a time). */
  async function checkCollection(title: string, members: WorkshopItem[]) {
    const ids: string[] = [];
    const failed: string[] = [];
    let unread = 0;
    for (const [i, it] of members.entries()) {
      const known = all.filter((m) => itemOf(m.source) === it.id);
      if (known.length) ids.push(...known.map((m) => m.id));
      else if (!it.have && it.tags.includes("Map")) {
        say("maps.check.readingMember", { title: it.title, n: i + 1, total: members.length });
        try {
          ids.push(...(await api.mapCheck(it.id, false)).map((m) => m.id));
        } catch {
          failed.push(it.title);
        }
      } else if (!it.have) unread++;
    }
    checkText = "";
    await load();
    setPreview(new Set([...preview, ...ids.filter((id) => !isOn(id))]));
    // up to three sentences, logged as one line
    say(
      [
        t("maps.check.collection", { title, count: ids.length, items: members.length }),
        failed.length && t("maps.check.collectionFailed", { titles: failed.join(", ") }),
        unread && t("maps.check.collectionUntagged", { count: unread }),
      ]
        .filter(Boolean)
        .join(" "),
      {},
      failed.length > 0,
    );
  }

  // --- the grid: a canvas (thousands of cells), coloured from the theme's tokens ---
  const HEIGHT = 460;
  let canvas = $state<HTMLCanvasElement>();
  let probe = $state<HTMLSpanElement>();
  let width = $state(0);
  /** Pixels per cell and the offset of cell 0,0. */
  let view = $state({ scale: 0, x: 0, y: 0 });
  let hover = $state<{ x: number; y: number; px: number; py: number } | null>(null);
  let theme = $state(0);
  let hideVanilla = $state(false);
  let colours = $state(false);
  let help = $state(false);

  /** Per-map colours: hues a golden angle apart, in the order maps are shown. */
  const hueOf = $derived(new Map(shown.map((m, i) => [m.id, 70 + ((i * 137.508) % 260)])));
  const swatch = (id: string) => `oklch(0.68 0.13 ${hueOf.get(id) ?? 0})`;

  const bounds = $derived.by(() => {
    if (!grid.length) return null;
    const xs = grid.map((c) => c.x);
    const ys = grid.map((c) => c.y);
    return { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) };
  });

  function fit() {
    if (!bounds || !width) return;
    const [cols, rows] = [bounds.x1 - bounds.x0 + 1, bounds.y1 - bounds.y0 + 1];
    const scale = Math.min(width / (cols + 2), HEIGHT / (rows + 2));
    view = { scale, x: (width - cols * scale) / 2 - bounds.x0 * scale, y: (HEIGHT - rows * scale) / 2 - bounds.y0 * scale };
  }
  let fitted = false;
  $effect(() => {
    if (!fitted && bounds && width) {
      fitted = true;
      untrack(fit);
    }
  });

  function zoom(k: number, px = width / 2, py = HEIGHT / 2) {
    const scale = Math.min(64, Math.max(1, view.scale * k));
    view = { scale, x: px - ((px - view.x) * scale) / view.scale, y: py - ((py - view.y) * scale) / view.scale };
  }

  $effect(() => {
    void theme;
    if (!canvas || !probe || !width || !view.scale) return;
    const dpr = devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = HEIGHT * dpr;
    const g = canvas.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    // resolved through an element, so color-mix() and relative colours come back as rgb()
    const color = (token: string) => {
      probe!.style.color = token.startsWith("--") ? `var(${token})` : token;
      return getComputedStyle(probe!).color;
    };
    const fill = { vanilla: color("--muted"), water: color("--map-water"), map: color("--map-cell"), over: color("--map-over"), conflict: color("--danger") };
    const hues = new Map([...hueOf.keys()].map((id) => [id, color(swatch(id))]));
    g.fillStyle = color("--bg");
    g.fillRect(0, 0, width, HEIGHT);
    const s = view.scale;
    const gap = s >= 10 ? 1 : 0;
    for (const c of grid) {
      const base = c.kind === "vanilla" || c.kind === "water";
      if (base && hideVanilla) continue;
      g.globalAlpha = base ? 0.3 : 0.9;
      g.fillStyle = colours && c.maps.length === 1 ? hues.get(c.maps[0])! : fill[c.kind];
      g.fillRect(view.x + c.x * s, view.y + c.y * s, s - gap, s - gap);
    }
    g.globalAlpha = 1;
    g.lineWidth = 2;
    const ring = (cell: Cell | null, token: string) => {
      if (!cell) return;
      g.strokeStyle = color(token);
      g.strokeRect(view.x + cell[0] * s - 1, view.y + cell[1] * s - 1, s + 1, s + 1);
    };
    pickedCells.forEach((c) => ring(c, "--accent-text"));
    ring(hover && [hover.x, hover.y], "--text");
    // axis labels: cell numbers along the top and left edges, at least 32px apart
    const step = [1, 2, 5, 10, 20, 50, 100].find((n) => n * s >= 32) ?? 200;
    g.fillStyle = color("--panel");
    g.globalAlpha = 0.85;
    g.fillRect(0, 0, width, 16);
    g.fillRect(0, 0, 28, HEIGHT);
    g.globalAlpha = 1;
    g.fillStyle = color("--muted");
    g.font = "11px 'IBM Plex Mono', monospace";
    g.textAlign = "center";
    g.textBaseline = "middle";
    for (let x = Math.ceil(-view.x / s / step) * step; view.x + x * s < width; x += step) if (view.x + (x + 0.5) * s > 28) g.fillText(String(x), view.x + (x + 0.5) * s, 8);
    g.textAlign = "right";
    for (let y = Math.ceil(-view.y / s / step) * step; view.y + y * s < HEIGHT; y += step) if (view.y + (y + 0.5) * s > 16) g.fillText(String(y), 25, view.y + (y + 0.5) * s);
  });

  onMount(() => {
    // redraw when the theme or accent changes
    const bump = () => theme++;
    const mo = new MutationObserver(bump);
    mo.observe(document.documentElement, { attributes: true });
    const mq = matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", bump);
    return () => {
      mo.disconnect();
      mq.removeEventListener("change", bump);
    };
  });

  let drag: { x: number; y: number; vx: number; vy: number; moved: boolean } | null = null;
  const at = (e: MouseEvent) => {
    const r = canvas!.getBoundingClientRect();
    const [px, py] = [e.clientX - r.left, e.clientY - r.top];
    return { x: Math.floor((px - view.x) / view.scale), y: Math.floor((py - view.y) / view.scale), px, py };
  };
  function down(e: PointerEvent) {
    drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false };
    canvas!.setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (drag) {
      const [dx, dy] = [e.clientX - drag.x, e.clientY - drag.y];
      if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
      if (drag.moved) {
        view = { ...view, x: drag.vx + dx, y: drag.vy + dy };
        hover = null;
        return;
      }
    }
    hover = at(e);
  }
  function up(e: PointerEvent) {
    if (drag && !drag.moved) {
      const { x, y } = at(e);
      const has = pickedCells.some((c) => c[0] === x && c[1] === y);
      if (e.shiftKey) pickedCells = has ? pickedCells.filter((c) => c[0] !== x || c[1] !== y) : [...pickedCells, [x, y]];
      else pickedCells = has && pickedCells.length === 1 ? [] : [[x, y]];
    }
    drag = null;
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const { px, py } = at(e);
    zoom(e.deltaY < 0 ? 1.25 : 0.8, px, py);
  }
  const hovered = $derived(hover ? byCell.get(`${hover.x},${hover.y}`) : undefined);
  const KIND = { vanilla: "maps.kind.vanilla", water: "maps.kind.water", map: "", over: "maps.kind.over", conflict: "maps.kind.conflict" };
</script>

<svelte:window onkeydown={undoKey} />

<section class="maps blueprint" aria-label={t("maps.label")}>
  <div class="inner">
    <div class="card world">
      <div class="head">
        <h2>{t("maps.worldMap")}</h2>
        <button class="quiet" onclick={() => (help = !help)} aria-expanded={help}><Icon name="info" size={14} />{t("maps.help.button")}</button>
      </div>
      {#if help}
        <div class="help" role="dialog" aria-label={t("maps.help.label")}>
          <h3>{t("maps.help.gridTitle")}</h3>
          <p>{@html th("maps.help.grid", { build: data?.build ?? "" })}</p>
          <h3>{t("maps.help.previewTitle")}</h3>
          <p>{@html th("maps.help.preview")}</p>
          <h3>{t("maps.help.workshopTitle")}</h3>
          <p>{t("maps.help.workshop")}</p>
          <button class="quiet" onclick={() => (help = false)}><Icon name="x" size={14} />{t("common.close")}</button>
        </div>
      {/if}
      <p class="lede">{t("maps.lede")}</p>
      <div class="legend">
        {#if colours}
          {#each drawn.slice(0, 12) as m (m.id)}<span><i style:background={swatch(m.id)}></i>{nameOf(m)}</span>{/each}
          {#if drawn.length > 12}<span>{t("maps.legend.more", { count: drawn.length - 12 })}</span>{/if}
        {:else}
          <span><i style="background: var(--map-cell)"></i>{t("maps.legend.map")}</span>
          <span><i style="background: var(--map-over)"></i>{t("maps.legend.over", { count: overCells })}</span>
        {/if}
        <span><i style="background: var(--danger)"></i>{t("maps.legend.conflict", { count: conflictCells })}</span>
        {#if !hideVanilla}
          <span><i style="background: var(--muted); opacity: 0.45"></i>{t("maps.legend.vanilla")}</span>
          {#if data?.water.length}<span><i style="background: var(--map-water); opacity: 0.6"></i>{t("maps.legend.river")}</span>{/if}
        {/if}
      </div>
      <div class="row layers">
        <label class="check"><input type="checkbox" bind:checked={hideVanilla} />{t("maps.hideVanilla")}</label>
        <label class="check"><input type="checkbox" bind:checked={colours} />{t("maps.perMapColours")}</label>
        <span class="zoom">
          <button class="quiet" onclick={() => zoom(1.25)} aria-label={t("maps.zoomIn")}><Icon name="plus" size={14} />{t("maps.zoomIn")}</button>
          <button class="quiet" onclick={() => zoom(0.8)}>{t("maps.zoomOut")}</button>
          <button class="quiet" onclick={fit}>{t("maps.fit")}</button>
        </span>
      </div>
      <div class="canvas" bind:clientWidth={width}>
        <canvas bind:this={canvas} style:height="{HEIGHT}px"
          aria-label={t("maps.gridLabel", { maps: shown.length, conflicts: conflictCells, over: overCells })}
          onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={() => (hover = null)} onwheel={wheel}></canvas>
        <span bind:this={probe} hidden></span>
        {#if hover}
          <div class="tip" style:left="{Math.min(hover.px + 14, width - 230)}px" style:top="{Math.min(hover.py + 14, HEIGHT - 140)}px">
            <strong>{t("maps.cell", { x: hover.x, y: hover.y })}</strong>
            {#if hovered}
              {#if KIND[hovered.kind]}<span class="kind {hovered.kind}">{t(KIND[hovered.kind])}</span>{/if}
              {#each hovered.maps as id}<span>{names.get(id) ?? id}</span>{/each}
            {:else}
              <span class="hint">{t("maps.empty")}</span>
            {/if}
          </div>
        {/if}
        {#if !data}<p class="loading">{t("maps.readingFiles")}</p>{/if}
      </div>
      {#if data && !data.vanilla}
        <p class="hint">{t("maps.noVanilla", { build: data.build })}</p>
      {/if}
    </div>

    <div class="card list">
      <div class="head">
        <h2>{t("maps.mapMods")}</h2>
        <button class="quiet icon" onclick={undo} disabled={busy || !(history.length || canUndo)} title={t("maps.undoTip")} aria-label={t("maps.undoLabel")}><Icon name="undo" size={16} /></button>
      </div>
      <div class="source">
        <label class="check"><input type="checkbox" bind:checked={prefs.mapsWorkshop} />{t("maps.workshopSource")}</label>
        <span class="hint note">{t("maps.workshopSourceNote")}</span>
        {#if prefs.mapsWorkshop}
          <span class="hint" class:bad={!!scan.error} role="status">{scanText}</span>
          {#if scan.running}
            {#if scan.p?.total}<progress max={scan.p.total} value={scan.p.done}></progress>{/if}
            <button class="quiet" onclick={stopScan} disabled={scan.stopping}><Icon name="x" size={14} />{scan.stopping ? t("maps.scan.stopping") : t("maps.scan.stop")}</button>
          {:else}
            <button class="quiet" onclick={startScan} title={t("maps.scan.refreshTip")}><Icon name="refresh" size={14} />{t("common.refresh")}</button>
          {/if}
        {/if}
      </div>
      <div class="row">
        <input type="search" bind:value={search} placeholder={t("maps.searchPlaceholder")} aria-label={t("maps.searchLabel")} />
        <select bind:value={filter} aria-label={t("maps.filterLabel")} title={t("maps.filterLabel")}>
          {#each FILTERS as [v, label]}<option value={v}>{t(label)}</option>{/each}
        </select>
        <select bind:value={sortBy} aria-label={t("maps.sortLabel")} title={t("maps.sortLabel")}>
          <option value="name">{t("maps.sort.name")}</option>
          <option value="cells">{t("maps.sort.cells")}</option>
          <option value="conflicts">{t("maps.sort.conflicts")}</option>
          <option value="state">{t("maps.sort.state")}</option>
          <option value="subs">{t("maps.sort.subs")}</option>
          <option value="updated">{t("maps.sort.updated")}</option>
          <option value="newest">{t("maps.sort.newest")}</option>
        </select>
      </div>
      {#if pickedCells.length}
        <p class="filter">
          <span class="chip">{@html th("maps.pickedCells", { cells: pickedCells.map(([x, y]) => `${x},${y}`).join(" · ") })}</span>
          {t("maps.mapCount", { count: listed.length })}
          <button class="quiet" onclick={() => (pickedCells = [])}><Icon name="x" size={14} />{t("maps.clearCells")}</button>
        </p>
      {/if}
      <div class="row bulk">
        <button onclick={quickSelect} disabled={!listed.length || picking} aria-busy={picking} title={t("maps.quickSelectTip")}><Icon name="wand" size={14} />{picking ? t("maps.quickSelecting") : t("maps.quickSelect")}</button>
        <button onclick={previewListed} disabled={!listed.length}>{t("maps.previewListed")}</button>
        {#if preview.size}
          <button onclick={clearListed}>{t("maps.unpreviewListed")}</button>
          <button onclick={() => setPreview(new Set())} title={t("maps.enabledOnlyTip")}>{t("maps.enabledOnly")}</button>
        {/if}
        {#if previewOff.length}<button onclick={enablePreviewed} disabled={busy}><Icon name="check" size={14} />{t("maps.enablePreviewed", { count: previewOff.length })}</button>{/if}
        {#if listedOn.length}<button onclick={() => setMods(listedOn, false)} disabled={busy}><Icon name="x" size={14} />{t("maps.disableListed", { count: listedOn.length })}</button>{/if}
        {#if hidden.size}<button onclick={() => (hidden = new Set())} title={t("maps.unhideTip")}>{t("maps.unhide", { count: hidden.size })}</button>{/if}
      </div>
      {#if data && !all.length}
        <p class="hint">{t("maps.noMaps", { build: data.build })}</p>
      {/if}
      <ul class="mods">
        {#each listed.slice(0, limit) as m (m.id)}
          {@const installed = mods.has(m.id)}
          {@const on = isOn(m.id)}
          {@const item = itemOf(m.source)}
          {@const meta = metaOf(m)}
          {@const clash = clashes.get(m.id)}
          {@const isShown = on || preview.has(m.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <li class:shown={isShown} class:hidden={hidden.has(m.id)} onclick={(e) => !on && !(e.target as HTMLElement).closest("button, input, a") && togglePreview(m.id)}>
            <input type="checkbox" checked={isShown} disabled={on} onchange={() => togglePreview(m.id)}
              aria-label={t("maps.previewLabel", { name: nameOf(m) })} title={on ? t("maps.alwaysShownTip") : t("maps.previewTip")} />
            <span class="name">
              <span><strong>{nameOf(m)}</strong>{#if on}<span class="on">{t("common.enabled")}</span>{:else if !installed}<span class="off">{t("maps.notInstalled")}</span>{/if}{#if isShown}<button class="link hide" onclick={() => toggleHidden(m.id)} title={hidden.has(m.id) ? t("maps.showOnGridTip") : t("maps.hideOnGridTip")}>{hidden.has(m.id) ? t("maps.showOnGrid") : t("maps.hideOnGrid")}</button>{/if}</span>
              <span class="hint">{t("maps.cellCount", { count: cellCount(m) })} · {m.maps.map((f) => f.folder).join("; ")}{meta?.author ? ` · ${t("maps.byAuthor", { author: meta.author })}` : ""}{meta?.subscriptions ? ` · ${t("maps.subscribers", { count: meta.subscriptions, n: compact.format(meta.subscriptions) })}` : ""}{!installed && meta?.size ? ` · ${fmtSize(meta.size)}` : ""}</span>
              {#each [...(clash?.with ?? [])].slice(0, 3) as [rival, cells] (rival)}
                <span class="bad">{@html th("maps.conflictsWith", { count: cells.length, name: names.get(rival) ?? rival, cells: cellList(cells) })}
                  <button class="link" onclick={() => swap(m)} disabled={busy} title={t("maps.swapTip")}>{t("maps.swap")}</button></span>
              {/each}
              {#if (clash?.with.size ?? 0) > 3}<span class="bad">{t("maps.andMore", { count: clash!.with.size - 3 })}</span>{/if}
              {#if clash?.over}<span class="over">{t("maps.cellsOverVanilla", { count: clash.over })}</span>{/if}
              {#if packSize(m) > 1}<span class="hint">{t("maps.packMember", { count: packSize(m) })}</span>{/if}
              {#each otherItems(m).slice(0, 1) as it (it)}
                <span class="dupe">{t(otherItems(m).length > 1 ? "maps.dupeModMore" : "maps.dupeMod", { id: m.id, item: data?.items[it]?.title ?? t("maps.workshopItem", { id: it }), count: otherItems(m).length - 1 })}</span>
              {/each}
              {#each m.maps.filter((f) => (folderOwners.get(f.folder)?.length ?? 0) > 1) as f (f.folder)}
                {@const others = folderOwners.get(f.folder)!.filter((id) => id !== m.id)}
                <span class="dupe">{t(others.length > 2 ? "maps.dupeFolderMore" : "maps.dupeFolder", { folder: f.folder, names: others.slice(0, 2).map((id) => names.get(id) ?? id).join(", "), count: others.length - 2 })}</span>
              {/each}
            </span>
            <span class="btns">
              {#if item}<button class="quiet" onclick={() => api.openUrl(workshopUrl(item))} title={t("maps.openWorkshopTip")}><Icon name="steam" size={14} />{t("common.workshop")}</button>{/if}
              {#if installed}
                <button disabled={busy} onclick={() => setMods([m.id], !on)} title={on ? "" : t("maps.alsoTurnsOnTip")}>{on ? t("common.disable") : t("common.enable")}</button>
              {:else if item}
                <button disabled={busy} onclick={() => onInstallWorkshop(item)} title={t("maps.installTip")}><Icon name="download" size={14} />{t("common.install")}</button>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
      {#if listed.length > limit}
        <button class="quiet more" onclick={() => (limit += 150)}>{t("maps.showMore", { count: Math.min(150, listed.length - limit), total: listed.length })}</button>
      {/if}
    </div>

    <div class="card">
      <div class="head">
        <h2>{t("maps.req.title")}</h2>
        {#if needs.length}
          <div class="seg" role="radiogroup" aria-label={t("maps.req.viewLabel")}>
            <button type="button" role="radio" aria-checked={reqView === "list"} class:active={reqView === "list"} onclick={() => (reqView = "list")} title={t("maps.req.neededByTip")}>{t("maps.req.neededBy")}</button>
            <button type="button" role="radio" aria-checked={reqView === "tree"} class:active={reqView === "tree"} onclick={() => (reqView = "tree")} title={t("maps.req.treeTip")}>{t("maps.req.tree")}</button>
          </div>
        {/if}
      </div>
      <p class="lede">{@html th("maps.req.lede")}</p>
      {#if !needs.length}
        <p class="hint">{shown.length ? t("maps.req.nothingElse") : t("maps.req.showOne")}</p>
      {/if}
      {#snippet needState(n: Need)}
        {@const m = n.id ? mods.get(n.id) : undefined}
        <span class="state {m ? (m.on ? 'is-on' : 'is-off') : 'is-missing'}">{m ? (m.on ? t("maps.req.on") : t("maps.req.off")) : t("maps.notInstalled")}</span>
      {/snippet}
      {#snippet needButtons(n: Need)}
        {@const m = n.id ? mods.get(n.id) : undefined}
        {@const item = needItem(n)}
        <span class="btns">
          {#if item}<button class="quiet" onclick={() => api.openUrl(workshopUrl(item))} title={t("maps.openWorkshopTip")}><Icon name="steam" size={14} />{t("common.workshop")}</button>{/if}
          {#if m && !m.on}
            <button disabled={busy} onclick={() => setMods([n.id], true)}>{t("common.enable")}</button>
          {:else if !m && n.id}
            <button disabled={busy} onclick={() => installNeed(n)} title={t("maps.req.findTip")}><Icon name="download" size={14} />{t("common.install")}</button>
            {#if finding[n.id]?.[0] === "maps.req.noClearResult" && n.by.some((b) => mods.has(b))}
              <button class="quiet" onclick={() => onShowMod(n.by.find((b) => mods.has(b))!)}>{t("maps.req.pickInDetails")}</button>
            {/if}
          {/if}
        </span>
      {/snippet}
      {#if reqView === "list"}
        {#each GROUPS as [g, title] (g)}
          {@const group = needs.filter((n) => n.group === g)}
          {#if group.length}
            <h3>{t(title)}</h3>
            <ul class="needs">
              {#each group as n (n.key)}
                <li>
                  <span class="name">
                    <span class="mono">{n.id || n.folder}</span>
                    {@render needState(n)}
                    <span class="hint">{t(n.folder && n.id ? "maps.req.neededByMap" : n.folder ? "maps.req.neededByFolder" : "maps.req.neededByOnly", { folder: n.folder, names: n.by.map((id) => names.get(id) ?? id).join(", ") })}</span>
                    {#if finding[n.id]}<span class="hint">{t(...finding[n.id])}</span>{/if}
                  </span>
                  {@render needButtons(n)}
                </li>
              {/each}
            </ul>
          {/if}
        {/each}
      {:else}
        {#snippet branch(root: MapMod, id: string, depth: number, path: string[])}
          {@const n = needs.find((x) => x.key === id) ?? { key: id, id, folder: "", by: [root.id], group: "missing" as Group }}
          <li style:padding-left="{8 + depth * 18}px">
            <span class="name">
              <span><span class="mono">{id}</span> {@render needState(n)}{#if path.includes(id)}<span class="hint"> {t("maps.req.loop")}</span>{/if}</span>
              {#if finding[id]}<span class="hint">{t(...finding[id])}</span>{/if}
            </span>
            {@render needButtons(n)}
          </li>
          {#if !path.includes(id)}
            {#each requiresOf(id) as r (r)}{@render branch(root, r, depth + 1, [...path, id])}{/each}
          {/if}
        {/snippet}
        {#each shown.filter((m) => m.requires.length || m.maps.some((f) => f.lots.some((l) => !data?.vanilla_folders.includes(l)))) as m (m.id)}
          <h3>{nameOf(m)}</h3>
          <ul class="needs tree">
            {#each m.requires as r (r)}{@render branch(m, r, 0, [m.id])}{/each}
            {#each needs.filter((n) => n.folder && n.by.includes(m.id)) as n (n.key)}
              <li><span class="name"><span><span class="mono">{n.id || n.folder}</span> {@render needState(n)} <span class="hint">{t("maps.req.map", { folder: n.folder })}</span></span></span>{@render needButtons(n)}</li>
            {/each}
          </ul>
        {/each}
      {/if}
    </div>

    <div class="card">
      <h2>{t("maps.export.title")}</h2>
      <p class="lede">{@html th("maps.export.lede")}</p>
      {#if mapLine.length}
        <h3>{t("maps.export.orderTitle")}</h3>
        <p class="hint">{@html th("maps.export.orderHint")}</p>
        <MapOrder bind:maps={mapLine} />
      {/if}
      <div class="row">
        <button class="primary" onclick={exportMaps} disabled={!shown.length}><Icon name="copy" size={14} />{t("maps.export.copy", { count: shown.length })}</button>
      </div>
      <h3>{t("maps.presets.title")}</h3>
      <p class="hint">{t("maps.presets.hint")}</p>
      <div class="row">
        {#if Object.keys(presets).length}
          <select bind:value={presetPick} aria-label={t("maps.presets.label")}>
            <option value="">{t("maps.presets.pick")}</option>
            {#each Object.entries(presets) as [name, ids] (name)}<option value={name}>{name} ({ids.length})</option>{/each}
          </select>
          <button onclick={loadPreset} disabled={!presetPick}>{t("maps.presets.preview")}</button>
          <button class="danger" onclick={deletePreset} disabled={!presetPick}><Icon name="trash" size={14} />{t("common.delete")}</button>
        {/if}
      </div>
      <form class="row" onsubmit={savePreset}>
        <input type="text" bind:value={presetName} placeholder={t("maps.presets.namePlaceholder")} aria-label={t("maps.presets.namePlaceholder")} />
        <button type="submit" disabled={!presetName.trim() || !shown.length}><Icon name="plus" size={14} />{t("maps.presets.save", { count: shown.length })}</button>
      </form>
    </div>

    <div class="card">
      <h2>{t("maps.compare.title")}</h2>
      <p class="lede">{@html th("maps.compare.lede")}</p>
      {#if servers.length}
        <div class="row">
          <select bind:value={server} aria-label={t("maps.compare.serverLabel")}>
            <!-- i18n-ignore: file names -->
            {#each servers as s (s)}<option value={s}>{s}.ini</option>{/each}
          </select>
          <button onclick={compareServer} disabled={!server}><Icon name="server" size={14} />{t("maps.compare.compare")}</button>
        </div>
      {:else}
        <p class="hint">{t("maps.compare.noServers")}</p>
      {/if}
      {#if compared}
        {#if !compared.only_server.length && !compared.only_you.length}
          <p class="hint">{t("maps.compare.same", { server })}</p>
        {/if}
        {#if compared.only_server.length}
          <h3>{t("maps.compare.onlyServer")}</h3>
          <ul class="needs">
            {#each compared.only_server as s (s.folder)}
              {@const m = byId(s.id)}
              {@const row = s.id ? mods.get(s.id) : undefined}
              <li>
                <span class="name"><span class="mono">{s.folder}</span>
                  <span class="hint">{m ? nameOf(m) : s.id ? s.id : t("maps.compare.noKnownMap")}{row ? ` · ${row.on ? t("maps.compare.on") : t("maps.compare.installed")}` : s.item ? ` · ${t("maps.compare.notInstalled")}` : ""}</span></span>
                <span class="btns">
                  {#if s.id && !row?.on && byId(s.id)}<button class="quiet" onclick={() => s.id && !preview.has(s.id) && togglePreview(s.id)}>{t("maps.compare.preview")}</button>{/if}
                  {#if row && !row.on}
                    <button disabled={busy} onclick={() => setMods([s.id!], true)}>{t("common.enable")}</button>
                  {:else if !row && s.item}
                    <button disabled={busy} onclick={() => onInstallWorkshop(s.item!)}><Icon name="download" size={14} />{t("common.install")}</button>
                  {/if}
                </span>
              </li>
            {/each}
          </ul>
        {/if}
        {#if compared.only_you.length}
          <h3>{t("maps.compare.onlyYou")}</h3>
          <p class="mono hint">{compared.only_you.join("; ")}</p>
        {/if}
      {/if}
    </div>

    <div class="card wide">
      <h2>{t("maps.check.title")}</h2>
      <p class="lede">{t("maps.check.lede")}</p>
      <form class="row" onsubmit={check}>
        <input type="text" bind:value={checkText} placeholder={t("maps.check.placeholder")} spellcheck="false" aria-label={t("maps.check.placeholder")} />
        <button type="submit" disabled={checking || !checkText.trim()}><Icon name="search" size={14} />{checking ? t("maps.check.checking") : t("maps.check.check")}</button>
      </form>
    </div>
  </div>
</section>

<style>
  .maps {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    max-width: 1280px;
    margin: 0 auto;
    padding: 24px 16px 40px;
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(320px, 2fr);
    align-items: start;
    gap: 16px;
  }
  @media (max-width: 960px) {
    .inner {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .wide {
    grid-column: 1 / -1;
  }
  .card {
    position: relative;
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
    padding: 20px 24px 22px;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
  }
  h3 {
    margin: 20px 0 4px;
    font-size: 15px;
  }
  .hint {
    display: block;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 8px;
  }
  .row input[type="text"],
  .row input[type="search"] {
    flex: 1 1 220px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .help {
    position: absolute;
    z-index: 2;
    top: 56px;
    right: 16px;
    width: min(460px, calc(100% - 32px));
    padding: 4px 18px 14px;
    background: var(--raised);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }
  .help h3 {
    margin-top: 12px;
  }
  .help p {
    margin: 0 0 4px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
    margin-bottom: 4px;
    color: var(--muted);
  }
  .legend i {
    display: inline-block;
    width: 12px;
    height: 12px;
    border-radius: 2px;
    margin-right: 6px;
    vertical-align: -1px;
  }
  .layers {
    margin: 0 0 8px;
    color: var(--muted);
  }
  .zoom {
    margin-left: auto;
    display: flex;
    gap: 2px;
  }
  .canvas {
    position: relative;
    border-radius: 6px;
    overflow: hidden;
  }
  canvas {
    display: block;
    width: 100%;
    cursor: crosshair;
    touch-action: none;
  }
  .loading {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    color: var(--muted);
  }
  .tip {
    position: absolute;
    z-index: 1;
    width: 216px;
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    background: var(--raised);
    border-radius: 6px;
    box-shadow: var(--shadow);
    pointer-events: none;
    overflow-wrap: anywhere;
  }
  .kind {
    font-size: 12px;
    color: var(--muted);
  }
  .kind.conflict {
    color: var(--danger);
  }
  .kind.over {
    color: var(--map-over);
  }
  .source {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 10px;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }
  .source .hint {
    flex: 1 1 200px;
  }
  .source progress {
    width: 120px;
    accent-color: var(--accent);
  }
  .bulk button {
    padding: 4px 8px;
  }
  .filter {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 10px 0 0;
  }
  .chip {
    padding: 2px 8px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--on-selected);
  }
  .mods,
  .needs {
    list-style: none;
    padding: 0;
    margin: 10px 0 0;
    border-top: 1px solid var(--line);
  }
  .mods {
    max-height: 620px;
    overflow-y: auto;
  }
  .mods li,
  .needs li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    margin: 0 -8px;
    border-radius: 6px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .mods li {
    cursor: pointer;
  }
  .mods li.shown {
    box-shadow: inset 3px 0 0 var(--accent-text);
  }
  .mods li:hover,
  .needs li:hover {
    background: color-mix(in oklab, var(--accent) 16%, var(--panel));
  }
  .more {
    margin-top: 6px;
  }
  .name {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 1px;
    overflow-wrap: anywhere;
  }
  .needs .name {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 8px;
  }
  .btns {
    display: flex;
    gap: 4px;
  }
  .on {
    margin-left: 8px;
    font-size: 12px;
    font-weight: 700;
    color: var(--accent-text);
  }
  .off {
    margin-left: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .state {
    font-size: 12px;
    font-weight: 700;
  }
  .is-on {
    color: var(--accent-text);
  }
  .is-off {
    color: var(--muted);
  }
  .is-missing {
    color: var(--danger);
  }
  .bad {
    color: var(--danger);
    font-size: 12px;
  }
  .bad .link {
    margin-left: 6px;
    font-size: 12px;
    color: var(--text);
  }
  .over,
  .dupe {
    color: var(--map-over);
    font-size: 12px;
  }
  .mods li.hidden strong {
    opacity: 0.6;
  }
  .link.hide {
    margin-left: 10px;
    font-size: 12px;
    color: var(--muted);
  }
  .source .note {
    flex-basis: 100%;
    order: 1;
  }
  .tree li {
    border-bottom-style: dashed;
  }
</style>
