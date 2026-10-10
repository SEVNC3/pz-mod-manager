<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { Overlap, Row } from "./api";
  import type { Problem } from "./order";

  let { rows, problems, fixable, overlaps, busy, onAutosort, onFix }: {
    rows: Row[];
    problems: Map<string, Problem>;
    /** How many problems Fix load order can fix. */
    fixable: number;
    overlaps: Record<string, Overlap>;
    busy: boolean;
    onAutosort: () => void;
    onFix: () => void;
  } = $props();

  const enabled = $derived(rows.filter((r) => r.kind === "mod" && r.on).length);
  const overridden = $derived(Object.values(overlaps).filter((o) => o.loses_to.length).length);
  const sections = $derived.by(() => {
    const out: { name: string; on: number; total: number }[] = [];
    for (const r of rows) {
      if (r.kind === "sep") {
        if (r.name) out.push({ name: r.name, on: 0, total: 0 });
        else out.push({ name: "", on: 0, total: 0 }); // a break: loose mods, not listed
      } else if (out.length) {
        out[out.length - 1].total++;
        if (r.on) out[out.length - 1].on++;
      }
    }
    return out.filter((s) => s.name);
  });
</script>

<aside class="panel" aria-label="Load order">
  <h2>Load order</h2>
  <p class="lede">Mods load top to bottom. When two change the same file, the lower one wins.</p>

  <dl class="facts">
    <div><dt>Enabled</dt><dd>{enabled}</dd></div>
    <div class:bad={problems.size}><dt>Problems</dt><dd>{problems.size}</dd></div>
    <div><dt>Overridden</dt><dd title="Enabled mods with files another mod replaces">{overridden}</dd></div>
  </dl>

  <button class="primary wide" onclick={onAutosort} disabled={busy}><Icon name="wand" size={14} />Autosort</button>
  <ol class="steps">
    <li>Turns on mods that your enabled mods need.</li>
    <li>Replaces your sections with one per category, from the mods' Workshop tags: frameworks first, so everything built on them loads after; translations last, so their text wins. It asks first if you have sections of your own.</li>
    <li>Moves mods so every <span class="mono">require</span>, <span class="mono">loadModAfter</span> and <span class="mono">loadModBefore</span> rule holds.</li>
    <li>Keeps your order within each category. Ctrl+Z puts the old order back.</li>
  </ol>

  {#if fixable}
    <button class="wide" onclick={onFix} disabled={busy}><Icon name="wand" size={14} />Fix {fixable} {fixable === 1 ? "problem" : "problems"} without regrouping</button>
  {/if}

  {#if sections.length}
    <h3>Sections</h3>
    <ul class="sections">
      {#each sections as s}
        <li><span>{s.name}</span><span class="mono">{s.on}/{s.total}</span></li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .panel {
    overflow-y: auto;
    padding: 16px 18px 24px;
    border-left: 1px solid var(--line);
    background: var(--panel);
  }
  h2 {
    margin: 0 0 4px;
    font-size: 20px;
    line-height: 1.2;
  }
  h3 {
    margin: 20px 0 6px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  .lede {
    color: var(--muted);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin: 0 0 14px;
  }
  .facts div {
    background: var(--raised);
    border-radius: 6px;
    padding: 6px 10px;
  }
  dt {
    font-size: 12px;
    color: var(--muted);
  }
  dd {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
  }
  .facts .bad dd {
    color: var(--danger);
  }
  .wide {
    width: 100%;
  }
  .steps {
    margin: 10px 0 14px;
    padding-left: 20px;
    color: var(--muted);
    font-size: 13px;
  }
  .steps li {
    margin-bottom: 4px;
  }
  .sections {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .sections li {
    display: flex;
    justify-content: space-between;
    padding: 3px 0;
    border-bottom: 1px solid var(--line);
  }
  .sections .mono {
    color: var(--muted);
  }
</style>
