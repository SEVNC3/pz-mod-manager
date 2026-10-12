<script lang="ts" module>
  import { listen } from "@tauri-apps/api/event";
  import type { MapScan } from "./api";

  // the Workshop scan outlives the tab (it runs in the backend), so its state lives here
  // `stopping`: Stop was clicked
  const scan = $state({ running: false, p: null as MapScan | null, error: "", ticks: 0, stopping: false });
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
  import { untrack } from "svelte";
  import { MediaQuery } from "svelte/reactivity";
  import { api, type Cell, type GridCell, type MapBackground, type MapCompare, type MapMod, type MapsView, type ModRow, type WorkshopItem } from "./api";
  import Icon from "./Icon.svelte";
  import MapGrid, { backgrounds, loadImage, readBackground } from "./MapGrid.svelte";
  import MapOrder from "./MapOrder.svelte";
  import { compact, fmtAgo, fmtNum, fmtSize, workshopUrl } from "./format";
  import { intlLang, keyedError, lang, t, th, type Vars } from "./i18n.svelte";
  import { prefs } from "./prefs.svelte";
  import { arrowPick, isTyping, trap } from "./ui";
  import { confirmed } from "./ask.svelte";
  import { solveBackground } from "./order";

  let { busy, profile, mods, loadOrder, say, onSetMods, onInstallWorkshop, onShowMod, canUndo, onUndo, find = "" }: {
    busy: boolean;
    /** The active profile: an arranged Map= order is saved to it, never to one switched to since. */
    profile: string;
    /** The library by mod id: what's installed and on. */
    mods: Map<string, ModRow>;
    /** The enabled mods' ids in load order. */
    loadOrder: string[];
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
    /** Start with this search (Show in Maps from a mod's menu). */
    find?: string;
  } = $props();

  /** A backend error in the player's language: Maps failures come as `maps.err.<key>`, then English detail after ": ". */
  function errParts(e: unknown) {
    const s = String(e).replace(/^Error:\s*/, "");
    const m = s.match(/^(maps\.err\.\w+)(?::\s*([\s\S]*))?$/);
    return m ? { text: t(m[1]), detail: m[2] ?? "" } : { text: s, detail: "" };
  }
  const errText = (e: unknown) => {
    const { text, detail } = errParts(e);
    return detail ? `${text} (${detail})` : text;
  };

  let data = $state.raw<MapsView | null>(null);
  /** Maps shown on the grid without being enabled. */
  let preview = $state<Set<string>>(new Set());
  /** Clicked cells (shift-click adds): the list shows only the maps covering one of them. */
  let pickedCells = $state<Cell[]>([]);

  // only the latest answer counts: an older, slower one never replaces it
  let viewFor = 0;
  const load = () => {
    const ask = ++viewFor;
    return api.mapView().then((v) => ask === viewFor && (data = v)).catch((e) => say(errText(e), {}, true));
  };
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
  /** Shown maps hidden from the grid (a hidden map no longer shown doesn't count). */
  const hiddenCount = $derived(shown.length - drawn.length);
  const cellCount = (m: MapMod) => m.maps.reduce((n, f) => n + f.cells.length, 0);
  const key = ([x, y]: Cell) => `${x},${y}`;

  let grid = $state.raw<GridCell[]>([]);
  /** The grid has been worked out at least once (an empty one then means nothing to draw). */
  let gridDone = $state(false);
  let gridFor = 0;
  $effect(() => {
    const ask = ++gridFor;
    api.mapGrid(data?.vanilla ?? [], data?.water ?? [], drawn).then((g) => ask === gridFor && ((grid = g), (gridDone = !!data))).catch((e) => say(errText(e), {}, true));
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
    if (isTyping(e) || busy || !(e.ctrlKey || e.metaKey) || e.shiftKey || e.key.toLowerCase() !== "z") return;
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
  const collate = $derived(new Intl.Collator(lang()).compare);
  /** Names as a list in the app's language ("a, b and c"). */
  const listOf = (xs: string[], type: Intl.ListFormatType = "conjunction") => new Intl.ListFormat(intlLang(), { type }).format(xs);
  // svelte-ignore state_referenced_locally
  let search = $state(find);
  type Filter = "all" | "on" | "installed" | "missing" | "conflict" | "over";
  let filter = $state<Filter>("all");
  /** Leave out mods with no cells (spawn points for a vanilla town). */
  let hideSpawn = $state(true);
  type Sort = "order" | "name" | "cells" | "conflicts" | "state" | "subs" | "updated" | "newest";
  let sortBy = $state<Sort>("order");
  const rank = $derived(new Map(loadOrder.map((id, i) => [id, i])));
  const orderRank = (m: MapMod) => (isOn(m.id) ? (rank.get(m.id) ?? loadOrder.length) : Infinity);
  const FILTERS: [Filter, string][] = [["all", "maps.filter.all"], ["on", "common.enabled"], ["installed", "common.installed"], ["missing", "maps.notInstalled"], ["conflict", "maps.filter.conflict"], ["over", "maps.filter.over"]];
  const stateRank = (m: MapMod) => (isOn(m.id) ? 0 : mods.has(m.id) ? 1 : 2);
  const listed = $derived.by(() => {
    const words = search.toLowerCase().split(/\s+/).filter(Boolean);
    const out = all.filter((m) => {
      const c = clashes.get(m.id);
      const pass = { all: true, on: isOn(m.id), installed: mods.has(m.id), missing: !mods.has(m.id), conflict: !!c?.with.size, over: !!c?.over }[filter];
      if (!pass || (hideSpawn && !cellCount(m))) return false;
      if (pickedCells.length && !pickedCells.some(([x, y]) => m.maps.some((f) => f.cells.some((c) => c[0] === x && c[1] === y)))) return false;
      if (!words.length) return true;
      const meta = metaOf(m);
      const hay = [nameOf(m), m.id, itemOf(m.source) ?? "", meta?.author ?? "", meta?.title ?? "", ...(meta?.tags ?? []), ...m.maps.map((f) => f.folder)].join("\n").toLowerCase();
      return words.every((w) => hay.includes(w));
    });
    const by: Record<Sort, (a: MapMod, b: MapMod) => number> = {
      // enabled maps in load order, then the rest by name
      order: (a, b) => (orderRank(a) === orderRank(b) ? 0 : orderRank(a) < orderRank(b) ? -1 : 1),
      name: () => 0,
      cells: (a, b) => cellCount(b) - cellCount(a),
      conflicts: (a, b) => (clashes.get(b.id)?.with.size ?? 0) - (clashes.get(a.id)?.with.size ?? 0),
      state: (a, b) => stateRank(a) - stateRank(b),
      subs: (a, b) => (metaOf(b)?.subscriptions ?? -1) - (metaOf(a)?.subscriptions ?? -1),
      updated: (a, b) => (metaOf(b)?.updated ?? -1) - (metaOf(a)?.updated ?? -1),
      newest: (a, b) => (metaOf(b)?.created ?? -1) - (metaOf(a)?.created ?? -1),
    };
    // spawn-point mods (no cells) after the maps, in every sort
    return out.sort((a, b) => +!cellCount(a) - +!cellCount(b) || by[sortBy](a, b) || collate(nameOf(a), nameOf(b)));
  });
  // thousands of Workshop maps: draw the list a page at a time
  let limit = $state(150);
  $effect(() => {
    [search, filter, hideSpawn, sortBy, pickedCells, prefs.mapsWorkshop];
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
      say(errText(e), {}, true);
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
    Object.assign(scan, { running: true, error: "", stopping: false });
    try {
      const p = await api.mapScan();
      scan.p = p;
      if (p.failed) say("maps.scan.failed", { count: p.failed }, true);
    } catch (e) {
      scan.error = t(...keyedError(e));
    } finally {
      scan.running = false; // the load effect picks up the last batch
    }
  }
  function stopScan() {
    scan.stopping = true;
    api.mapScanStop().catch(() => {});
  }
  const workshopCount = $derived(data ? Object.keys(data.items).length : 0);
  const scanText = $derived.by(() => {
    const p = scan.p;
    if (scan.error) return /429|limiting/i.test(scan.error) ? t("maps.scan.limited") : errText(scan.error);
    if (scan.running && scan.stopping) return t("maps.scan.stopping");
    if (!scan.running || !p) return t("maps.scan.summary", { n: fmtNum(data?.checked.length ?? 0), count: workshopCount });
    if (p.phase === "listing") return t("maps.scan.listing", { done: fmtNum(p.done), total: p.total ? fmtNum(p.total) : "?" });
    return t(p.failed ? "maps.scan.readingFailed" : "maps.scan.reading", { done: fmtNum(p.done), total: fmtNum(p.total), failed: fmtNum(p.failed) });
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
    return [...out.values()].sort((a, b) => collate(a.id || a.folder, b.id || b.folder));
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
      finding[n.id] = keyedError(e);
    }
  }

  // --- Map= order (kept with the profile once arranged) and export ---
  let mapLine = $state<string[]>([]);
  /** Each map folder's lots= (the maps it builds on), for Autosort. */
  const lotsByFolder = $derived(new Map(all.flatMap((m) => m.maps.map((f) => [f.folder, f.lots] as const))));
  const folders = $derived(shown.flatMap((m) => m.maps.map((f) => f.folder)));
  let lineFor = 0;
  $effect(() => {
    const f = folders;
    const ask = ++lineFor;
    api.mapLine(untrack(() => $state.snapshot(mapLine)), f).then((l) => ask === lineFor && (mapLine = l)).catch(() => {});
  });
  // saved (and given to the game) only when the player arranges, once they stop for a moment:
  // opening the tab changes nothing
  let saving: ReturnType<typeof setTimeout> | undefined;
  /** The profile was snapshotted before this visit's first arrangement (Snapshots can undo it). */
  let snapped = false;
  function arranged(line: string[]) {
    lineFor++; // an answer still on its way is older than this
    clearTimeout(saving);
    if (!snapped) {
      snapped = true;
      api.snapshotCreate("maps").catch(() => {});
    }
    const to = profile; // as arranged: the prop follows a switch made while the save waits
    saving = setTimeout(() => api.mapOrderSave(line, to).catch((e) => say(errText(e), {}, true)), 600);
  }

  async function exportMaps() {
    try {
      const labels = { ids: t("maps.export.text.ids"), required: t("maps.export.text.required"), missing: t("maps.export.text.missing") };
      await navigator.clipboard.writeText(await api.mapExport(shown, $state.snapshot(mapLine), labels));
      say("maps.export.copied", { count: shown.length });
    } catch (e) {
      say(errText(e), {}, true);
    }
  }

  // --- presets: named sets of shown maps ---
  let presets = $state<Record<string, string[]>>({});
  let presetName = $state("");
  let presetPick = $state("");
  async function savePreset(e: Event) {
    e.preventDefault();
    const name = presetName.trim();
    if (!name) return;
    if (presets[name] && !(await confirmed(t("maps.presets.confirmReplace", { name }), { label: t("maps.presets.replace"), icon: "check" }))) return;
    try {
      presets = await api.mapPresetSet(name, shown.map((m) => m.id));
      presetPick = name;
      presetName = "";
      say("maps.presets.saved", { count: shown.length, name });
    } catch (err) {
      say(errText(err), {}, true);
    }
  }
  function loadPreset() {
    const ids = presets[presetPick] ?? [];
    const known = new Set(all.map((m) => m.id));
    setPreview(new Set(ids.filter((id) => known.has(id) && !isOn(id))));
    const gone = ids.filter((id) => !known.has(id));
    say(gone.length ? "maps.presets.previewingMissing" : "maps.presets.previewing", { name: presetPick, ids: listOf(gone, "unit") }, gone.length > 0);
  }
  async function deletePreset() {
    if (!(await confirmed(t("maps.presets.confirmRemove", { name: presetPick }), { label: t("common.remove"), icon: "trash" }))) return;
    try {
      presets = await api.mapPresetSet(presetPick, null);
      presetPick = "";
    } catch (err) {
      say(errText(err), {}, true);
    }
  }

  // --- compare with a server's Map= ---
  let servers = $state<string[]>([]);
  let server = $state("");
  let compared = $state<MapCompare | null>(null);
  // presets and server configs: read on opening, and again once a busy spell (that may have failed them) ends
  $effect(() => {
    if (busy) return;
    api.mapPresets().then((p) => (presets = p)).catch(() => {});
    api.serversAndSaves().then((d) => ((servers = d.servers), (server = d.servers.includes(server) ? server : (d.servers[0] ?? "")))).catch(() => {});
  });
  async function compareServer() {
    try {
      compared = await api.mapServerCompare(server, [...new Set(folders)]);
    } catch (err) {
      say(errText(err), {}, true);
    }
  }
  const byId = (id: string | null) => (id ? all.find((m) => m.id === id) : undefined);

  // --- check one Workshop map without installing it ---
  let checkText = $state("");
  let checking = $state(false);
  let checkStopping = $state(false);
  function stopCheck() {
    checkStopping = true;
    api.mapCheckStop().catch(() => {});
  }
  /** The Workshop item in a link or text: its `id=`, else the last long number (never the game's own app id). */
  const workshopIdOf = (text: string) => {
    const id = text.match(/[?&]id=(\d+)/)?.[1] ?? text.match(/\d{4,}/g)?.at(-1);
    return id === "108600" ? undefined : id;
  };
  async function check(e: Event) {
    e.preventDefault();
    const id = workshopIdOf(checkText.trim());
    if (!id) return say("maps.check.pasteId", {}, true);
    checking = true;
    checkStopping = false;
    try {
      const { item, members } = await api.workshopInfo(id);
      if (!item) throw new Error(t("maps.check.noItem", { id }));
      if (item.collection) return await checkCollection(item.title, members);
      if (item.have) return say("maps.check.alreadyInstalled", { title: item.title });
      let found;
      try {
        found = await api.mapCheck(id, false);
      } catch (err) {
        // only a missing file list is what a SteamCMD download gets round (not being offline, say)
        if (checkStopping || !String(err).includes("maps.err.fileList")) throw err;
        const why = errParts(err);
        if (!(await confirmed(t("maps.check.confirmDownload", { title: item.title, error: why.detail || why.text, size: fmtSize(item.file_size) }), { label: t("maps.check.download"), icon: "download" }))) return;
        found = await api.mapCheck(id, true);
      }
      checkText = "";
      setPreview(new Set([...preview, ...found.map((m) => m.id)]));
      if (found.length) prefs.mapsTab = "mods";
      say(found.length ? "maps.check.read" : "maps.check.noMaps", { title: item.title, build: String(data?.build) });
      await load();
    } catch (err) {
      say(errText(err), {}, true);
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
      if (checkStopping) break;
      const known = all.filter((m) => itemOf(m.source) === it.id);
      if (known.length) ids.push(...known.map((m) => m.id));
      else if (!it.have && it.tags.includes("Map")) {
        say("maps.check.readingMember", { title: it.title, n: fmtNum(i + 1), total: fmtNum(members.length) });
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
    if (ids.length) prefs.mapsTab = "mods";
    // up to three sentences, logged as one line
    say(
      [
        t("maps.check.collection", { title, count: ids.length, items: fmtNum(members.length) }),
        failed.length && t("maps.check.collectionFailed", { titles: listOf(failed) }),
        unread && t("maps.check.collectionUntagged", { count: unread }),
      ]
        .filter(Boolean)
        .join(" "),
      {},
      failed.length > 0,
    );
  }

  // --- the tool panel: tabs in a window floating over the map (a sheet over its bottom in narrow windows),
  // moved by its header, sized by its edges, minimisable ---
  const TABS: [string, string, string, string][] = [["mods", "maps.mapMods", "", "layers"], ["req", "maps.req.title", "", "alert"], ["order", "maps.tabs.order", "maps.export.title", "swap"], ["compare", "maps.tabs.compare", "maps.compare.title", "server"], ["check", "maps.tabs.check", "maps.check.title", "search"], ["bg", "maps.bg.title", "", "image"]];
  const TAB_IDS = TABS.map(([id]) => id);
  const sideTab = $derived(TAB_IDS.includes(prefs.mapsTab) ? prefs.mapsTab : "mods");
  /** Requirements that aren't installed (the Requirements tab's badge). */
  const missing = $derived(needs.filter((n) => !(n.id && mods.has(n.id))).length);
  /** A tab's count on its icon: the listed maps, the missing requirements. */
  const countOf = (id: string) => (id === "mods" ? compact.format(listed.length) : id === "req" && missing ? fmtNum(missing) : "");
  /** The open tab's badge, on the minimised panel. */
  const tabBadge = $derived(sideTab === "req" && missing ? t("maps.tabs.missing", { count: missing }) : countOf(sideTab));
  let side = $state<HTMLElement>();
  let mapW = $state(0);
  /** The map's toolbar height: the map (and the panel's box) starts below it. */
  let barH = $state(0);
  /** Narrow windows: the panel is a sheet over the map's bottom (as the styles below), not movable. */
  const narrow = new MediaQuery("max-width: 960px");
  /** Smallest panel; its inset from the map's edges; the height of the grid's x-axis label row. */
  const [MIN_W, MIN_H, GAP, AXIS] = [360, 320, 12, 16];
  const clamp = (v: number, lo: number, hi: number) => Math.round(Math.min(hi, Math.max(lo, v)));
  /** A box kept inside the map; docked (no x, y or h): at the right, below the axis labels, full height. */
  function inside(p: typeof prefs.mapsPanel) {
    const w = clamp(p.w, MIN_W, mapW);
    const h = clamp(p.h ?? height - AXIS - 2 * GAP, MIN_H, height);
    return { w, h, x: clamp(p.x ?? mapW - GAP - w, 0, mapW - w), y: clamp(p.y ?? AXIS + GAP, 0, height - h) };
  }
  const box = $derived(inside(prefs.mapsPanel));
  /** How each grip moves the box: [x, w] per pixel across, [y, h] per pixel down. */
  const GRIPS = { move: [1, 0, 1, 0], left: [1, -1, 0, 0], right: [0, 1, 0, 0], bottom: [0, 0, 0, 1], corner: [0, 1, 0, 1] };
  type Grip = keyof typeof GRIPS;
  const EDGES: Grip[] = ["left", "right", "bottom"];
  function shift(b: typeof box, grip: Grip, dx: number, dy: number) {
    const [kx, kw, ky, kh] = GRIPS[grip];
    // an edge stops at the map's edge; the left one keeps the right edge put
    const w = kw ? clamp(b.w + kw * dx, MIN_W, kx ? b.x + b.w : mapW - b.x) : b.w;
    const h = kh ? clamp(b.h + kh * dy, MIN_H, height - b.y) : b.h;
    prefs.mapsPanel = inside({ x: kw && kx ? b.x + b.w - w : b.x + kx * dx, y: b.y + ky * dy, w, h });
  }
  function grab(e: PointerEvent, grip: Grip) {
    if (narrow.current || (e.target as HTMLElement).closest("button")) return;
    const [x0, y0, b, el] = [e.clientX, e.clientY, box, e.currentTarget as HTMLElement];
    el.setPointerCapture(e.pointerId);
    const to = (m: PointerEvent) => shift(b, grip, m.clientX - x0, m.clientY - y0);
    const end = () => {
      el.removeEventListener("pointermove", to);
      el.removeEventListener("pointerup", end);
    };
    el.addEventListener("pointermove", to);
    el.addEventListener("pointerup", end);
  }
  /** Arrow keys on a grip: 20px a press, 80px with Shift. */
  function nudge(e: KeyboardEvent, grip: Grip) {
    const d = e.shiftKey ? 80 : 20;
    const [dx, dy] = ({ ArrowLeft: [-d, 0], ArrowRight: [d, 0], ArrowUp: [0, -d], ArrowDown: [0, d] } as Record<string, number[]>)[e.key] ?? [0, 0];
    if (narrow.current || e.target !== e.currentTarget || !(dx || dy)) return;
    e.preventDefault();
    shift(box, grip, dx, dy);
  }
  /** Double-clicking the header docks the panel back: at the right, full height. */
  function dock(e: MouseEvent) {
    if (!narrow.current && !(e.target as HTMLElement).closest("button")) prefs.mapsPanel = { x: null, y: null, w: box.w, h: null };
  }

  // --- the grid (MapGrid): coloured by kind, or per map ---
  let height = $state(0);
  let width = $state(0);
  let mapGrid = $state<ReturnType<typeof MapGrid>>();
  let hideVanilla = $state(false);
  let colours = $state(false);
  let help = $state(false);

  /**
   * Per-map colours, in the order maps are shown: hues a golden angle apart round the whole
   * wheel except 45° either side of the conflict red (--danger, hue ~32), the first map blue
   * like a plain map cell; past 10 maps every other map takes a darker band, so maps of near
   * hues still differ in lightness.
   */
  function swatchOf(i: number, n: number) {
    const [band, j] = n > 10 ? [i % 2, i >> 1] : [0, i];
    const hue = 77 + (((230 + j * 137.508 + band * 68.75) % 360) * 270) / 360;
    return `oklch(${n > 10 ? (band ? 0.56 : 0.78) : 0.68} 0.14 ${hue.toFixed(1)})`;
  }
  const colourOf = $derived(new Map(shown.map((m, i) => [m.id, swatchOf(i, shown.length)])));

  /** Where a map's first folder stands in Map= (unlisted ones after the rest). */
  const lineRank = (m: MapMod) => Math.min(1e9, ...m.maps.map((f) => mapLine.indexOf(f.folder)).filter((i) => i >= 0));
  /** The maps whose world is drawn, bottom first: the game's own (unless hidden), then the drawn
   * installed maps from the last in Map= to the first, which wins where they overlap. */
  const worldIds = $derived(
    prefs.mapsWorld && data
      ? [...(hideVanilla ? [] : [""]), ...drawn.filter((m) => data!.mods.some((x) => x.id === m.id)).sort((a, b) => lineRank(b) - lineRank(a)).map((m) => m.id)]
      : [],
  );
  const swatch = (id: string) => colourOf.get(id) ?? "transparent";
  const FILL = { vanilla: "--muted", water: "--map-water", map: "--map-cell", over: "--map-over", conflict: "--danger" };
  function paint(c: GridCell): [string, number] | null {
    const base = c.kind === "vanilla" || c.kind === "water";
    if (base && hideVanilla) return null;
    return [colours && c.maps.length === 1 ? swatch(c.maps[0]) : FILL[c.kind], base ? 0.3 : 0.9];
  }

  /** The part of the map the open tool panel leaves clear: Fit and the zoom buttons use it. */
  function clear() {
    const all = { x: 0, y: 0, w: width, h: height };
    if (prefs.mapsMin || !side || !mapGrid) return all;
    // above the sheet in narrow windows, else the biggest strip beside, above or below the panel
    if (narrow.current) return { ...all, h: Math.min(height, side.getBoundingClientRect().top - mapGrid.top()) };
    const { x, y, w, h } = box;
    const strips = [{ ...all, w: x }, { ...all, x: x + w, w: width - x - w }, { ...all, h: y }, { ...all, y: y + h, h: height - y - h }];
    return strips.reduce((a, b) => (b.w * b.h > a.w * a.h ? b : a));
  }
  function pick([x, y]: Cell, shift: boolean) {
    const has = pickedCells.some((c) => c[0] === x && c[1] === y);
    if (shift) pickedCells = has ? pickedCells.filter((c) => c[0] !== x || c[1] !== y) : [...pickedCells, [x, y]];
    else pickedCells = has && pickedCells.length === 1 ? [] : [[x, y]];
    prefs.mapsTab = "mods"; // the list shows the maps there
  }

  // --- a picture of the player's own under the grid (instead of the world map), lined up from two spots ---
  $effect(() => void (data && readBackground(data.build)));
  const picture = $derived(data ? backgrounds[data.build] : null);
  const tilesPer = $derived(data?.build === 41 ? 300 : 256);
  /** Lining it up: the spots set so far (in image pixels and world tiles), the image spot waiting for its world spot. */
  let cal = $state<{ done: { img: number[]; world: number[] }[]; img: number[] | null } | null>(null);
  let spotText = $state("");
  let spotUnit = $state<"tiles" | "cells">("tiles");
  let calError = $state("");

  async function placePicture(bg: MapBackground | null, img: ImageBitmap | null = null) {
    if (!data) return;
    const build = data.build;
    try {
      await api.mapBackgroundSet(build, bg);
    } catch (e) {
      return say(errText(e), {}, true);
    }
    backgrounds[build] = bg && { bg, img: img ?? backgrounds[build]?.img ?? null };
  }
  async function pickPicture() {
    const path = await api.pickImage().catch((e) => (say(errText(e), {}, true), null));
    const img = path && (await loadImage(path));
    if (!path || !data) return;
    if (!img) return say("maps.bg.unreadable", { path }, true);
    // until it's lined up, fitted over the grid
    const [xs, ys] = grid.length ? [grid.map((c) => c.x), grid.map((c) => c.y)] : [[0, 59], [0, 59]];
    const [x0, y0] = [Math.min(...xs), Math.min(...ys)];
    const [w, h] = [(Math.max(...xs) + 1 - x0) * tilesPer, (Math.max(...ys) + 1 - y0) * tilesPer];
    const k = Math.min(w / img.width, h / img.height);
    await placePicture({ path, scale: [k, k], offset: [x0 * tilesPer, y0 * tilesPer] }, img);
    prefs.mapsWorld = true;
    calibrate();
  }
  function calibrate() {
    cal = { done: [], img: null };
    calError = "";
    spotText = "";
  }
  /** A spot pointed at on the grid (in cells): on the picture first, then in the world. */
  function point(cx: number, cy: number) {
    if (!cal || !picture) return;
    const [x, y] = [cx * tilesPer, cy * tilesPer];
    const { scale, offset } = picture.bg;
    calError = "";
    if (cal.img) setSpot([x, y]);
    else cal.img = [(x - offset[0]) / scale[0], (y - offset[1]) / scale[1]];
  }
  function setSpot(world: number[]) {
    if (!cal?.img || !picture) return;
    cal.done.push({ img: cal.img, world });
    cal.img = null;
    spotText = "";
    if (cal.done.length < 2) return;
    const fit = solveBackground(cal.done[0], cal.done[1]);
    if (!fit) {
      cal.done = [];
      calError = t("maps.bg.tooClose");
      return;
    }
    cal = null;
    placePicture({ ...picture.bg, ...fit }).then(() => say("maps.bg.lined"));
  }
  function typedSpot(e: SubmitEvent) {
    e.preventDefault();
    const n = spotText.match(/-?\d+(\.\d+)?/g)?.map(Number) ?? [];
    if (n.length !== 2) return void (calError = t("maps.bg.badSpot"));
    const k = spotUnit === "cells" ? tilesPer : 1;
    calError = "";
    setSpot([n[0] * k, n[1] * k]);
  }
  function resetPicture() {
    cal = null;
    placePicture(null);
  }

  const KIND = { vanilla: "maps.kind.vanilla", water: "maps.kind.water", map: "", over: "maps.kind.over", conflict: "maps.kind.conflict" };
</script>

<svelte:window onkeydown={(e) => (cal && e.key === "Escape" ? (e.preventDefault(), (cal = null)) : undoKey(e))} />

<section class="maps" aria-label={t("maps.label")} style:--bar="{barH}px" bind:clientWidth={mapW}>
  <div class="world">
    <div class="mapbar" bind:offsetHeight={barH}>
    <div class="head">
      <h2>{t("maps.worldMap")}</h2>
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
    </div>
    <div class="row layers">
      <label class="check"><input type="checkbox" bind:checked={hideVanilla} />{t("maps.hideVanilla")}</label>
      <label class="check"><input type="checkbox" bind:checked={colours} />{t("maps.perMapColours")}</label>
      <label class="check" title={t("maps.worldTip")}><input type="checkbox" bind:checked={prefs.mapsWorld} />{t("maps.world")}</label>
      <span class="zoom">
        <button class="quiet" onclick={() => mapGrid?.zoom(1.25)} aria-label={t("maps.zoomIn")}><Icon name="plus" size={14} />{t("maps.zoomIn")}</button>
        <button class="quiet" onclick={() => mapGrid?.zoom(0.8)}>{t("maps.zoomOut")}</button>
        <button class="quiet" onclick={() => mapGrid?.fit()}>{t("maps.fit")}</button>
        <button class="quiet icon" onclick={undo} disabled={busy || !(history.length || canUndo)} title={t("maps.undoTip")} aria-label={t("maps.undoLabel")}><Icon name="undo" size={16} /></button>
        <button class="quiet" onclick={() => (help = !help)} aria-expanded={help}><Icon name="info" size={14} />{t("maps.help.button")}</button>
      </span>
    </div>
    </div>
    {#if help}
      <div class="help" role="dialog" aria-modal="true" tabindex="-1" aria-label={t("maps.help.label")} use:trap onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), (help = false))}>
        <p class="lede">{t("maps.lede")}</p>
        <h3>{t("maps.help.gridTitle")}</h3>
        <p>{@html th("maps.help.grid", { build: data?.build ?? "" })}</p>
        <h3>{t("maps.help.previewTitle")}</h3>
        <p>{@html th("maps.help.preview")}</p>
        <h3>{t("maps.help.workshopTitle")}</h3>
        <p>{t("maps.help.workshop")}</p>
        <button class="quiet" onclick={() => (help = false)}><Icon name="x" size={14} />{t("common.close")}</button>
      </div>
    {/if}
    <MapGrid bind:this={mapGrid} bind:width bind:height {grid} {paint} worlds={worldIds} backdrop={data && (cal ? !cal.img : prefs.mapsWorld) ? data.build : undefined} onPoint={cal ? point : undefined} tiles={tilesPer} rings={pickedCells.map((c) => [c, "--accent-text"])} area={clear} ready={!!mapW} onPick={pick}
      label={t("maps.gridLabel", { maps: fmtNum(shown.length), conflicts: fmtNum(conflictCells), over: fmtNum(overCells) })}>
      {#snippet tip([x, y])}
        {@const hovered = byCell.get(`${x},${y}`)}
        <strong>{t("maps.cell", { x, y })}</strong>
        {#if hovered}
          {#if KIND[hovered.kind]}<span class="kind {hovered.kind}">{t(KIND[hovered.kind])}</span>{/if}
          {#each hovered.maps as id}<span>{names.get(id) ?? id}</span>{/each}
        {:else}
          <span class="hint">{t("maps.empty")}</span>
        {/if}
      {/snippet}
      {#if !data}<p class="loading">{t("maps.readingFiles")}</p>{:else if gridDone && !grid.length}<p class="loading">{t("maps.gridEmpty")}</p>{/if}
      {#if prefs.mapsMin}
        <button class="restore" onclick={() => (prefs.mapsMin = false)} aria-expanded="false" title={t("maps.restoreTip")}>{t("maps.tabs.label")}{#if tabBadge}<span class="badge" class:bad={sideTab === "req"}>{tabBadge}</span>{/if}</button>
      {/if}
    </MapGrid>
    {#if data && !data.vanilla}
      <p class="hint">{t("maps.noVanilla", { build: data.build })}</p>
    {/if}
    {#if picture && !picture.img}
      <p class="hint">{t("maps.bg.missing", { path: picture.bg.path })}</p>
    {/if}
  </div>

  {#if !prefs.mapsMin}
  <aside class="side" id="maps-panel" aria-label={t("maps.tabs.label")} bind:this={side} style:--x="{box.x}px" style:--y="{box.y}px" style:--w="{box.w}px" style:--h="{box.h}px">
    <!-- window splitters (focusable separators) on its edges: svelte counts separators as non-interactive -->
    {#each EDGES as grip (grip)}
      {@const tall = grip === "bottom"}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div class="resize {grip}" role="separator" aria-orientation={tall ? "horizontal" : "vertical"} aria-label={t(tall ? "maps.heightLabel" : "maps.resizeLabel")} title={t(tall ? "maps.heightTip" : "maps.resizeTip")} tabindex="0"
        aria-valuenow={tall ? box.h : box.w} aria-valuemin={tall ? MIN_H : MIN_W} aria-valuemax={tall ? height : mapW} onpointerdown={(e) => grab(e, grip)} onkeydown={(e) => nudge(e, grip)}></div>
    {/each}
    <!-- the corner sizes both ways (the edges do it by keyboard) -->
    <div class="resize corner" aria-hidden="true" onpointerdown={(e) => grab(e, "corner")}></div>
    <!-- an activity rail, like an IDE's: icons with their names in tooltips, the open tab's name in the header -->
    <div class="rail" role="tablist" aria-orientation="vertical" aria-label={t("maps.tabs.label")} tabindex="-1" onkeydown={(e) => arrowPick(e, TAB_IDS, sideTab, (v) => (prefs.mapsTab = v))}>
      {#each TABS as [id, label, tip, icon] (id)}
        {@const count = countOf(id)}
        <button role="tab" id="maps-tab-{id}" aria-controls="maps-pane" aria-selected={sideTab === id} tabindex={sideTab === id ? 0 : -1} class:active={sideTab === id}
          title={t(tip || label)} aria-label={count ? `${t(label)} (${count})` : t(label)} onclick={() => (prefs.mapsTab = id)}>
          <Icon name={icon} size={18} />
          {#if count}<span class="badge" class:bad={id === "req"}>{count}</span>{/if}
        </button>
      {/each}
    </div>
    <div class="main">
    <!-- the title bar moves the panel: drag it, or arrow keys once focused; a double-click docks it -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div class="top" role="group" aria-label={t("maps.moveLabel")} title={narrow.current ? undefined : t("maps.moveTip")} tabindex={narrow.current ? undefined : 0}
      onpointerdown={(e) => grab(e, "move")} onkeydown={(e) => nudge(e, "move")} ondblclick={dock}>
      <h2>{t(TABS.find(([id]) => id === sideTab)![1])}</h2>
      <button class="quiet icon" onclick={() => (prefs.mapsMin = true)} aria-expanded="true" aria-controls="maps-panel" title={t("maps.minimise")} aria-label={t("maps.minimise")}><Icon name="minimize" size={16} /></button>
    </div>
    <div class="pane" class:rows={sideTab === "mods"} id="maps-pane" role="tabpanel" aria-labelledby="maps-tab-{sideTab}">
      {#if sideTab === "mods"}
        <div class="source">
          <!-- reading from Steam starts when it's ticked (or on Refresh), never just by opening the tab -->
          <label class="check"><input type="checkbox" bind:checked={prefs.mapsWorkshop} onchange={() => prefs.mapsWorkshop && !scan.running && startScan()} />{t("maps.workshopSource")}</label>
          <span class="hint note">{t("maps.workshopSourceNote")}</span>
          {#if prefs.mapsWorkshop}
            <span class="hint" class:bad={!!scan.error} role="status" id="maps-scan">{scanText}</span>
            {#if scan.running}
              {#if scan.p?.total}<progress max={scan.p.total} value={scan.p.done} aria-labelledby="maps-scan"></progress>{/if}
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
            <option value="order">{t("maps.sort.order")}</option>
            <option value="name">{t("maps.sort.name")}</option>
            <option value="cells">{t("maps.sort.cells")}</option>
            <option value="conflicts">{t("maps.sort.conflicts")}</option>
            <option value="state">{t("maps.sort.state")}</option>
            <option value="subs">{t("maps.sort.subs")}</option>
            <option value="updated">{t("maps.sort.updated")}</option>
            <option value="newest">{t("maps.sort.newest")}</option>
          </select>
          <label class="check" title={t("maps.hideSpawnTip")}><input type="checkbox" bind:checked={hideSpawn} />{t("maps.hideSpawn")}</label>
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
          {#if hiddenCount}<button onclick={() => (hidden = new Set())} title={t("maps.unhideTip")}>{t("maps.unhide", { count: hiddenCount })}</button>{/if}
        </div>
        {#if data && !all.length}
          <p class="hint">{t("maps.noMaps", { build: data.build })}</p>
        {/if}
        <div class="scroll">
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
                  <span class="title"><strong title={nameOf(m)}>{nameOf(m)}</strong>{#if on}<span class="on">{t("common.enabled")}</span>{:else if !installed}<span class="off">{t("maps.notInstalled")}</span>{/if}</span>
                  <span class="hint">{[cellCount(m) ? t("maps.cellCount", { count: cellCount(m) }) : t("maps.spawnPoints"), listOf(m.maps.map((f) => f.folder), "unit"), meta?.author && t("maps.byAuthor", { author: meta.author }), meta?.subscriptions && t("maps.subscribers", { count: meta.subscriptions, n: compact.format(meta.subscriptions) }), !installed && meta?.size && fmtSize(meta.size), meta?.updated && t("maps.updatedAgo", { when: fmtAgo(meta.updated) })].filter(Boolean).join(" · ")}</span>
                  {#each [...(clash?.with ?? [])].slice(0, 3) as [rival, cells] (rival)}
                    <span class="bad line" title={t("maps.conflictsWith", { count: cells.length, name: names.get(rival) ?? rival, cells: cellList(cells) }).replace(/<[^>]+>/g, "")}><span class="txt">{@html th("maps.conflictsWith", { count: cells.length, name: names.get(rival) ?? rival, cells: cellList(cells) })}</span>
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
                    <span class="dupe">{t(others.length > 2 ? "maps.dupeFolderMore" : "maps.dupeFolder", { folder: f.folder, names: listOf(others.slice(0, 2).map((id) => names.get(id) ?? id), others.length > 2 ? "unit" : "conjunction"), count: others.length - 2 })}</span>
                  {/each}
                </span>
                <span class="btns">
                  {#if isShown}<button class="quiet icon" onclick={() => toggleHidden(m.id)} title={hidden.has(m.id) ? t("maps.showOnGridTip") : t("maps.hideOnGridTip")} aria-label={hidden.has(m.id) ? t("maps.showOnGrid") : t("maps.hideOnGrid")} aria-pressed={hidden.has(m.id)}><Icon name={hidden.has(m.id) ? "eye-off" : "eye"} size={16} /></button>{/if}
                  {#if item}<button class="quiet icon" onclick={() => api.openUrl(workshopUrl(item))} title={t("maps.openWorkshopTip")} aria-label={t("common.workshop")}><Icon name="steam" size={16} /></button>{/if}
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
            <button class="quiet more" onclick={() => (limit += 150)}>{t("maps.showMore", { count: Math.min(150, listed.length - limit), total: fmtNum(listed.length) })}</button>
          {/if}
        </div>

      {:else if sideTab === "req"}
        {#if needs.length}
          <div class="seg" role="radiogroup" aria-label={t("maps.req.viewLabel")}>
            <button type="button" role="radio" aria-checked={reqView === "list"} class:active={reqView === "list"} onclick={() => (reqView = "list")} title={t("maps.req.neededByTip")}>{t("maps.req.neededBy")}</button>
            <button type="button" role="radio" aria-checked={reqView === "tree"} class:active={reqView === "tree"} onclick={() => (reqView = "tree")} title={t("maps.req.treeTip")}>{t("maps.req.tree")}</button>
          </div>
        {/if}
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
                      <span class="hint">{t(n.folder && n.id ? "maps.req.neededByMap" : n.folder ? "maps.req.neededByFolder" : "maps.req.neededByOnly", { folder: n.folder, names: listOf(n.by.map((id) => names.get(id) ?? id)) })}</span>
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

      {:else if sideTab === "compare"}
        <p class="lede">{@html th("maps.compare.lede")}</p>
        {#if servers.length}
          <div class="row">
            <select bind:value={server} onchange={() => (compared = null)} aria-label={t("maps.compare.serverLabel")}>
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
            <p class="mono hint">{listOf(compared.only_you, "unit")}</p>
          {/if}
        {/if}

      {:else if sideTab === "order"}
        <p class="lede">{@html th("maps.export.lede")}</p>
        {#if mapLine.length}
          <h3>{t("maps.export.orderTitle")}</h3>
          <p class="hint">{@html th("maps.export.orderHint")}</p>
          <MapOrder bind:maps={mapLine} onarrange={arranged} lotsOf={(f) => lotsByFolder.get(f) ?? []} />
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
              {#each Object.entries(presets) as [name, ids] (name)}<option value={name}>{name} ({fmtNum(ids.length)})</option>{/each}
            </select>
            <button onclick={loadPreset} disabled={!presetPick}>{t("maps.presets.preview")}</button>
            <button class="danger" onclick={deletePreset} disabled={!presetPick}><Icon name="trash" size={14} />{t("common.remove")}</button>
          {/if}
        </div>
        <form class="row" onsubmit={savePreset}>
          <input type="text" bind:value={presetName} placeholder={t("maps.presets.namePlaceholder")} aria-label={t("maps.presets.namePlaceholder")} />
          <button type="submit" disabled={!presetName.trim() || !shown.length}><Icon name="plus" size={14} />{t("maps.presets.save", { count: shown.length })}</button>
        </form>

      {:else if sideTab === "bg"}
        <p class="lede">{t("maps.bg.lede", { build: data?.build ?? "" })}</p>
        <div class="row">
          <button onclick={pickPicture} disabled={!data}><Icon name="image" size={14} />{t("maps.bg.pick")}</button>
          {#if picture}
            <button onclick={calibrate} disabled={!picture.img || !!cal}><Icon name="pencil" size={14} />{t("maps.bg.calibrate")}</button>
            <button class="quiet" onclick={resetPicture} title={t("maps.bg.resetTip")}><Icon name="rollback" size={14} />{t("maps.bg.reset")}</button>
          {/if}
        </div>
        {#if picture}
          <p class="mono hint path">{picture.bg.path}</p>
          {#if !picture.img}<p class="bad">{t("maps.bg.missing", { path: picture.bg.path })}</p>{/if}
        {/if}
        {#if cal}
          <h3>{t("maps.bg.spot", { n: cal.done.length + 1 })}</h3>
          <ol class="steps">
            <li class:now={!cal.img}>{t("maps.bg.stepImage")}</li>
            <li class:now={!!cal.img}>{t("maps.bg.stepWorld")}</li>
          </ol>
          {#if cal.img}
            <form class="row" onsubmit={typedSpot}>
              <input type="text" bind:value={spotText} placeholder="10620, 9750" spellcheck="false" aria-label={t("maps.bg.spotLabel")} />
              <select bind:value={spotUnit} aria-label={t("maps.bg.unitLabel")}>
                <option value="tiles">{t("maps.bg.tiles")}</option>
                <option value="cells">{t("maps.bg.cells")}</option>
              </select>
              <button type="submit" disabled={!spotText.trim()}><Icon name="check" size={14} />{t("maps.bg.set")}</button>
            </form>
          {/if}
          {#if calError}<p class="bad" role="alert">{calError}</p>{/if}
          <div class="row"><button class="quiet" onclick={() => (cal = null)} title={t("maps.bg.cancelTip")}><Icon name="x" size={14} />{t("common.cancel")}</button></div>
        {/if}

      {:else}
        <p class="lede">{t("maps.check.lede")}</p>
        <form class="row" onsubmit={check}>
          <input type="text" bind:value={checkText} placeholder={t("maps.check.placeholder")} spellcheck="false" aria-label={t("maps.check.placeholder")} />
          <button type="submit" disabled={checking || !checkText.trim()}><Icon name="search" size={14} />{checking ? t("maps.check.checking") : t("maps.check.check")}</button>
          {#if checking}
            <button type="button" class="quiet" onclick={stopCheck} disabled={checkStopping}><Icon name="x" size={14} />{checkStopping ? t("maps.scan.stopping") : t("maps.scan.stop")}</button>
          {/if}
        </form>
      {/if}
    </div>
    </div>
  </aside>
  {/if}
</section>

<style>
  /* the map fills the tab edge to edge under a slim toolbar; the tool panel is a window floating over it
     (a sheet over its bottom in narrow windows): the tab never scrolls, only the panes do */
  .maps {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    overflow: hidden;
    min-height: 0;
  }
  .side {
    position: absolute;
    z-index: 1;
    top: calc(var(--bar) + var(--y));
    left: var(--x);
    width: var(--w);
    height: var(--h);
    display: flex;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: var(--card-shadow), var(--shadow);
  }
  @media (max-width: 960px) {
    .side {
      top: auto;
      left: 12px;
      right: 12px;
      bottom: 12px;
      width: auto;
      /* tall enough for a few rows (or all the room below the toolbar in short windows), so the
         list is reachable; Minimise shows the whole map */
      height: max(45%, min(200px, calc(100% - var(--bar) - 24px)));
    }
    /* Help says it too: the rows get the room */
    .source .note {
      display: none;
    }
    .resize {
      display: none;
    }
    .side .top {
      cursor: auto;
    }
    /* too short to keep the list's controls put: the whole pane scrolls */
    .side .pane.rows {
      display: block;
      overflow-y: auto;
    }
    .side .pane.rows > .scroll {
      overflow: visible;
    }
  }
  .world {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--bg);
  }
  .mapbar {
    padding: 8px 16px 0;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .world > .hint {
    padding: 6px 16px;
  }
  .world :global(.map-grid) {
    flex: 1;
    min-height: 0;
  }
  /* minimised: a tab at the map's top right (below the axis labels) brings the panel back */
  .restore {
    position: absolute;
    z-index: 1;
    top: 28px;
    right: 12px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--panel);
    box-shadow: var(--card-shadow), var(--shadow);
  }
  /* the panel: an icon rail down its left edge, then the open tab */
  .rail {
    flex: none;
    width: 44px;
    display: flex;
    flex-direction: column;
    /* a short sheet scrolls its icons rather than squashing them */
    overflow-y: auto;
    scrollbar-width: none;
    padding-top: 6px;
    background: var(--raised);
    border-radius: 7px 0 0 7px;
  }
  .rail button {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    height: 42px;
    padding: 0;
    border: 0;
    border-radius: 0;
    border-left: 2px solid transparent;
    background: transparent;
    color: var(--muted);
  }
  .rail button:hover:not(:disabled),
  .rail button.active {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-left-color: var(--on-selected);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  /* a count on the icon's corner */
  .rail .badge {
    position: absolute;
    top: 3px;
    right: 2px;
    padding: 0 4px;
    font-size: 12px;
    line-height: 14px;
    color: var(--text);
    background: var(--line);
  }
  .rail .badge.bad {
    background: var(--danger);
    color: var(--panel);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 16px;
    border-bottom: 1px solid var(--line);
    cursor: move;
    user-select: none;
    touch-action: none;
  }
  .top:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .top h2 {
    flex: 1;
    font-size: 15px;
  }
  .badge {
    padding: 0 6px;
    border-radius: 8px;
    background: var(--raised);
    font-size: 12px;
    font-weight: 700;
  }
  .badge.bad {
    color: var(--danger);
  }
  /* reaches 6px over the rail: a picked row's chevron, on the rail's edge, isn't cut off */
  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin-left: -6px;
    padding: 12px 16px 16px 22px;
  }
  /* Map mods: the controls stay put, the rows scroll */
  .pane.rows {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding-bottom: 0;
  }
  .pane.rows > * {
    flex: none;
  }
  .pane.rows > .scroll {
    flex: 1 1 auto;
    min-height: 120px;
    overflow-y: auto;
    margin: 0 -8px 0 -22px;
    padding: 0 8px 12px 22px;
  }
  .pane > .seg {
    width: fit-content;
    margin-bottom: 10px;
  }
  /* the name line: the name truncates so its badges and "Hide on grid" stay on the line */
  .name > .title {
    display: flex;
    align-items: baseline;
    min-width: 0;
    white-space: nowrap;
  }
  .name > .title strong {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name > .title > :not(strong) {
    flex: none;
  }
  /* each warning is one line; the full text is in its tooltip */
  .name > .bad,
  .name > .over,
  .name > .dupe,
  .name > .hint {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name > .bad.line {
    display: flex;
    align-items: baseline;
    min-width: 0;
  }
  .bad.line .txt {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bad.line .link {
    flex: none;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 16px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
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
  /* under the toolbar, over the grid */
  .help {
    position: absolute;
    z-index: 2;
    right: 16px;
    width: min(460px, calc(100% - 32px));
    padding: 4px 18px 14px;
    background: var(--raised);
    border-radius: 8px;
    box-shadow: var(--shadow);
  }
  .help .lede {
    margin: 12px 0 0;
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
    margin: 2px 0 6px;
    color: var(--muted);
  }
  .zoom {
    margin-left: auto;
    display: flex;
    gap: 2px;
  }
  /* the tool panel's edges and corner: drag them to resize (they straddle its border) */
  .resize {
    position: absolute;
    z-index: 1;
    top: 0;
    bottom: 0;
    left: -5px;
    width: 8px;
    cursor: ew-resize;
    touch-action: none;
    background: linear-gradient(var(--muted), var(--muted)) center / 3px 36px no-repeat;
    border-radius: 4px;
    opacity: 0.4;
  }
  .resize.right {
    left: auto;
    right: -5px;
  }
  .resize.bottom {
    top: auto;
    bottom: -5px;
    left: 0;
    right: 0;
    width: auto;
    height: 8px;
    cursor: ns-resize;
    background-size: 36px 3px;
  }
  .resize.corner {
    z-index: 2;
    top: auto;
    left: auto;
    right: -5px;
    bottom: -5px;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    background: none;
  }
  .resize:hover,
  .resize:focus-visible {
    opacity: 1;
    background-color: color-mix(in oklab, var(--accent) 16%, transparent);
  }
  .resize:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .loading {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    color: var(--muted);
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
  .mods li,
  .needs li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    margin: 0 -8px;
    border-radius: 4px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .mods li {
    position: relative;
    cursor: pointer;
  }
  /* hovered, and shown (picked) maps: blood red under on-selected text, inset 8px from the pane */
  .mods li:hover,
  .needs li:hover {
    background: var(--hover);
    color: var(--on-selected);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .mods li.shown {
    background: var(--accent-soft);
    color: var(--on-selected);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .mods li:is(:hover, .shown) :is(.hint, .off),
  .needs li:hover .hint {
    color: color-mix(in oklab, var(--on-selected) 72%, transparent);
  }
  /* the warning lines too: their own red and amber are unreadable on the highlight (Daylight's ~1.7:1) */
  .mods li:is(:hover, .shown) :is(.on, .over, .dupe, .bad) {
    color: var(--on-selected);
  }
  /* the chevron, centred on the pane's left edge (the row starts 8px in) */
  .mods li:is(:hover, .shown)::before {
    content: "";
    position: absolute;
    left: -13px;
    top: 50%;
    width: 10px;
    height: 14px;
    margin-top: -7px;
    background: var(--on-selected);
    clip-path: polygon(0 0, 45% 0, 100% 50%, 45% 100%, 0 100%, 55% 50%);
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
  .path {
    overflow-wrap: anywhere;
  }
  .steps {
    margin: 4px 0 0;
    padding-left: 20px;
    color: var(--muted);
  }
  .steps .now {
    color: var(--text);
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
  .source .note {
    flex-basis: 100%;
    order: 1;
  }
  .tree li {
    border-bottom-style: dashed;
  }
</style>
