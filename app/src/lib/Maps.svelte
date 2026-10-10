<script lang="ts" module>
  import { listen } from "@tauri-apps/api/event";
  import type { MapScan } from "./api";

  // the Workshop scan outlives the tab (it runs in the backend), so its state lives here
  const scan = $state({ running: false, p: null as MapScan | null, error: "", ticks: 0, started: false });
  let listening = false;
  function follow() {
    if (listening) return;
    listening = true;
    listen<MapScan>("maps-scan", (e) => {
      scan.p = e.payload;
      // show what's read so far every 50 items
      if (e.payload.phase === "reading" && e.payload.done % 50 === 0) scan.ticks++;
    }).catch(() => (listening = false));
  }
</script>

<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, type Cell, type GridCell, type MapCompare, type MapMod, type MapsView, type ModRow } from "./api";
  import Icon from "./Icon.svelte";
  import MapOrder from "./MapOrder.svelte";
  import { compact, fmtSize, workshopUrl } from "./format";
  import { prefs } from "./prefs.svelte";

  let { busy, mods, say, onSetMods, onInstallWorkshop, onShowMod, canUndo, onUndo }: {
    busy: boolean;
    /** The library by mod id: what's installed and on. */
    mods: Map<string, ModRow>;
    /** A line in the activity log. */
    say: (text: string, error?: boolean) => void;
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

  const load = () => api.mapsView().then((v) => (data = v)).catch((e) => say(String(e), true));
  // after installs, removals and toggles (busy flips back), like Details; and as the scan reads
  $effect(() => {
    void scan.ticks;
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
  const cellCount = (m: MapMod) => m.maps.reduce((n, f) => n + f.cells.length, 0);
  const key = ([x, y]: Cell) => `${x},${y}`;

  let grid = $state.raw<GridCell[]>([]);
  let gridFor = 0;
  $effect(() => {
    const ask = ++gridFor;
    api.mapGrid(data?.vanilla ?? [], data?.water ?? [], shown).then((g) => ask === gridFor && (grid = g)).catch((e) => say(String(e), true));
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

  function togglePreview(id: string) {
    const next = new Set(preview);
    next.has(id) ? next.delete(id) : next.add(id);
    preview = next;
  }

  // --- the list: search, filter, sort ---
  let search = $state("");
  type Filter = "all" | "on" | "installed" | "missing" | "conflict" | "over";
  let filter = $state<Filter>("all");
  type Sort = "name" | "cells" | "conflicts" | "state" | "subs";
  let sortBy = $state<Sort>("name");
  const FILTERS: [Filter, string][] = [["all", "All maps"], ["on", "Enabled"], ["installed", "Installed"], ["missing", "Not installed"], ["conflict", "Conflicting"], ["over", "Over vanilla"]];
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
      const hay = [nameOf(m), m.id, itemOf(m.source) ?? "", meta?.author ?? "", meta?.title ?? "", ...m.maps.map((f) => f.folder)].join("\n").toLowerCase();
      return words.every((w) => hay.includes(w));
    });
    const by: Record<Sort, (a: MapMod, b: MapMod) => number> = {
      name: () => 0,
      cells: (a, b) => cellCount(b) - cellCount(a),
      conflicts: (a, b) => (clashes.get(b.id)?.with.size ?? 0) - (clashes.get(a.id)?.with.size ?? 0),
      state: (a, b) => stateRank(a) - stateRank(b),
      subs: (a, b) => (metaOf(b)?.subscriptions ?? -1) - (metaOf(a)?.subscriptions ?? -1),
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
    if (await onSetMods(ids, true)) preview = new Set([...preview].filter((id) => !ids.includes(id)));
  }
  const previewListed = () => (preview = new Set([...preview, ...listed.filter((m) => !isOn(m.id)).map((m) => m.id)]));
  const clearListed = () => {
    const ids = new Set(listed.map((m) => m.id));
    preview = new Set([...preview].filter((id) => !ids.has(id)));
  };

  /** Preview the most maps (of those listed) that fit around the enabled ones: installed and popular ones win ties. */
  async function quickSelect() {
    try {
      const ranks = Object.fromEntries(listed.map((m) => [m.id, (mods.has(m.id) ? 2 ** 40 : 0) + (metaOf(m)?.subscriptions ?? 0)]));
      const pick = await api.mapQuickSelect(listed, all.filter((m) => isOn(m.id)).map((m) => m.id), ranks);
      preview = new Set(pick);
      say(pick.length ? `Previewing ${pick.length} ${pick.length === 1 ? "map" : "maps"}: the most of the listed maps that fit together around the enabled ones.` : "Every listed map shares cells with an enabled one.");
    } catch (e) {
      say(String(e), true);
    }
  }

  /** Show `m` instead of the shown maps it conflicts with (previewed ones are hidden, enabled ones turned off). */
  async function swap(m: MapMod) {
    const rivals = [...(clashes.get(m.id)?.with.keys() ?? [])];
    const off = rivals.filter(isOn);
    if (off.length && !(await onSetMods(off, false))) return;
    const next = new Set([...preview].filter((id) => !rivals.includes(id)));
    if (!isOn(m.id)) next.add(m.id);
    preview = next;
  }
  const cellList = (cells: Cell[]) => cells.slice(0, 3).map(([x, y]) => `${x},${y}`).join(" · ") + (cells.length > 3 ? ` +${cells.length - 3}` : "");

  // --- the Workshop source: every Map-tagged item, read from Steam's file lists ---
  async function startScan() {
    follow();
    scan.running = true;
    scan.error = "";
    scan.started = true;
    try {
      const p = await api.mapScan();
      scan.p = p;
      if (p.failed) say(`${p.failed} Workshop ${p.failed === 1 ? "item's" : "items'"} file lists couldn't be read; the next scan tries again.`, true);
    } catch (e) {
      scan.error = String(e);
    } finally {
      scan.running = false;
      load();
    }
  }
  $effect(() => {
    // once per session: later scans only read new and updated items
    if (prefs.mapsWorkshop && !untrack(() => scan.started)) startScan();
  });
  const workshopCount = $derived(data ? Object.keys(data.items).length : 0);
  const scanText = $derived.by(() => {
    const p = scan.p;
    if (scan.error) return /429|limiting/i.test(scan.error) ? "Steam is limiting requests right now. Wait a minute and refresh." : scan.error.replace(/^Error:\s*/, "");
    if (!scan.running || !p) return `${data?.checked.length ?? 0} not installed, from ${workshopCount} Workshop items read.`;
    if (p.phase === "listing") return `Listing Map-tagged Workshop items… ${p.done} of ${p.total || "?"}`;
    return `Reading file lists… ${p.done} of ${p.total}${p.failed ? ` (${p.failed} failed)` : ""}`;
  });

  // --- requirements: required mods (require=) and map folders (lots=) the shown maps need ---
  type Group = "enabled" | "preview" | "missing";
  type Need = { key: string; id: string; folder: string; by: string[]; group: Group };
  const GROUPS: [Group, string][] = [["enabled", "Needed by enabled maps"], ["preview", "Needed by previewed maps"], ["missing", "Needed by maps you haven't installed"]];
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
      for (const r of m.requires) add(r, r, "", m);
      for (const lot of m.maps.flatMap((f) => f.lots)) {
        if (data?.vanilla_folders.includes(lot) || m.maps.some((f) => f.folder === lot)) continue;
        const p = all.find((o) => o.maps.some((f) => f.folder === lot));
        add(p?.id ?? `map:${lot}`, p?.id ?? "", lot, m);
      }
    }
    return [...out.values()].sort((a, b) => (a.id || a.folder).localeCompare(b.id || b.folder));
  });
  /** A requirement's Workshop item: from the installed mod, else a Workshop map that is it. */
  const needItem = (n: Need) => (n.id ? itemOf(mods.get(n.id)?.source ?? "") ?? itemOf(all.find((m) => m.id === n.id)?.source ?? "") : undefined);
  /** Workshop lookups for missing requirements, by mod id. */
  let finding = $state<Record<string, string>>({});
  async function installNeed(n: Need) {
    const known = all.find((m) => m.id === n.id && !mods.has(m.id));
    const item = known && itemOf(known.source);
    if (item) return onInstallWorkshop(item);
    finding[n.id] = `Looking for ${n.id} on the Workshop…`;
    try {
      const list = await api.findRequirement(n.id, n.by.find((b) => mods.has(b)) ?? n.by[0]);
      if (list[0]?.confirmed && list[0]?.fits_build) {
        delete finding[n.id];
        await onInstallWorkshop(list[0].id);
      } else {
        finding[n.id] = list.length ? "No result is clearly it: pick one in the details of a map that needs it." : `Nothing on the Workshop matches ${n.id}.`;
      }
    } catch (e) {
      finding[n.id] = String(e);
    }
  }

  // --- Map= order (kept with the profile) and export ---
  let mapLine = $state<string[]>([]);
  const folders = $derived(shown.flatMap((m) => m.maps.map((f) => f.folder)));
  $effect(() => {
    const f = folders;
    api.mapLine(untrack(() => $state.snapshot(mapLine)), f).then((l) => (mapLine = l)).catch(() => {});
  });
  $effect(() => {
    if (mapLine.length) api.mapOrderSave($state.snapshot(mapLine)).catch(() => {});
  });

  async function exportMaps() {
    try {
      await navigator.clipboard.writeText(await api.mapsExport(shown, $state.snapshot(mapLine)));
      say(`Copied ${shown.length} ${shown.length === 1 ? "map" : "maps"}: Workshop links, mod IDs, requirements and the Mods=, WorkshopItems= and Map= lines.`);
    } catch (e) {
      say(String(e), true);
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
    if (presets[name] && !confirm(`Replace the preset "${name}"?`)) return;
    try {
      presets = await api.mapPresetSet(name, shown.map((m) => m.id));
      presetPick = name;
      presetName = "";
      say(`Saved the ${shown.length} shown maps as "${name}".`);
    } catch (err) {
      say(String(err), true);
    }
  }
  function loadPreset() {
    const ids = presets[presetPick] ?? [];
    const known = new Set(all.map((m) => m.id));
    preview = new Set(ids.filter((id) => known.has(id) && !isOn(id)));
    const gone = ids.filter((id) => !known.has(id));
    say(`Previewing "${presetPick}"${gone.length ? `; not found: ${gone.join(", ")} (install them, or turn on Workshop maps)` : ""}.`, gone.length > 0);
  }
  async function deletePreset() {
    try {
      presets = await api.mapPresetSet(presetPick, null);
      presetPick = "";
    } catch (err) {
      say(String(err), true);
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
      say(String(err), true);
    }
  }
  const byId = (id: string | null) => (id ? all.find((m) => m.id === id) : undefined);

  // --- check one Workshop map without installing it ---
  let checkText = $state("");
  let checking = $state(false);
  async function check(e: Event) {
    e.preventDefault();
    const id = checkText.trim().match(/(?:id=)?(\d{4,})/)?.[1];
    if (!id) return say("Paste a Workshop link or item ID.", true);
    checking = true;
    try {
      const { item } = await api.workshopInfo(id);
      if (!item) throw new Error(`There's no Workshop item ${id}.`);
      if (item.have) return say(`${item.title} is already installed: if it adds maps, they're in the list.`);
      let found;
      try {
        found = await api.mapCheck(id, false);
      } catch (err) {
        if (!confirm(`Steam didn't give the file list of "${item.title}" (${String(err).replace(/^Error:\s*/, "")}). Download it (${fmtSize(item.file_size)}) to read its cells instead? Nothing is installed, and the download is deleted afterwards.`)) return;
        found = await api.mapCheck(id, true);
      }
      checkText = "";
      preview = new Set([...preview, ...found.map((m) => m.id)]);
      say(found.length ? `Read ${item.title}: it's previewed on the grid and in the list as not installed.` : `${item.title} adds no maps for Build ${data?.build}.`);
      await load();
    } catch (err) {
      say(String(err), true);
    } finally {
      checking = false;
    }
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
  const KIND = { vanilla: "Vanilla area", water: "Vanilla river", map: "", over: "Over a vanilla area", conflict: "Conflict: these maps share this cell" };
</script>

<section class="maps blueprint" aria-label="Maps">
  <div class="inner">
    <div class="card world">
      <div class="head">
        <h2>World map</h2>
        <button class="quiet" onclick={() => (help = !help)} aria-expanded={help}><Icon name="info" size={14} />Help</button>
      </div>
      {#if help}
        <div class="help" role="dialog" aria-label="Maps help">
          <h3>Reading the grid</h3>
          <p>Each square is one cell of the Build {data?.build ?? ""} world (B42 cells are 256 tiles, B41 cells 300, so only one build's maps compare).
            <strong>Grey</strong> is the game's own map, <strong>teal</strong> its river; <strong>blue</strong> is a cell one map fills,
            <strong>gold</strong> a map built over a vanilla area (usually on purpose), <strong>red</strong> two or more maps fighting over a cell:
            only one of them loads there. Per-map colours give each shown map its own hue instead.</p>
          <h3>Preview or enable</h3>
          <p>Enabled maps are always shown. Tick a map, or click its row, to <em>preview</em> it: it's drawn and checked for conflicts, but nothing
            changes in your mod list until you enable it. Click cells to list the maps there; Shift+click adds more cells.</p>
          <h3>Workshop maps</h3>
          <p>With Workshop maps on, PZMM lists every Map-tagged Workshop item for this build, including ones you haven't installed. It reads each
            item's file list from Steam (no download, nothing installed) and keeps the cells, reading an item again only when it's updated.
            Preview them like installed maps, then Install the ones you want.</p>
          <button class="quiet" onclick={() => (help = false)}><Icon name="x" size={14} />Close</button>
        </div>
      {/if}
      <p class="lede">The grid shows enabled maps and the ones you preview. Scroll to zoom, drag to move, click a cell to list the maps there.</p>
      <div class="legend">
        {#if colours}
          {#each shown.slice(0, 12) as m (m.id)}<span><i style:background={swatch(m.id)}></i>{nameOf(m)}</span>{/each}
          {#if shown.length > 12}<span>+{shown.length - 12} more</span>{/if}
        {:else}
          <span><i style="background: var(--map-cell)"></i>A map</span>
          <span><i style="background: var(--map-over)"></i>Over vanilla ({overCells})</span>
        {/if}
        <span><i style="background: var(--danger)"></i>Conflict ({conflictCells})</span>
        {#if !hideVanilla}
          <span><i style="background: var(--muted); opacity: 0.45"></i>Vanilla</span>
          {#if data?.water.length}<span><i style="background: var(--map-water); opacity: 0.6"></i>River</span>{/if}
        {/if}
      </div>
      <div class="row layers">
        <label class="check"><input type="checkbox" bind:checked={hideVanilla} />Hide vanilla</label>
        <label class="check"><input type="checkbox" bind:checked={colours} />Per-map colours</label>
        <span class="zoom">
          <button class="quiet" onclick={() => zoom(1.25)} aria-label="Zoom in"><Icon name="plus" size={14} />Zoom in</button>
          <button class="quiet" onclick={() => zoom(0.8)}>Zoom out</button>
          <button class="quiet" onclick={fit}>Fit</button>
        </span>
      </div>
      <div class="canvas" bind:clientWidth={width}>
        <canvas bind:this={canvas} style:height="{HEIGHT}px"
          aria-label="Cell grid: {shown.length} maps shown, {conflictCells} cells in conflict, {overCells} over vanilla"
          onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={() => (hover = null)} onwheel={wheel}></canvas>
        <span bind:this={probe} hidden></span>
        {#if hover}
          <div class="tip" style:left="{Math.min(hover.px + 14, width - 230)}px" style:top="{Math.min(hover.py + 14, HEIGHT - 140)}px">
            <strong>Cell {hover.x}, {hover.y}</strong>
            {#if hovered}
              {#if KIND[hovered.kind]}<span class="kind {hovered.kind}">{KIND[hovered.kind]}</span>{/if}
              {#each hovered.maps as id}<span>{names.get(id) ?? id}</span>{/each}
            {:else}
              <span class="hint">Empty</span>
            {/if}
          </div>
        {/if}
        {#if !data}<p class="loading">Reading the map files…</p>{/if}
      </div>
      {#if data && !data.vanilla}
        <p class="hint">Vanilla areas aren't shown: the game install wasn't found, or it isn't Build {data.build}. Check the folders in Settings.</p>
      {/if}
    </div>

    <div class="card list">
      <div class="head">
        <h2>Map mods</h2>
        <button class="quiet icon" onclick={onUndo} disabled={busy || !canUndo} title="Undo the last enable or disable (Ctrl+Z)" aria-label="Undo"><Icon name="undo" size={16} /></button>
      </div>
      <div class="source">
        <label class="check"><input type="checkbox" bind:checked={prefs.mapsWorkshop} />Workshop maps you haven't installed</label>
        {#if prefs.mapsWorkshop}
          <span class="hint" class:bad={!!scan.error} role="status">{scanText}</span>
          {#if scan.running}
            {#if scan.p?.total}<progress max={scan.p.total} value={scan.p.done}></progress>{/if}
            <button class="quiet" onclick={() => api.mapScanStop()}><Icon name="x" size={14} />Stop</button>
          {:else}
            <button class="quiet" onclick={startScan} title="List the Map-tagged items again and read new and updated ones"><Icon name="refresh" size={14} />Refresh</button>
          {/if}
        {/if}
      </div>
      <div class="row">
        <input type="search" bind:value={search} placeholder="Search name, mod ID, Workshop ID, folder, author" aria-label="Search maps" />
        <select bind:value={filter} aria-label="Show maps" title="Show maps">
          {#each FILTERS as [v, label]}<option value={v}>{label}</option>{/each}
        </select>
        <select bind:value={sortBy} aria-label="Sort maps" title="Sort maps">
          <option value="name">Name</option>
          <option value="cells">Most cells</option>
          <option value="conflicts">Most conflicts</option>
          <option value="state">Enabled first</option>
          <option value="subs">Most subscribers</option>
        </select>
      </div>
      {#if pickedCells.length}
        <p class="filter">
          <span class="chip">Cells <span class="mono">{pickedCells.map(([x, y]) => `${x},${y}`).join(" · ")}</span></span>
          {listed.length} {listed.length === 1 ? "map" : "maps"}
          <button class="quiet" onclick={() => (pickedCells = [])}><Icon name="x" size={14} />Clear cells</button>
        </p>
      {/if}
      <div class="row bulk">
        <button onclick={quickSelect} disabled={!listed.length} title="Preview the most listed maps that share no cell with each other or the enabled ones (installed and popular maps win ties)"><Icon name="wand" size={14} />Most maps that fit</button>
        <button onclick={previewListed} disabled={!listed.length}>Preview listed</button>
        {#if preview.size}
          <button onclick={clearListed}>Unpreview listed</button>
          <button onclick={() => (preview = new Set())} title="Clear the preview: the grid shows enabled maps only">Enabled only</button>
        {/if}
        {#if previewOff.length}<button onclick={enablePreviewed} disabled={busy}><Icon name="check" size={14} />Enable {previewOff.length} previewed</button>{/if}
        {#if listedOn.length}<button onclick={() => onSetMods(listedOn, false)} disabled={busy}><Icon name="x" size={14} />Disable {listedOn.length} listed</button>{/if}
      </div>
      {#if data && !all.length}
        <p class="hint">No installed mod adds maps for Build {data.build}. Turn on Workshop maps above, or check one below before installing it.</p>
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
          <li class:shown={isShown} onclick={(e) => !on && !(e.target as HTMLElement).closest("button, input, a") && togglePreview(m.id)}>
            <input type="checkbox" checked={isShown} disabled={on} onchange={() => togglePreview(m.id)}
              aria-label="Preview {nameOf(m)} on the grid" title={on ? "Enabled maps are always shown" : "Show on the grid without enabling"} />
            <span class="name">
              <span><strong>{nameOf(m)}</strong>{#if on}<span class="on">Enabled</span>{:else if !installed}<span class="off">Not installed</span>{/if}</span>
              <span class="hint">{cellCount(m)} {cellCount(m) === 1 ? "cell" : "cells"} · {m.maps.map((f) => f.folder).join("; ")}{meta?.author ? ` · by ${meta.author}` : ""}{meta?.subscriptions ? ` · ${compact.format(meta.subscriptions)} subscribers` : ""}{!installed && meta?.size ? ` · ${fmtSize(meta.size)}` : ""}</span>
              {#each [...(clash?.with ?? [])].slice(0, 3) as [rival, cells] (rival)}
                <span class="bad">Conflicts with {names.get(rival) ?? rival} on {cells.length === 1 ? "cell" : "cells"} <span class="mono">{cellList(cells)}</span>
                  <button class="link" onclick={() => swap(m)} disabled={busy} title="Show this map instead: hide the previewed maps it conflicts with, turn off the enabled ones">Swap</button></span>
              {/each}
              {#if (clash?.with.size ?? 0) > 3}<span class="bad">…and {clash!.with.size - 3} more</span>{/if}
              {#if clash?.over}<span class="over">{clash.over} {clash.over === 1 ? "cell" : "cells"} over vanilla</span>{/if}
            </span>
            <span class="btns">
              {#if item}<button class="quiet" onclick={() => api.openUrl(workshopUrl(item))} title="Open its Workshop page"><Icon name="steam" size={14} />Workshop</button>{/if}
              {#if installed}
                <button disabled={busy} onclick={() => onSetMods([m.id], !on)} title={on ? "" : "Also turns on the mods it needs"}>{on ? "Disable" : "Enable"}</button>
              {:else if item}
                <button disabled={busy} onclick={() => onInstallWorkshop(item)} title="Install it with the items it requires"><Icon name="download" size={14} />Install</button>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
      {#if listed.length > limit}
        <button class="quiet more" onclick={() => (limit += 150)}>Show {Math.min(150, listed.length - limit)} more of {listed.length}</button>
      {/if}
    </div>

    <div class="card">
      <h2>Requirements</h2>
      <p class="lede">What the shown maps need: required mods (<span class="mono">require=</span>) and other maps they build on (<span class="mono">lots=</span>).</p>
      {#if !needs.length}
        <p class="hint">{shown.length ? "The shown maps need nothing else." : "Show or enable a map to see what it needs."}</p>
      {/if}
      {#each GROUPS as [g, title] (g)}
        {@const group = needs.filter((n) => n.group === g)}
        {#if group.length}
          <h3>{title}</h3>
          <ul class="needs">
            {#each group as n (n.key)}
              {@const m = n.id ? mods.get(n.id) : undefined}
              {@const item = needItem(n)}
              <li>
                <span class="name">
                  <span class="mono">{n.id || n.folder}</span>
                  <span class="state {m ? (m.on ? 'is-on' : 'is-off') : 'is-missing'}">{m ? (m.on ? "On" : "Off") : "Not installed"}</span>
                  <span class="hint">{n.folder && n.id ? `map ${n.folder} · ` : n.folder ? "map folder · " : ""}needed by {n.by.map((id) => names.get(id) ?? id).join(", ")}</span>
                  {#if finding[n.id]}<span class="hint">{finding[n.id]}</span>{/if}
                </span>
                <span class="btns">
                  {#if item}<button class="quiet" onclick={() => api.openUrl(workshopUrl(item))} title="Open its Workshop page"><Icon name="steam" size={14} />Workshop</button>{/if}
                  {#if m && !m.on}
                    <button disabled={busy} onclick={() => onSetMods([n.id], true)}>Enable</button>
                  {:else if !m && n.id}
                    <button disabled={busy} onclick={() => installNeed(n)} title="Find it on the Steam Workshop and install it"><Icon name="download" size={14} />Install</button>
                    {#if finding[n.id]?.startsWith("No result") && n.by.some((b) => mods.has(b))}
                      <button class="quiet" onclick={() => onShowMod(n.by.find((b) => mods.has(b))!)}>Pick in details</button>
                    {/if}
                  {/if}
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      {/each}
    </div>

    <div class="card">
      <h2>Map order and export</h2>
      <p class="lede">
        Copy the shown maps for a friend or a server: their Workshop links and mod IDs, what they need, and the
        <span class="mono">Mods=</span>, <span class="mono">WorkshopItems=</span> and <span class="mono">Map=</span> lines.
      </p>
      {#if mapLine.length}
        <h3>Map order</h3>
        <p class="hint">The game loads maps top to bottom: mod maps go before <span class="mono">Muldraugh, KY</span>. Drag or Alt+↑/↓ to arrange; it's kept with your profile and used by the export and the server card.</p>
        <MapOrder bind:maps={mapLine} />
      {/if}
      <div class="row">
        <button class="primary" onclick={exportMaps} disabled={!shown.length}><Icon name="copy" size={14} />Copy {shown.length} {shown.length === 1 ? "map" : "maps"} to the clipboard</button>
      </div>
      <h3>Presets</h3>
      <p class="hint">Save the shown maps under a name, and preview them again later.</p>
      <div class="row">
        {#if Object.keys(presets).length}
          <select bind:value={presetPick} aria-label="Preset">
            <option value="">Pick a preset</option>
            {#each Object.entries(presets) as [name, ids] (name)}<option value={name}>{name} ({ids.length})</option>{/each}
          </select>
          <button onclick={loadPreset} disabled={!presetPick}>Preview</button>
          <button class="danger" onclick={deletePreset} disabled={!presetPick}><Icon name="trash" size={14} />Delete</button>
        {/if}
      </div>
      <form class="row" onsubmit={savePreset}>
        <input type="text" bind:value={presetName} placeholder="Preset name" aria-label="Preset name" />
        <button type="submit" disabled={!presetName.trim() || !shown.length}><Icon name="plus" size={14} />Save {shown.length} shown</button>
      </form>
    </div>

    <div class="card">
      <h2>Compare with a server</h2>
      <p class="lede">The maps a server config loads (<span class="mono">Map=</span>) against the ones shown here.</p>
      {#if servers.length}
        <div class="row">
          <select bind:value={server} aria-label="Server config">
            {#each servers as s (s)}<option value={s}>{s}.ini</option>{/each}
          </select>
          <button onclick={compareServer} disabled={!server}><Icon name="server" size={14} />Compare</button>
        </div>
      {:else}
        <p class="hint">No server configs in Zomboid/Server yet.</p>
      {/if}
      {#if compared}
        {#if !compared.only_server.length && !compared.only_you.length}
          <p class="hint">Same maps as {server}.ini.</p>
        {/if}
        {#if compared.only_server.length}
          <h3>On the server, not shown here</h3>
          <ul class="needs">
            {#each compared.only_server as s (s.folder)}
              {@const m = byId(s.id)}
              {@const row = s.id ? mods.get(s.id) : undefined}
              <li>
                <span class="name"><span class="mono">{s.folder}</span>
                  <span class="hint">{m ? nameOf(m) : s.id ? s.id : "no known Workshop map adds it"}{row ? (row.on ? " · on" : " · installed") : s.item ? " · not installed" : ""}</span></span>
                <span class="btns">
                  {#if s.id && !row?.on && byId(s.id)}<button class="quiet" onclick={() => s.id && !preview.has(s.id) && togglePreview(s.id)}>Preview</button>{/if}
                  {#if row && !row.on}
                    <button disabled={busy} onclick={() => onSetMods([s.id!], true)}>Enable</button>
                  {:else if !row && s.item}
                    <button disabled={busy} onclick={() => onInstallWorkshop(s.item!)}><Icon name="download" size={14} />Install</button>
                  {/if}
                </span>
              </li>
            {/each}
          </ul>
        {/if}
        {#if compared.only_you.length}
          <h3>Shown here, not on the server</h3>
          <p class="mono hint">{compared.only_you.join("; ")}</p>
        {/if}
      {/if}
    </div>

    <div class="card wide">
      <h2>Check a Workshop map before installing</h2>
      <p class="lede">See where one map sits and what it conflicts with first. PZMM reads the item's file list from Steam (nothing is downloaded or installed) and keeps only its cells.</p>
      <form class="row" onsubmit={check}>
        <input type="text" bind:value={checkText} placeholder="Workshop link or ID" spellcheck="false" aria-label="Workshop link or ID" />
        <button type="submit" disabled={checking || !checkText.trim()}><Icon name="search" size={14} />{checking ? "Checking…" : "Check"}</button>
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
  .over {
    color: var(--map-over);
    font-size: 12px;
  }
</style>
