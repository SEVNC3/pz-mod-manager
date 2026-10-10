<script lang="ts">
  import { describeSource, type ModRow, type Overlap, type Row, type Stats } from "./api";
  import { fmtDate, fmtSize } from "./format";
  import Icon from "./Icon.svelte";
  import LayerIcon, { overlapLabel } from "./LayerIcon.svelte";
  import { sectionDrop, type Problem } from "./order";

  // The library: rows, separators as section cards, drag and drop. Edits go back through the callbacks.
  let {
    rows,
    order,
    visible,
    sectionOf,
    filtering,
    busy,
    build,
    stats,
    overlaps,
    problems,
    updates,
    collapsed,
    picked,
    selected,
    dragFrom = $bindable(),
    editingSep = $bindable(),
    onRowClick,
    onRowKey,
    onToggle,
    onToggleSection,
    onDrop,
    onRename,
    onRemoveSep,
  }: {
    rows: Row[];
    /** The load order (enabled mod ids): the number column shows each mod's place in it. */
    order: string[];
    /** The rows shown (search, filter, sort, collapsed sections) with their index in `rows`. */
    visible: { r: Row; i: number }[];
    /** For each row, the separator it sits under ("" above the first one). */
    sectionOf: string[];
    filtering: boolean;
    busy: boolean;
    build: number | null;
    stats: Record<string, Stats>;
    overlaps: Record<string, Overlap>;
    problems: Map<string, Problem>;
    updates: Set<string>;
    collapsed: Set<string>;
    picked: Set<string>;
    selected: string | null;
    dragFrom: number | null;
    editingSep: number | null;
    onRowClick: (e: MouseEvent, id: string) => void;
    onRowKey: (e: KeyboardEvent, i: number, r: Row) => void;
    onToggle: (id: string) => void;
    onToggleSection: (name: string) => void;
    /** A row (or a separator with its section) dropped before row `to`. */
    onDrop: (from: number, to: number) => void;
    onRename: (i: number, name: string) => void;
    onRemoveSep: (i: number) => void;
  } = $props();

  let dropAt = $state<number | null>(null);

  const SOURCE_HINTS: Record<string, string> = {
    steam: "Subscribed in Steam; Steam keeps it updated",
    workshop: "Downloaded from the Workshop by PZMM",
    github: "Installed from GitHub by PZMM",
    local: "Installed from a file by PZMM",
  };

  const place = $derived(new Map(order.map((id, i) => [id, i + 1])));
  /** Mods / enabled mods under each separator. */
  const sectionCounts = $derived.by(() => {
    const out = new Map<string, { total: number; on: number }>();
    rows.forEach((r, i) => {
      if (r.kind !== "mod") return;
      const c = out.get(sectionOf[i]) ?? { total: 0, on: 0 };
      c.total++;
      if (r.on) c.on++;
      out.set(sectionOf[i], c);
    });
    return out;
  });

  /** Once the list has sections, mods outside them are loose: each is a card of its own, in place. */
  const hasSections = $derived(!filtering && rows.some((r) => r.kind === "sep" && r.name));
  const loose = (k: number) => {
    const v = visible[k];
    return hasSections && !!v && v.r.kind === "mod" && !sectionOf[v.i];
  };

  // dragging a separator carries its whole section, and drops land on section boundaries
  const sectionDrag = $derived(dragFrom !== null && rows[dragFrom]?.kind === "sep");
  const dragEnd = $derived.by(() => {
    if (!sectionDrag || dragFrom === null) return -1;
    let e = dragFrom + 1;
    while (e < rows.length && rows[e].kind !== "sep") e++;
    return e;
  });
  const dropShown = $derived(sectionDrag && dragFrom !== null && dropAt !== null ? sectionDrop(rows, dragFrom, dropAt) : dropAt);

  function drop(to: number) {
    if (dragFrom !== null) onDrop(dragFrom, to);
    dragFrom = dropAt = null;
  }
</script>

