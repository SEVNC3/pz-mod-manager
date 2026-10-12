<script lang="ts" module>
  import { api, type MapBackground, type WorldMap } from "./api";

  // --- the world under the grid: each map's own world map data (worldmap.xml.bin / worldmap.xml) ---
  type World = { water: Path2D; roads: Path2D; buildings: Path2D; lines: Path2D; holed: [keyof WorldMap, Path2D][] };
  /** Read once per map ("" for the game's own) and kept for every grid; null: none (yet). */
  const worlds = new Map<string, World | null>();
  const arrived = $state({ n: 0 });
  function toWorld(w: WorldMap): World | null {
    const ring = (p: Path2D, r: number[]) => {
      p.moveTo(r[0], r[1]);
      for (let i = 2; i < r.length; i += 2) p.lineTo(r[i], r[i + 1]);
    };
    const out: World = { water: new Path2D(), roads: new Path2D(), buildings: new Path2D(), lines: new Path2D(), holed: [] };
    for (const layer of ["water", "roads", "buildings"] as const) {
      for (const rings of w[layer]) {
        // holes (islands, courtyards) need their own even-odd fill
        const p = rings.length > 1 ? new Path2D() : out[layer];
        for (const r of rings) {
          ring(p, r);
          p.closePath();
        }
        if (rings.length > 1) out.holed.push([layer, p]);
      }
    }
    w.road_lines.forEach((r) => ring(out.lines, r));
    return w.water.length || w.roads.length || w.buildings.length || w.road_lines.length ? out : null;
  }
  function readWorld(id: string) {
    if (worlds.has(id)) return;
    worlds.set(id, null);
    api.mapWorld(id).then((w) => {
      worlds.set(id, toWorld(w));
      arrived.n++;
    }).catch(() => {});
  }

  // --- the player's own picture under the grid, per build: drawn instead of the world maps ---
  /** Each build's picture: where it lies, and the image (null: the file is missing or unreadable). */
  export const backgrounds: Record<number, { bg: MapBackground; img: ImageBitmap | null } | null> = $state({});
  /** A picture's file, decoded (null: missing or not an image). */
  export const loadImage = (path: string) => api.mapBackgroundImage(path).then((b) => createImageBitmap(new Blob([b]))).catch(() => null);
  /** Read `build`'s saved picture, once. */
  export function readBackground(build: number) {
    if (build in backgrounds) return;
    backgrounds[build] = null;
    api.mapBackground(build).then(async (bg) => bg && (backgrounds[build] = { bg, img: await loadImage(bg.path) })).catch(() => {});
  }
</script>

