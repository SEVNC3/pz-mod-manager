<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { t, th } from "./i18n.svelte";
  import LayerIcon, { overlapLabel } from "./LayerIcon.svelte";
  import type { ModRow, Overlap, Row } from "./api";
  import type { Problem } from "./order";

  let { rows, order, mods, problems, fixable, overlaps, busy, onAutosort, onFix, onMove, onSelect, onMenu, onMenuKey }: {
    rows: Row[];
    /** The load order: enabled mod ids. */
    order: string[];
    mods: Map<string, ModRow>;
    problems: Map<string, Problem>;
    /** How many problems Fix load order can fix. */
    fixable: number;
    overlaps: Record<string, Overlap>;
    busy: boolean;
    onAutosort: () => void;
    onFix: () => void;
    /** The mod at `from` dropped before position `to`. */
    onMove: (from: number, to: number) => void;
    /** Open a mod's details. */
    onSelect: (id: string) => void;
    /** A row's right-click menu, at the pointer; `onMenuKey` opens it from the keyboard (true if it did). */
    onMenu: (id: string, x: number, y: number) => void;
    onMenuKey: (e: KeyboardEvent, id: string) => boolean;
  } = $props();

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

  // the list: drag (or Alt+arrows) to reorder, like the library
  let list = $state<HTMLOListElement>();
  let dragFrom = $state<number | null>(null);
  let dropAt = $state<number | null>(null);
  /** The row last clicked, highlighted like a selected row in the library. */
  let current = $state<string | null>(null);

  function drop(to: number) {
    if (dragFrom !== null) onMove(dragFrom, to);
    dragFrom = dropAt = null;
  }

  async function key(e: KeyboardEvent, i: number, id: string) {
    const li = e.currentTarget as HTMLElement;
    if (onMenuKey(e, id)) return;
    if ((e.key === "ArrowUp" || e.key === "ArrowDown") && e.altKey) {
      e.preventDefault();
      const to = e.key === "ArrowUp" ? i - 1 : i + 2;
      if (busy || to < 0 || to > order.length) return;
      current = id;
      onMove(i, to);
      await tick();
      (list?.children[e.key === "ArrowUp" ? i - 1 : i + 1] as HTMLElement | undefined)?.focus();
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      ((e.key === "ArrowUp" ? li.previousElementSibling : li.nextElementSibling) as HTMLElement | null)?.focus();
    } else if (e.key === "Enter") {
      onSelect(id);
    }
  }
</script>

<aside class="panel" aria-label={t("loadOrder.title")}>
  <h2>{t("loadOrder.title")}</h2>
  <p class="lede">{t("loadOrder.lede")}</p>

  <dl class="facts">
    <div><dt>{t("common.enabled")}</dt><dd>{order.length}</dd></div>
    <div class:bad={problems.size}><dt>{t("loadOrder.problems")}</dt><dd>{problems.size}</dd></div>
    <div><dt>{t("loadOrder.overridden")}</dt><dd title={t("loadOrder.overriddenTip")}>{overridden}</dd></div>
  </dl>

  <button class="primary wide" onclick={onAutosort} disabled={busy}><Icon name="wand" size={14} />{t("loadOrder.autosort")}</button>
  <ol class="steps">
    <li>{t("loadOrder.step.needs")}</li>
    <li>{t("loadOrder.step.tiers")}</li>
    <li>{@html th("loadOrder.step.rules")}</li>
    <li>{t("loadOrder.step.keep")}</li>
  </ol>

  {#if fixable}
    <button class="wide" onclick={onFix} disabled={busy}><Icon name="wand" size={14} />{t("loadOrder.fix", { count: fixable })}</button>
  {/if}

  {#if order.length}
    <h3>{t("loadOrder.modsInOrder")}</h3>
    <p class="hint">{t("loadOrder.dragHint")}</p>
    <ol class="drag-list" role="listbox" aria-label={t("loadOrder.modsInOrder")} bind:this={list}>
      {#each order as id, i (id)}
        {@const m = mods.get(id)}
        {@const p = problems.get(id)}
        <li role="option" aria-selected={current === id} class:selected={current === id} class:drop={dropAt === i} class:dragging={dragFrom === i} tabindex="0" draggable={!busy}
          onclick={() => (current = id)} ondblclick={() => onSelect(id)} onkeydown={(e) => key(e, i, id)}
          oncontextmenu={(e) => { e.preventDefault(); current = id; onMenu(id, e.clientX, e.clientY); }}
          ondragstart={() => (dragFrom = i)} ondragend={() => { dragFrom = dropAt = null; }}
          ondragover={(e) => { e.preventDefault(); dropAt = i; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}>
          <span class="num mono">{i + 1}</span>
          <span class="name">
            <span class="title" title={m?.name || id}>{m?.name || id}</span>
            {#if m?.name && m.name !== id}<span class="id mono">{id}</span>{/if}
          </span>
          {#if m?.missing || problems.has(id)}
            <span class="problem" title={m?.missing ? t("loadOrder.notInstalled") : p && t(p.key, { id: p.id })}><Icon name="alert" size={12} /></span>
          {/if}
          {#if overlaps[id]}
            <LayerIcon wins={overlaps[id].wins_over.length > 0} loses={overlaps[id].loses_to.length > 0} label={overlapLabel(overlaps[id])} />
          {/if}
        </li>
      {/each}
      <li class="tail" class:drop={dropAt === order.length} aria-hidden="true"
        ondragover={(e) => { e.preventDefault(); dropAt = order.length; }}
        ondrop={(e) => { e.preventDefault(); drop(order.length); }}></li>
    </ol>
  {/if}

  {#if sections.length}
    <h3>{t("loadOrder.librarySections")}</h3>
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
    white-space: normal; /* long translations wrap inside the panel */
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

  /* the row colours of .drag-list (app.css) reach the number and id too */
  li:not(.tail):hover .num,
  li.selected .num {
    color: var(--on-selected);
  }
  li:not(.tail):hover .id,
  li.selected .id {
    color: rgb(255 255 255 / 0.72);
  }
  .num {
    flex: none;
    width: 28px;
    text-align: right;
    color: var(--accent-text);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .title {
    font-weight: 700;
  }
  .id {
    margin-left: 8px;
    color: var(--muted);
  }
  .problem {
    display: inline-flex;
    color: var(--danger);
  }
</style>