<table>
  <thead class="visually-hidden">
    <tr><th>Move</th><th>Enabled</th><th>Load order</th><th>Mod</th><th>Size</th><th>Last changed</th><th>Shared files</th><th>Source</th></tr>
  </thead>
  <tbody>
    {#each visible as { r, i }, k (r.kind === "mod" ? `m:${r.id}` : `s:${i}:${r.name}`)}
      <!-- each section is a card: a separator starts one, the row before the next separator ends it -->
      {@const start = k === 0 || r.kind === "sep" || loose(k) || loose(k - 1)}
      {@const end = k === visible.length - 1 || visible[k + 1].r.kind === "sep" || loose(k) || loose(k + 1)}
      {#if start && k > 0}<tr class="gap" aria-hidden="true"><td colspan="8"></td></tr>{/if}
      {#if r.kind === "sep"}
        <tr class="sep" class:start class:end class:drop={dropShown === i} class:dragging={dragFrom === i} data-index={i} tabindex="0" draggable={!filtering && editingSep !== i}
          ondragstart={() => (dragFrom = i)} ondragend={() => { dragFrom = dropAt = null; }}
          ondragover={(e) => { e.preventDefault(); dropAt = i; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}
          ondblclick={() => !busy && (editingSep = i)} onkeydown={(e) => editingSep !== i && onRowKey(e, i, r)}>
          <td class="grip" aria-hidden="true"><Icon name="grip" size={14} fill /></td>
          <td colspan="7">
            {#if editingSep === i}
              <!-- svelte-ignore a11y_autofocus -->
              <input class="tape-edit" type="text" value={r.name} autofocus aria-label="Separator name (empty removes it)"
                onblur={(e) => onRename(i, e.currentTarget.value)}
                onkeydown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") editingSep = null; }} />
            {:else}
              <button class="quiet fold" aria-expanded={!collapsed.has(r.name)} aria-label={`${collapsed.has(r.name) ? "Expand" : "Collapse"} ${r.name}`}
                onclick={(e) => { e.stopPropagation(); onToggleSection(r.name); }}><Icon name={collapsed.has(r.name) ? "chevron-right" : "chevron-down"} size={14} /></button>
              <span class="tape" title="Double-click to rename">{r.name}</span>
              {#if sectionCounts.get(r.name)}
                {@const c = sectionCounts.get(r.name)!}
                <span class="sec-count">{c.on} of {c.total} enabled</span>
              {/if}
              <button class="quiet icon sep-remove" disabled={busy || filtering} title="Remove this separator (Delete). Its mods stay where they are."
                aria-label={`Remove separator ${r.name}`} onclick={(e) => { e.stopPropagation(); onRemoveSep(i); }}><Icon name="trash" size={14} /></button>
            {/if}
          </td>
        </tr>
      {:else}
        {@const src = describeSource(r.source)}
        <tr class="mod" class:start class:end class:off={!r.on} class:selected={selected === r.id} class:picked={picked.size >= 2 && picked.has(r.id)} class:drop={dropShown === i} class:dragging={dragFrom === i || (sectionDrag && dragFrom !== null && i > dragFrom && i < dragEnd)}
          data-index={i} tabindex="0" draggable={!filtering}
          aria-selected={selected === r.id}
          onclick={(e) => onRowClick(e, r.id)} onkeydown={(e) => onRowKey(e, i, r)}
          ondragstart={() => (dragFrom = i)} ondragend={() => { dragFrom = dropAt = null; }}
          ondragover={(e) => { e.preventDefault(); dropAt = i; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}>
          <td class="grip" aria-hidden="true">{#if !filtering}<Icon name="grip" size={14} fill />{/if}</td>
          <td class="check">
            <input type="checkbox" checked={r.on} disabled={busy} aria-label={`Enable ${r.name || r.id}`}
              onclick={(e) => e.stopPropagation()} onchange={() => onToggle(r.id)} />
          </td>
          <td class="num mono">{place.get(r.id) ?? ""}</td>
          <td class="name">
            <span class="title" title={r.name || r.id}>{r.name || r.id}</span>
            {#if r.name && r.name !== r.id}<span class="id mono">{r.id}</span>{/if}
            {#if r.only41 && (build ?? 42) !== 41}
              <span class="build-badge" title="Built for Build 41 only (no 42/ or common/ folder): on Build 42 it may error or do nothing">B41 only</span>
            {:else if r.only42 && build === 41}
              <span class="build-badge" title="Built for Build 42 only">B42 only</span>
            {/if}
          </td>
          <td class="meta mono">{stats[r.id] ? fmtSize(stats[r.id].size) : ""}</td>
          <td class="meta mono">{stats[r.id] ? fmtDate(stats[r.id].modified, "short") : ""}</td>
          <td class="overlap">
            {#if r.on && overlaps[r.id]}
              {@const o = overlaps[r.id]}
              <LayerIcon wins={o.wins_over.length > 0} loses={o.loses_to.length > 0} label={overlapLabel(o)} />
            {/if}
          </td>
          <td class="src">
            {#if r.missing}
              <span class="missing">Not installed</span>
            {:else if problems.has(r.id)}
              <span class="missing" title={problems.get(r.id)?.text}><Icon name="alert" size={12} />{problems.get(r.id)?.text}</span>
            {:else if updates.has(r.id)}
              <span class="update"><Icon name="update" size={12} />Update available</span>
            {:else}
              <span class="badge" data-kind={r.source.split(":")[0]} title={SOURCE_HINTS[r.source.split(":")[0]] ?? "Added to the mods folder outside PZMM"}>{src.label}</span>
            {/if}
          </td>
        </tr>
      {/if}
    {/each}
    {#if !filtering}
      <tr class="tail" class:drop={dropShown === rows.length} aria-hidden="true"
        ondragover={(e) => { e.preventDefault(); dropAt = rows.length; }}
        ondrop={(e) => { e.preventDefault(); drop(rows.length); }}>
        <td colspan="8"></td>
      </tr>
    {/if}
  </tbody>
</table>

<style>
  /* sections are cards lifted off the blueprint: rows paint the card, gap rows space them, and the
     table's drop shadow follows each card's shape (box-shadow can't wrap a group of rows) */
  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    filter: var(--card-drop);
  }
  tr.mod td {
    background: var(--panel);
  }
  tr.sep td {
    height: 46px;
    background: var(--panel);
  }
  tr.start td:first-child {
    border-top-left-radius: 8px;
  }
  tr.start td:last-child {
    border-top-right-radius: 8px;
  }
  tr.end td {
    border-bottom: 0;
  }
  tr.end td:first-child {
    border-bottom-left-radius: 8px;
  }
  tr.end td:last-child {
    border-bottom-right-radius: 8px;
  }
  tr.gap td {
    height: 14px;
    padding: 0;
    border: 0;
  }
  td {
    padding: 0 8px;
    height: 40px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  tr.mod {
    cursor: default;
  }
  /* hovered, picked and selected rows look like a selected tab: blood red under white text,
     with a white bar on the left */
  tr.mod:hover td {
    background: color-mix(in oklab, var(--accent) 24%, var(--panel));
  }
  tr.mod.picked td {
    background: color-mix(in oklab, var(--accent) 22%, var(--panel));
  }
  tr.mod.selected td {
    background: color-mix(in oklab, var(--accent) 32%, var(--panel));
  }
  tr.mod:hover td,
  tr.mod.picked td,
  tr.mod.selected td {
    color: var(--on-selected);
  }
  tr.mod:hover .num,
  tr.mod.picked .num,
  tr.mod.selected .num,
  tr.mod:hover .title,
  tr.mod.picked .title,
  tr.mod.selected .title {
    color: var(--on-selected);
  }
  tr.mod:hover .id,
  tr.mod:hover .meta,
  tr.mod.picked .id,
  tr.mod.picked .meta,
  tr.mod.selected .id,
  tr.mod.selected .meta {
    color: rgb(255 255 255 / 0.72);
  }
  /* the hover/selection marker: a solid white chevron sitting on the card's left edge, half over it */
  tr.mod td:first-child {
    position: relative;
  }
  tr.mod:hover td:first-child::before,
  tr.mod.picked td:first-child::before,
  tr.mod.selected td:first-child::before {
    content: "";
    position: absolute;
    left: -5px;
    top: 50%;
    width: 10px;
    height: 14px;
    margin-top: -7px;
    z-index: 1;
    background: var(--on-selected);
    clip-path: polygon(0 0, 45% 0, 100% 50%, 45% 100%, 0 100%, 55% 50%);
  }
  .fold {
    padding: 0 6px;
    margin-right: 4px;
    color: var(--muted);
  }
  .sec-count {
    margin-left: 12px;
    color: var(--muted);
    font-size: 12px;
  }
  tr.off .title,
  tr.off .num {
    color: var(--muted);
  }
  tr.dragging {
    opacity: 0.4;
  }
  /* where a drag will land: blood red feathering down from the row's top edge */
  tr.drop td {
    background-image: linear-gradient(to bottom, color-mix(in oklab, var(--accent) 85%, white) 0, color-mix(in oklab, var(--accent) 55%, transparent) 3px, transparent 20px);
  }
  tr:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .grip {
    width: 22px;
    color: var(--muted);
    cursor: grab;
    text-align: center;
    padding-right: 0;
  }
  .check {
    width: 28px;
  }
  .check input {
    width: 16px;
    height: 16px;
  }
  .num {
    width: 40px;
    text-align: right;
    color: var(--accent-text);
  }
  /* max-width: 0 lets an auto-layout cell shrink, so long names ellipsize instead of wrapping */
  .name {
    max-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .name .title {
    font-weight: 700;
  }
  .name .id {
    margin-left: 10px;
    color: var(--muted);
  }
  .icon {
    padding: 4px 8px;
    font-size: 16px;
    line-height: 1;
  }
  .build-badge {
    margin-left: 8px;
    font-size: 11px;
    font-weight: 700;
    color: var(--danger);
    border: 1px solid color-mix(in oklab, var(--danger) 50%, transparent);
    border-radius: 4px;
    padding: 0 5px;
  }
  .meta {
    width: 78px;
    text-align: right;
    color: var(--muted);
    white-space: nowrap;
  }
  /* its own column so the icons line up whatever the badge beside them says */
  .overlap {
    width: 28px;
    padding-left: 8px;
    padding-right: 0;
    text-align: center;
  }
  .src {
    width: 1%;
    white-space: nowrap;
    text-align: right;
  }
  .badge {
    font-size: 12px;
    color: var(--muted);
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 1px 8px;
  }
  .badge[data-kind="github"],
  .badge[data-kind="workshop"],
  .badge[data-kind="local"] {
    color: var(--text);
  }
  .update {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--accent-text);
    font-weight: 700;
    font-size: 12px;
  }
  .missing {
    display: inline-block;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: middle;
    color: var(--danger);
    font-weight: 700;
    font-size: 12px;
  }
  .missing :global(.icon) {
    margin-right: 4px;
  }
  tr.tail td {
    height: 48px;
    border: 0;
  }

  /* signature: separators are masking-tape labels, like crates in a safehouse */
  .tape,
  .tape-edit {
    display: inline-block;
    font-family: var(--font-marker);
    font-size: 16px;
    letter-spacing: 0.02em;
    color: var(--tape-ink);
    background: var(--tape);
    padding: 2px 16px 0;
    transform: rotate(-0.8deg);
    clip-path: polygon(0 8%, 3% 0, 6% 10%, 9% 1%, 100% 0, 98% 30%, 100% 55%, 97% 80%, 100% 100%, 4% 100%, 1% 88%, 3% 60%, 0 40%);
    box-shadow: 0 1px 0 rgb(0 0 0 / 0.15);
  }
  .tape {
    padding-right: 72px;
    background: url(/blood.svg) right -10px center / auto 140% no-repeat, var(--tape);
  }
  .tape-edit {
    border: 0;
    border-radius: 0;
    outline-offset: 3px;
    min-width: 220px;
  }
  .sep-remove {
    float: right;
    margin-top: 6px;
    color: var(--muted);
    opacity: 0;
  }
  tr.sep:hover .sep-remove,
  tr.sep:focus-within .sep-remove {
    opacity: 1;
  }
  .sep-remove:hover:not(:disabled) {
    color: var(--danger);
  }
</style>