<script lang="ts">
  import { onMount, untrack, type Snippet } from "svelte";
  import type { Cell, GridCell } from "./api";

  // The world's cell grid on a canvas (thousands of cells), coloured from the theme's tokens; it
  // fills its box. Drag (or arrow keys) to pan, wheel (or + and -) to zoom, click a cell (`onPick`) or a dot (`onDot`).
  let { grid, paint, worlds: worldIds = [], backdrop, tiles = 256, rings = [], dots = [], label, area, ready = true, onPick, onDot, onPoint, tip, children, width = $bindable(0), height = $bindable(0) }: {
    grid: GridCell[];
    /** Maps whose world map is drawn under the cells ("" for the game's own), bottom first: each
     * covers what's below it in the cells it fills (the grid's `maps`), as a map listed earlier in
     * Map= does in the game. */
    worlds?: string[];
    /** The build whose own picture (`backgrounds`), if it has one, is drawn instead of the worlds. */
    backdrop?: number;
    /** Tiles per cell: 300 in B41, 256 in B42. */
    tiles?: number;
    /** A cell's fill (a colour or a --token) and opacity; null leaves it out. */
    paint: (c: GridCell) => [string, number] | null;
    /** Cells outlined in a colour (or --token). */
    rings?: [Cell, string][];
    /** Points in cells (fractions inside one), with a colour (or --token) and a name drawn beside. */
    dots?: { x: number; y: number; colour: string; name: string }[];
    label: string;
    /** The part of the box Fit and the zoom buttons use (all of it by default). */
    area?: () => { x: number; y: number; w: number; h: number };
    /** Fit to the grid the first time this is true and there's a grid. */
    ready?: boolean;
    onPick?: (cell: Cell, shift: boolean) => void;
    /** A dot clicked (its index), and where on screen. */
    onDot?: (i: number, x: number, y: number) => void;
    /** Pointing at spots (lining a picture up): a click, or Enter for the cross in the middle,
     * gives the spot in cells (fractions inside one) instead of picking. */
    onPoint?: (x: number, y: number) => void;
    /** What the hovered cell holds. */
    tip?: Snippet<[Cell]>;
    /** More things over the map. */
    children?: Snippet;
    width?: number;
    height?: number;
  } = $props();

  let canvas = $state<HTMLCanvasElement>();
  let probe = $state<HTMLSpanElement>();
  /** Pixels per cell and the offset of cell 0,0. */
  let view = $state({ scale: 0, x: 0, y: 0 });
  let hover = $state<{ x: number; y: number; px: number; py: number } | null>(null);
  let theme = $state(0);

  /** What Fit shows: the cells and the dots. */
  const bounds = $derived.by(() => {
    if (!grid.length && !dots.length) return null;
    const xs = [...grid.map((c) => c.x), ...dots.map((d) => Math.floor(d.x))];
    const ys = [...grid.map((c) => c.y), ...dots.map((d) => Math.floor(d.y))];
    return { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) };
  });

  const whole = () => ({ x: 0, y: 0, w: width, h: height });
  const mid = () => {
    const c = (area ?? whole)();
    return [c.x + c.w / 2, c.y + c.h / 2];
  };
  export function fit() {
    if (!bounds || !width) return;
    const { x, y, w, h } = (area ?? whole)();
    const [cols, rows] = [bounds.x1 - bounds.x0 + 1, bounds.y1 - bounds.y0 + 1];
    const scale = Math.min(w / (cols + 2), h / (rows + 2));
    view = { scale, x: x + (w - cols * scale) / 2 - bounds.x0 * scale, y: y + (h - rows * scale) / 2 - bounds.y0 * scale };
  }
  let fitted = false;
  $effect(() => {
    if (!fitted && bounds && width && height && ready) {
      fitted = true;
      untrack(fit);
    }
  });

  export function zoom(k: number, [px, py] = mid()) {
    const scale = Math.min(256, Math.max(1, view.scale * k));
    view = { scale, x: px - ((px - view.x) * scale) / view.scale, y: py - ((py - view.y) * scale) / view.scale };
  }

  /** The canvas's top on screen. */
  export const top = () => canvas?.getBoundingClientRect().top ?? 0;

  const dotR =(s: number) => Math.max(4, Math.min(8, s / 6));

  $effect(() => worldIds.forEach(readWorld));
  $effect(() => void (backdrop !== undefined && readBackground(backdrop)));
  let fontFamily = "";
  /** The world layer, drawn again only when the view, theme, maps or grid change (not on hover). */
  let worldLayer: { key: string; grid: GridCell[]; canvas: HTMLCanvasElement } | null = null;
  /** The player's picture, scaled: drawn again only when the picture, its placement or the view change. */
  let picLayer: { key: string; img: ImageBitmap; canvas: HTMLCanvasElement } | null = null;

  $effect(() => {
    void theme;
    if (!canvas || !probe || !width || !view.scale) return;
    const dpr = devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    const g = canvas.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    // resolved through an element, so tokens, color-mix() and relative colours come back as rgb()
    const known = new Map<string, string>();
    const color = (c: string) => {
      let v = known.get(c);
      if (v === undefined) {
        probe!.style.color = c.startsWith("--") ? `var(${c})` : c;
        known.set(c, (v = getComputedStyle(probe!).color));
      }
      return v;
    };
    g.fillStyle = color("--bg");
    g.fillRect(0, 0, width, height);
    const s = view.scale;
    void arrived.n;
    const lands = worldIds.flatMap((id) => {
      const w = worlds.get(id);
      return w ? [[id, w] as const] : [];
    });
    const pic = backdrop === undefined ? null : backgrounds[backdrop];
    if (pic?.img) {
      const key = [width, height, dpr, s, view.x, view.y, tiles, pic.bg.scale, pic.bg.offset].join();
      if (picLayer?.key !== key || picLayer.img !== pic.img) {
        const c = picLayer?.canvas ?? document.createElement("canvas");
        [c.width, c.height] = [canvas.width, canvas.height];
        const k = s / tiles;
        const b = c.getContext("2d")!;
        b.setTransform(dpr * k * pic.bg.scale[0], 0, 0, dpr * k * pic.bg.scale[1], dpr * (view.x + pic.bg.offset[0] * k), dpr * (view.y + pic.bg.offset[1] * k));
        b.drawImage(pic.img, 0, 0);
        picLayer = { key, img: pic.img, canvas: c };
      }
      g.drawImage(picLayer.canvas, 0, 0, width, height);
    } else if (lands.length) {
      const key = [width, height, dpr, s, view.x, view.y, theme, tiles, worldIds, arrived.n].join();
      if (worldLayer?.key !== key || worldLayer.grid !== grid) {
        const c = worldLayer?.canvas ?? document.createElement("canvas");
        [c.width, c.height] = [canvas.width, canvas.height];
        const w = c.getContext("2d")!;
        const k = s / tiles; // world tiles to pixels
        w.setTransform(dpr * k, 0, 0, dpr * k, dpr * view.x, dpr * view.y);
        const fill = { water: color("--map-water"), roads: color("--map-road"), buildings: color("--map-building") };
        // each map's cells, in world tiles: it replaces what's under it there
        const cellsOf = new Map<string, Path2D>();
        for (const cell of grid) {
          for (const id of cell.maps) {
            if (!cellsOf.has(id)) cellsOf.set(id, new Path2D());
            cellsOf.get(id)!.rect(cell.x * tiles, cell.y * tiles, tiles, tiles);
          }
        }
        for (const [id, world] of lands) {
          const cells = id ? cellsOf.get(id) : undefined;
          w.save();
          if (cells) {
            w.clip(cells);
            w.fillStyle = color("--bg");
            w.fill(cells);
          }
          for (const layer of ["water", "roads", "buildings"] as const) {
            w.fillStyle = fill[layer];
            w.fill(world[layer]);
            for (const [l, p] of world.holed) if (l === layer) w.fill(p, "evenodd");
          }
          w.strokeStyle = fill.roads;
          w.lineWidth = Math.max(4, 1.5 / k);
          w.stroke(world.lines);
          w.restore();
        }
        worldLayer = { key, grid, canvas: c };
      }
      g.drawImage(worldLayer.canvas, 0, 0, width, height);
    }
    const gap = s >= 10 ? 1 : 0;
    for (const c of grid) {
      const p = paint(c);
      if (!p) continue;
      // over the world, the cells only tint it
      g.globalAlpha = lands.length || pic?.img ? p[1] / 2 : p[1];
      g.fillStyle = color(p[0]);
      g.fillRect(view.x + c.x * s, view.y + c.y * s, s - gap, s - gap);
    }
    g.globalAlpha = 1;
    g.lineWidth = 2;
    const ring = (cell: Cell, token: string) => {
      g.strokeStyle = color(token);
      g.strokeRect(view.x + cell[0] * s - 1, view.y + cell[1] * s - 1, s + 1, s + 1);
    };
    rings.forEach(([c, token]) => ring(c, token));
    if (hover) ring([hover.x, hover.y], "--text");
    // the app's own (bundled) body font, read once (a style recalc on every draw otherwise)
    g.font = `12px ${(fontFamily ||= getComputedStyle(canvas).fontFamily)}`;
    g.textAlign = "left";
    g.textBaseline = "middle";
    const r = dotR(s);
    for (const d of dots) {
      const [x, y] = [view.x + d.x * s, view.y + d.y * s];
      g.beginPath();
      g.arc(x, y, r, 0, 2 * Math.PI);
      g.fillStyle = color(d.colour);
      g.fill();
      g.strokeStyle = color("--bg");
      g.stroke();
      g.fillStyle = color("--text");
      g.fillText(d.name, x + r + 4, y);
    }
    // the cross Enter points at
    if (onPoint) {
      const [x, y] = mid();
      for (const [token, w] of [["--bg", 4], ["--text", 2]] as const) {
        g.strokeStyle = color(token);
        g.lineWidth = w;
        g.beginPath();
        g.moveTo(x - 12, y);
        g.lineTo(x + 12, y);
        g.moveTo(x, y - 12);
        g.lineTo(x, y + 12);
        g.stroke();
      }
    }
    // axis labels: cell numbers along the top and left edges, at least 32px apart
    const step = [1, 2, 5, 10, 20, 50, 100].find((n) => n * s >= 32) ?? 200;
    g.fillStyle = color("--panel");
    g.globalAlpha = 0.85;
    g.fillRect(0, 0, width, 16);
    g.fillRect(0, 0, 28, height);
    g.globalAlpha = 1;
    g.fillStyle = color("--muted");
    g.font = "11px 'IBM Plex Mono', monospace";
    g.textAlign = "center";
    for (let x = Math.ceil(-view.x / s / step) * step; view.x + x * s < width; x += step) if (view.x + (x + 0.5) * s > 28) g.fillText(String(x), view.x + (x + 0.5) * s, 8);
    g.textAlign = "right";
    for (let y = Math.ceil(-view.y / s / step) * step; view.y + y * s < height; y += step) if (view.y + (y + 0.5) * s > 16) g.fillText(String(y), 25, view.y + (y + 0.5) * s);
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
    const [fx, fy] = [(px - view.x) / view.scale, (py - view.y) / view.scale];
    return { x: Math.floor(fx), y: Math.floor(fy), fx, fy, px, py };
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
      const { x, y, fx, fy, px, py } = at(e);
      if (onPoint) {
        onPoint(fx, fy);
        drag = null;
        return;
      }
      const r = dotR(view.scale) + 3;
      const i = dots.findIndex((d) => Math.hypot(view.x + d.x * view.scale - px, view.y + d.y * view.scale - py) <= r);
      if (i >= 0 && onDot) onDot(i, e.clientX, e.clientY);
      else onPick?.([x, y], e.shiftKey);
    }
    drag = null;
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const { px, py } = at(e);
    zoom(e.deltaY < 0 ? 1.25 : 0.8, [px, py]);
  }
  /** Arrow keys pan, + and - zoom about the middle; Enter points at the cross. */
  function key(e: KeyboardEvent) {
    if (e.key === "Enter" && onPoint) {
      const [px, py] = mid();
      onPoint((px - view.x) / view.scale, (py - view.y) / view.scale);
      return e.preventDefault();
    }
    const pan = { ArrowLeft: [1, 0], ArrowRight: [-1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] }[e.key];
    if (pan) view = { ...view, x: view.x + pan[0] * 60, y: view.y + pan[1] * 60 };
    else if (e.key === "+" || e.key === "=") zoom(1.25);
    else if (e.key === "-") zoom(0.8);
    else return;
    e.preventDefault();
  }
</script>

<div class="map-grid" bind:clientWidth={width} bind:clientHeight={height}>
  <canvas bind:this={canvas} aria-label={label} tabindex="0" onkeydown={key}
    onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={() => (hover = null)} onwheel={wheel}></canvas>
  <span bind:this={probe} hidden></span>
  {#if hover && tip}
    <div class="tip" style:left="{Math.min(hover.px + 14, width - 230)}px" style:top="{Math.min(hover.py + 14, height - 140)}px">
      {@render tip([hover.x, hover.y])}
    </div>
  {/if}
  {@render children?.()}
</div>

<style>
  .map-grid {
    position: relative;
    overflow: hidden;
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: crosshair;
    touch-action: none;
  }
  /* the box clips: the focus ring goes inside */
  canvas:focus-visible {
    outline-offset: -2px;
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
</style>
