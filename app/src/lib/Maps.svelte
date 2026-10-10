<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, type Cell, type GridCell, type MapMod, type MapsView, type ModRow } from "./api";
  import Icon from "./Icon.svelte";
  import MapOrder from "./MapOrder.svelte";
  import { fmtSize, workshopUrl } from "./format";

  let { busy, mods, say, onSetMods, onInstallWorkshop, onShowMod }: {
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
  } = $props();

  let data = $state.raw<MapsView | null>(null);
  /** Maps shown on the grid without being enabled. */
  let preview = $state<Set<string>>(new Set());
  /** A clicked cell: the list shows only the maps that cover it. */
  let picked = $state<Cell | null>(null);

  const load = () => api.mapsView().then((v) => (data = v)).catch((e) => say(String(e), true));
  // after installs, removals and toggles (busy flips back), like Details
  $effect(() => {
    if (!busy) load();
  });

  const all = $derived(data ? [...data.mods, ...data.checked] : []);
  const nameOf = (m: MapMod) => m.name || m.id;
  const names = $derived(new Map(all.map((m) => [m.id, nameOf(m)])));
  const isOn = (id: string) => !!mods.get(id)?.on;
  const itemOf = (source: string) => source.match(/^(?:steam|workshop):(\d+)$/)?.[1];
  const shown = $derived(all.filter((m) => isOn(m.id) || preview.has(m.id)));
  const cellCount = (m: MapMod) => m.maps.reduce((n, f) => n + f.cells.length, 0);
  const covers = (m: MapMod, [x, y]: Cell) => m.maps.some((f) => f.cells.some((c) => c[0] === x && c[1] === y));
  const listed = $derived(picked ? all.filter((m) => covers(m, picked!)) : all);

  let grid = $state.raw<GridCell[]>([]);
  let gridFor = 0;
  $effect(() => {
    const ask = ++gridFor;
    api.mapGrid(data?.vanilla ?? [], shown).then((g) => ask === gridFor && (grid = g)).catch((e) => say(String(e), true));
  });
  const byCell = $derived(new Map(grid.map((c) => [`${c.x},${c.y}`, c])));
  /** Per shown map: the shown maps it shares cells with, and its cells over vanilla areas. */
  const clashes = $derived.by(() => {
    const out = new Map<string, { with: Set<string>; over: number }>();
    for (const c of grid)
      for (const id of c.maps) {
        if (!out.has(id)) out.set(id, { with: new Set(), over: 0 });
        const e = out.get(id)!;
        if (c.kind === "conflict") c.maps.forEach((o) => o !== id && e.with.add(o));
        if (c.kind === "over") e.over++;
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

  async function selectNonConflicting() {
    try {
      const add = await api.mapNonConflicting(all, shown.map((m) => m.id));
      preview = new Set([...preview, ...add]);
      say(add.length ? `Showing ${add.length} more ${add.length === 1 ? "map" : "maps"} that conflict with nothing shown.` : "Every other map shares cells with the shown ones.");
    } catch (e) {
      say(String(e), true);
    }
  }

  const previewOff = $derived(shown.filter((m) => preview.has(m.id) && mods.has(m.id) && !isOn(m.id)).map((m) => m.id));
  async function enablePreviewed() {
    const ids = previewOff;
    if (await onSetMods(ids, true)) preview = new Set([...preview].filter((id) => !ids.includes(id)));
  }

  // --- requirements: required mods (require=) and map folders (lots=) the shown maps need ---
  type Need = { key: string; id: string; folder: string; by: string[] };
  const needs = $derived.by(() => {
    const out = new Map<string, Need>();
    const add = (key: string, id: string, folder: string, by: string) => {
      const n = out.get(key) ?? { key, id, folder, by: [] };
      if (!n.by.includes(by)) n.by.push(by);
      out.set(key, n);
    };
    for (const m of shown) {
      for (const r of m.requires) add(r, r, "", m.id);
      for (const lot of m.maps.flatMap((f) => f.lots)) {
        if (data?.vanilla_folders.includes(lot) || m.maps.some((f) => f.folder === lot)) continue;
        const p = all.find((o) => o.maps.some((f) => f.folder === lot));
        add(p?.id ?? `map:${lot}`, p?.id ?? "", lot, m.id);
      }
    }
    return [...out.values()];
  });
  /** Workshop lookups for missing requirements, by mod id. */
  let finding = $state<Record<string, string>>({});
  async function installNeed(n: Need) {
    const checked = all.find((m) => m.id === n.id && !mods.has(m.id));
    const item = checked && itemOf(checked.source);
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

  // --- Map= order and export ---
  let mapLine = $state<string[]>([]);
  const folders = $derived(shown.flatMap((m) => m.maps.map((f) => f.folder)));
  $effect(() => {
    const f = folders;
    api.mapLine(untrack(() => $state.snapshot(mapLine)), f).then((l) => (mapLine = l)).catch(() => {});
  });

  async function exportMaps() {
    try {
      await navigator.clipboard.writeText(await api.mapsExport(shown, $state.snapshot(mapLine)));
      say(`Copied ${shown.length} ${shown.length === 1 ? "map" : "maps"}: Workshop links, mod IDs, requirements and the Mods=, WorkshopItems= and Map= lines.`);
    } catch (e) {
      say(String(e), true);
    }
  }

  // --- check a Workshop map without installing it ---
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
      if (!confirm(`Download "${item.title}" (${fmtSize(item.file_size)}) to read its map cells? Nothing is installed, and the download is deleted afterwards.`)) return;
      const found = await api.mapCheck(id);
      checkText = "";
      preview = new Set([...preview, ...found.map((m) => m.id)]);
      say(found.length ? `Read ${item.title}: it's shown on the grid and in the list as not installed.` : `${item.title} adds no maps for Build ${data?.build}.`);
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
      probe!.style.color = `var(${token})`;
      return getComputedStyle(probe!).color;
    };
    const fill = { vanilla: color("--muted"), map: color("--map-cell"), over: color("--map-over"), conflict: color("--danger") };
    g.fillStyle = color("--bg");
    g.fillRect(0, 0, width, HEIGHT);
    const s = view.scale;
    const gap = s >= 10 ? 1 : 0;
    for (const c of grid) {
      g.globalAlpha = c.kind === "vanilla" ? 0.3 : 0.9;
      g.fillStyle = fill[c.kind];
      g.fillRect(view.x + c.x * s, view.y + c.y * s, s - gap, s - gap);
    }
    g.globalAlpha = 1;
    g.lineWidth = 2;
    const ring = (cell: Cell | null, token: string) => {
      if (!cell) return;
      g.strokeStyle = color(token);
      g.strokeRect(view.x + cell[0] * s - 1, view.y + cell[1] * s - 1, s + 1, s + 1);
    };
    ring(picked, "--accent-text");
    ring(hover && [hover.x, hover.y], "--text");
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
      picked = picked && picked[0] === x && picked[1] === y ? null : [x, y];
    }
    drag = null;
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const { px, py } = at(e);
    zoom(e.deltaY < 0 ? 1.25 : 0.8, px, py);
  }
  const hovered = $derived(hover ? byCell.get(`${hover.x},${hover.y}`) : undefined);
  const KIND = { vanilla: "Vanilla area", map: "", over: "Over a vanilla area", conflict: "Conflict: these maps share this cell" };
</script>

<section class="maps blueprint" aria-label="Maps">
  <div class="inner">
    <div class="card world">
      <h2>World map</h2>
      <p class="lede">
        Each square is one cell of the Build {data?.build ?? ""} world, read from the map files. The grid shows enabled maps
        and the ones you tick to preview. Scroll to zoom, drag to move, click a cell to list the maps there.
      </p>
      <div class="legend">
        <span><i style="background: var(--muted); opacity: 0.45"></i>Vanilla</span>
        <span><i style="background: var(--map-cell)"></i>A map</span>
        <span><i style="background: var(--map-over)"></i>Over vanilla ({overCells})</span>
        <span><i style="background: var(--danger)"></i>Conflict ({conflictCells})</span>
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
      <h2>Map mods</h2>
      <div class="row">
        <button onclick={selectNonConflicting} disabled={!all.length} title="Tick every map that shares no cell with the shown ones (covering vanilla is fine)"><Icon name="wand" size={14} />Select maps that don't conflict</button>
        {#if preview.size}<button class="quiet" onclick={() => (preview = new Set())}><Icon name="x" size={14} />Clear preview</button>{/if}
        {#if previewOff.length}<button onclick={enablePreviewed} disabled={busy}><Icon name="check" size={14} />Enable {previewOff.length} previewed</button>{/if}
      </div>
      {#if picked}
        <p class="filter">Maps in cell <span class="mono">{picked[0]}, {picked[1]}</span>: {listed.length}
          <button class="quiet" onclick={() => (picked = null)}><Icon name="x" size={14} />Show all</button></p>
      {/if}
      {#if data && !all.length}
        <p class="hint">No installed mod adds maps for Build {data.build}. Subscribe to map mods on the Workshop, or check one below before installing it.</p>
      {/if}
      <ul class="mods">
        {#each listed as m (m.id)}
          {@const installed = mods.has(m.id)}
          {@const on = isOn(m.id)}
          {@const item = itemOf(m.source)}
          {@const clash = clashes.get(m.id)}
          <li>
            <input type="checkbox" checked={on || preview.has(m.id)} disabled={on} onchange={() => togglePreview(m.id)}
              aria-label="Show {nameOf(m)} on the grid" title={on ? "Enabled maps are always shown" : "Show on the grid without enabling"} />
            <span class="name">
              <span><strong>{nameOf(m)}</strong>{#if on}<span class="on">Enabled</span>{/if}</span>
              <span class="hint">{cellCount(m)} {cellCount(m) === 1 ? "cell" : "cells"} · {m.maps.map((f) => f.folder).join("; ")}{installed ? "" : " · not installed"}</span>
              {#if clash?.with.size}<span class="bad">Conflicts with {[...clash.with].map((id) => names.get(id) ?? id).join(", ")}</span>{/if}
              {#if clash?.over}<span class="over">{clash.over} {clash.over === 1 ? "cell" : "cells"} over vanilla</span>{/if}
            </span>
            <span class="btns">
              {#if item}<button class="quiet" onclick={() => api.openUrl(workshopUrl(item))} title="Open its Workshop page"><Icon name="steam" size={14} />Workshop</button>{/if}
              {#if installed}
                <button disabled={busy} onclick={() => onSetMods([m.id], !on)} title={on ? "" : "Also turns on the mods it needs"}>{on ? "Disable" : "Enable"}</button>
              {:else if item}
                <button disabled={busy} onclick={() => onInstallWorkshop(item)}><Icon name="download" size={14} />Install</button>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
    </div>

    <div class="card">
      <h2>Requirements</h2>
      <p class="lede">What the shown maps need: required mods (<span class="mono">require=</span>) and other maps they build on (<span class="mono">lots=</span>).</p>
      {#if !needs.length}
        <p class="hint">{shown.length ? "The shown maps need nothing else." : "Show or enable a map to see what it needs."}</p>
      {:else}
        <ul class="needs">
          {#each needs as n (n.key)}
            {@const m = n.id ? mods.get(n.id) : undefined}
            <li>
              <span class="name">
                <span class="mono">{n.id || n.folder}</span>
                <span class={m ? (m.on ? "ok" : "bad") : "bad"}>{m ? (m.on ? "on" : "off") : "not installed"}</span>
                <span class="hint">{n.folder && n.id ? `map ${n.folder} · ` : n.folder ? "map folder · " : ""}needed by {n.by.map((id) => names.get(id) ?? id).join(", ")}</span>
                {#if finding[n.id]}<span class="hint">{finding[n.id]}</span>{/if}
              </span>
              <span class="btns">
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
    </div>

    <div class="card">
      <h2>Map order and export</h2>
      <p class="lede">
        Copy the shown maps for a friend or a server: their Workshop links and mod IDs, what they need, and the
        <span class="mono">Mods=</span>, <span class="mono">WorkshopItems=</span> and <span class="mono">Map=</span> lines.
      </p>
      {#if mapLine.length}
        <h3>Map order</h3>
        <p class="hint">The game loads maps top to bottom: mod maps go before <span class="mono">Muldraugh, KY</span>. Drag or Alt+↑/↓ to arrange; the export uses this order.</p>
        <MapOrder bind:maps={mapLine} />
      {/if}
      <div class="row">
        <button class="primary" onclick={exportMaps} disabled={!shown.length}><Icon name="copy" size={14} />Copy {shown.length} {shown.length === 1 ? "map" : "maps"} to the clipboard</button>
      </div>
    </div>

    <div class="card wide">
      <h2>Check a Workshop map before installing</h2>
      <p class="lede">See where a map sits and what it conflicts with first. PZMM downloads it, reads its cells, keeps only the cell list and deletes the download. Maps can be large: you'll see the size before it starts.</p>
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
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
    padding: 20px 24px 22px;
    min-width: 0;
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
  }
  .row input {
    flex: 0 1 360px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
    margin-bottom: 8px;
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
  .filter {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 0 0;
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
  .mods li:hover,
  .needs li:hover {
    background: color-mix(in oklab, var(--accent) 16%, var(--panel));
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
  .ok {
    color: var(--muted);
  }
  .bad {
    color: var(--danger);
    font-size: 12px;
  }
  .over {
    color: var(--map-over);
    font-size: 12px;
  }
</style>
