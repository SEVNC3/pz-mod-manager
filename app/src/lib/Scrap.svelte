<script module lang="ts">
  // A scrap of paper taped onto the blueprint (What's new). Its look comes from an id, so it's the
  // same every time: a torn edge, a strip of tape, and now and then blood, dirt or a fingerprint.
  function rng(seed: string) {
    let h = 2166136261;
    for (const c of seed) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
    return () => {
      h = Math.imul(h ^ (h >>> 15), 2246822507);
      h = Math.imul(h ^ (h >>> 13), 3266489909);
      return ((h ^= h >>> 16) >>> 0) / 4294967296;
    };
  }

  /** One mark on a scrap: a filled (or ringed) shape in a 100-unit box, placed by its centre. */
  type Mark = { x: number; y: number; size: number; rot: number; d: string; ink: "blood" | "dirt"; o: number; blur: number; ring: boolean; width?: number };

  /** An irregular blob around (cx, cy), with a smooth outline. */
  function blob(r: () => number, cx: number, cy: number, size: number, points = 11) {
    const p = Array.from({ length: points }, (_, i) => {
      const a = (i / points) * Math.PI * 2;
      const k = size * (0.7 + r() * 0.5);
      return [cx + Math.cos(a) * k, cy + Math.sin(a) * k];
    });
    const f = (v: number[]) => v.map((n) => n.toFixed(1)).join(",");
    const mid = (i: number) => p[i % points].map((v, j) => (v + p[(i + 1) % points][j]) / 2);
    let d = `M${f(mid(0))}`;
    for (let i = 1; i <= points; i++) d += `Q${f(p[i % points])} ${f(mid(i))}`;
    return d + "Z";
  }

  /** Little dots scattered round the centre, denser near it. */
  function specks(r: () => number, n: number, spread: number, min: number, max: number) {
    let d = "";
    for (let i = 0; i < n; i++) {
      const a = r() * Math.PI * 2;
      const dist = spread * Math.sqrt(r());
      d += blob(r, 50 + Math.cos(a) * dist, 50 + Math.sin(a) * dist, min + r() * (max - min), 6);
    }
    return d;
  }

  // the edges and corners on the right and at the bottom: titles, text and buttons start at the left
  const edgeSpot = (r: () => number) => (r() < 0.5 ? [78 + r() * 22, 6 + r() * 88] : [30 + r() * 70, 82 + r() * 18]);

  /** A fingertip's ridges inside a 100-unit box: a whorl, a loop or an arch, broken where the print
      didn't take. Stroked, not filled. */
  function fingerprint(r: () => number) {
    const kind = Math.floor(r() * 3);
    const [rx, ry] = [24 + r() * 6, 31 + r() * 6];
    const inside = (x: number, y: number) => ((x - 50) / rx) ** 2 + ((y - 50) / ry) ** 2 < 1;
    // part of the print is missing: a side that didn't press down
    const cut = r() < 0.5 ? { a: r() * Math.PI * 2, k: 0.25 + r() * 0.5 } : null;
    const kept = (x: number, y: number) => inside(x, y) && (!cut || (x - 50) * Math.cos(cut.a) + (y - 50) * Math.sin(cut.a) < cut.k * rx);
    let d = "";
    const line = (pts: [number, number][]) => {
      let on = false;
      for (const [x, y] of pts) {
        // short gaps where the ridge didn't print
        const draw = kept(x, y) && r() > 0.07;
        d += `${draw && on ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`;
        on = draw;
      }
    };
    const rings = 9 + Math.floor(r() * 4);
    for (let k = 1; k <= rings; k++) {
      const pts: [number, number][] = [];
      if (kind === 2) {
        // arch: ridges rise in the middle and run out at the sides
        const y0 = 50 + ry - k * ((2 * ry) / rings);
        for (let x = 50 - rx; x <= 50 + rx; x += 2) pts.push([x, y0 - (6 + k * 0.4) * Math.cos(((x - 50) / rx) * (Math.PI / 2))]);
      } else {
        // whorl: closed rings; loop: rings open at the bottom, pushed to one side
        const span = kind === 0 ? Math.PI * 2 : Math.PI * 1.35;
        const start = kind === 0 ? 0 : Math.PI * 0.82;
        const [cx, cy] = kind === 0 ? [50, 48] : [46 + r() * 2, 44];
        for (let t = 0; t <= span; t += 0.12) pts.push([cx + Math.cos(start + t) * k * 2.6, cy + Math.sin(start + t) * k * 3.1]);
      }
      line(pts);
    }
    return d;
  }

  /** The kinds of mark, with how often each turns up. */
  const KINDS: [number, (r: () => number) => Mark][] = [
    // a fine spray of blood
    [24, (r) => { const [x, y] = edgeSpot(r); return { x, y, size: 70 + r() * 70, rot: r() * 360, d: specks(r, 12 + Math.floor(r() * 22), 34, 0.4, 1.6), ink: "blood", o: 0.35 + r() * 0.3, blur: 0, ring: false }; }],
    // a little dirt, anywhere, faint
    [12, (r) => ({ x: r() * 100, y: r() * 100, size: 80 + r() * 80, rot: r() * 360, d: specks(r, 6 + Math.floor(r() * 10), 40, 0.3, 0.9), ink: "dirt", o: 0.18 + r() * 0.17, blur: 0, ring: false })],
    // a faint smear of blood wiped across
    [10, (r) => { const [x, y] = edgeSpot(r); const w = 6 + r() * 6; return { x, y, size: 90 + r() * 60, rot: r() * 180, d: `M12,50 C30,${50 - w} 70,${50 - w * 0.6} 90,${48 + r() * 4} C70,${50 + w * 0.7} 30,${50 + w} 12,50Z`, ink: "blood", o: 0.14 + r() * 0.14, blur: 1.5 + r() * 2, ring: false }; }],
    // a splat: small or large, with or without runs, its spray
    [12, (r) => {
      const [x, y] = edgeSpot(r);
      const big = 5 + r() * 9;
      let d = blob(r, 50, 46, big, 13) + specks(r, 6 + Math.floor(r() * 14), big * 2.6, 0.4, 1.9);
      for (let k = 0; k < (r() < 0.5 ? 0 : 1 + Math.floor(r() * 2)); k++) {
        const dx = 50 + (r() - 0.5) * big;
        const top = 46 + big * 0.4;
        const end = top + big * (0.8 + r() * 1.8);
        const w = 0.8 + r() * 1.2;
        d += `M${(dx - w).toFixed(1)},${top.toFixed(1)} Q${(dx - w * 0.5).toFixed(1)},${((top + end) / 2).toFixed(1)} ${(dx - w * 0.45).toFixed(1)},${end.toFixed(1)} Q${dx.toFixed(1)},${(end + w * 2.2).toFixed(1)} ${(dx + w * 0.45).toFixed(1)},${end.toFixed(1)} Q${(dx + w * 0.5).toFixed(1)},${((top + end) / 2).toFixed(1)} ${(dx + w).toFixed(1)},${top.toFixed(1)}Z`;
      }
      return { x, y, size: 90 + r() * 60, rot: (r() - 0.5) * 30, d, ink: "blood", o: 0.6 + r() * 0.25, blur: 0, ring: false };
    }],
    // a bloody corner: picked up by a bloody hand, soaked in from one corner
    [12, (r) => {
      const corner = [[100, 0], [100, 100], [0, 100]][Math.floor(r() * 3)];
      const smeared = r() < 0.4;
      // the blob sits on the corner; the paper's edge cuts it
      const d = blob(r, 50, 50, 26 + r() * 14, 16) + specks(r, 6 + Math.floor(r() * 10), 46, 0.5, 2.2);
      return { x: corner[0], y: corner[1], size: 120 + r() * 70, rot: r() * 360, d, ink: "blood", o: 0.6 + r() * 0.25, blur: smeared ? 1.2 + r() * 1.5 : 0, ring: false };
    }],
    // a bloody fingerprint: a whorl, loop or arch, sometimes partial or smudged, now and then a pair
    [14, (r) => {
      const [x, y] = edgeSpot(r);
      return { x, y, size: 46 + r() * 26, rot: (r() - 0.5) * 90, d: fingerprint(r), ink: "blood", o: 0.4 + r() * 0.3, blur: r() < 0.3 ? 0.5 : 0, ring: true, width: 1.6 + r() * 0.8 };
    }],
  ];
  const TOTAL = KINDS.reduce((n, [w]) => n + w, 0);

  function look(id: string) {
    const r = rng(id);
    const jag = (n: number, f: (i: number, j: number) => string) => Array.from({ length: n + 1 }, (_, i) => f((i / n) * 100, r() * 1.6)).join(",");
    const edge = `polygon(${[
      jag(18, (x, j) => `${x.toFixed(1)}% ${j.toFixed(2)}%`),
      jag(14, (y, j) => `${(100 - j).toFixed(2)}% ${y.toFixed(1)}%`),
      jag(18, (x, j) => `${(100 - x).toFixed(1)}% ${(100 - j).toFixed(2)}%`),
      jag(14, (y, j) => `${j.toFixed(2)}% ${(100 - y).toFixed(1)}%`),
    ].join(",")})`;
    const marks: Mark[] = [];
    // some scraps stay clean; most pick up one to three marks
    const count = r() < 0.12 ? 0 : 1 + Math.floor(r() * 3);
    for (let i = 0; i < count; i++) {
      let pick = r() * TOTAL;
      const kind = KINDS.find(([w]) => (pick -= w) < 0) ?? KINDS[0];
      const m = kind[1](r);
      marks.push(m);
      if (m.ring && r() < 0.3) marks.push({ ...m, x: m.x - 6 - r() * 6, y: m.y + 4 + r() * 6, rot: m.rot + (r() - 0.5) * 30, o: m.o * 0.7, d: fingerprint(r) });
    }
    return { tilt: (r() - 0.5) * 4.4, tape: (r() - 0.5) * 10, tapeX: 30 + r() * 40, edge, marks };
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  let { id, off = false, children }: {
    /** Picks the look. */
    id: string;
    /** Greyer paper (a disabled mod's). */
    off?: boolean;
    children: Snippet;
  } = $props();
  const l = $derived(look(id));
</script>

<li class="scrap" class:off style="--tilt: {l.tilt}deg">
  <span class="tape" style="--tape: {l.tape}deg; left: {l.tapeX}%" aria-hidden="true"></span>
  <article class="paper" style="clip-path: {l.edge}">
    {#each l.marks as m}
      <svg class="mark {m.ink}" class:ring={m.ring} viewBox="0 0 100 100" aria-hidden="true" stroke-width={m.width ?? 2.4}
        style="left: {m.x}%; top: {m.y}%; width: {m.size}px; height: {m.size}px; opacity: {m.o}; rotate: {m.rot}deg; filter: {m.blur ? `blur(${m.blur}px)` : 'none'}"><path d={m.d} /></svg>
    {/each}
    {@render children()}
  </article>
</li>

<style>
  .scrap {
    --paper: #ece7da;
    --ink: #24211c;
    --ink-soft: #5d574c;
    --blood: #6a0a0d;
    position: relative;
    transform: rotate(var(--tilt));
    filter: drop-shadow(0 6px 10px rgb(0 0 0 / 0.45)) drop-shadow(0 1px 1px rgb(0 0 0 / 0.35));
    transition: transform 0.18s ease;
  }
  .scrap:hover,
  .scrap:focus-within {
    transform: rotate(calc(var(--tilt) * 0.4)) translateY(-3px);
  }
  @media (prefers-reduced-motion: reduce) {
    .scrap {
      transition: none;
    }
  }
  .scrap.off {
    --paper: #dcd8cd;
  }
  .paper {
    position: relative;
    min-height: 200px;
    padding: 24px 20px 16px;
    color: var(--ink);
    /* fibres and grain: an SVG noise over the paper colour */
    background:
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='180' height='180'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='3' stitchTiles='stitch'/%3E%3CfeColorMatrix values='0 0 0 0 0.35 0 0 0 0 0.32 0 0 0 0 0.26 0 0 0 0.16 0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E"),
      radial-gradient(120% 90% at 30% 20%, transparent 55%, rgb(120 100 70 / 0.12)),
      var(--paper);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .tape {
    position: absolute;
    top: -9px;
    width: 104px;
    height: 26px;
    margin-left: -52px;
    z-index: 2;
    transform: rotate(var(--tape));
    background:
      repeating-linear-gradient(90deg, rgb(255 255 255 / 0.08) 0 2px, transparent 2px 6px),
      rgb(231 219 170 / 0.8);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    clip-path: polygon(3% 0, 97% 4%, 100% 30%, 98% 70%, 100% 100%, 2% 96%, 0 65%, 2% 35%);
  }
  .mark {
    position: absolute;
    translate: -50% -50%;
    pointer-events: none;
    mix-blend-mode: multiply;
  }
  .mark.blood path {
    fill: var(--blood);
  }
  .mark.dirt path {
    fill: #4a3a2a;
  }
  .mark.ring path {
    fill: none;
    stroke: var(--blood);
    stroke-linecap: round;
  }
  .paper :global(.name) {
    all: unset;
    position: relative;
    cursor: pointer;
    font-family: var(--font-marker);
    font-size: 21px;
    line-height: 1.15;
    color: var(--ink);
    overflow-wrap: anywhere;
  }
  .paper :global(.name:hover),
  .paper :global(.name:focus-visible) {
    text-decoration: underline;
    text-decoration-color: var(--blood);
  }
  .paper :global(.name:focus-visible) {
    outline: 2px dashed var(--blood);
    outline-offset: 2px;
  }
  .paper :global(.meta) {
    position: relative;
    margin: 0;
    font-size: 12px;
    color: var(--ink-soft);
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
  }
  .paper :global(.stamp) {
    margin-left: auto;
    align-self: center;
    padding: 2px 7px;
    border: 2px solid var(--blood);
    border-radius: 3px;
    color: var(--blood);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    transform: rotate(-7deg);
    opacity: 0.85;
    mix-blend-mode: multiply;
  }
  .paper :global(.excerpt) {
    position: relative;
    margin: 4px 0 0;
    font-size: 13px;
    line-height: 1.45;
    white-space: pre-line;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 7;
    line-clamp: 7;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .paper :global(.excerpt.muted) {
    color: var(--ink-soft);
    font-style: italic;
  }
  .paper :global(.actions) {
    position: relative;
    margin-top: auto;
    padding-top: 8px;
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .paper :global(.ink) {
    border: 1px solid rgb(36 33 28 / 0.35);
    background: rgb(255 255 255 / 0.25);
    color: var(--ink);
  }
  .paper :global(.ink:hover:not(:disabled)) {
    background: var(--blood);
    border-color: var(--blood);
    color: #fff;
  }
  .paper :global(.ink.strong) {
    border-color: var(--blood);
    color: var(--blood);
    font-weight: 700;
  }
</style>
