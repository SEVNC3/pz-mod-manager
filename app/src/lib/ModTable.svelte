<script lang="ts">
  import { describeSource, holdable, type ModRow, type Overlap, type Row, type Stats } from "./api";
  import { dragScroll } from "./dragscroll";
  import { fmtDate, fmtNum, fmtSize } from "./format";
  import { t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import LayerIcon, { overlapLabel } from "./LayerIcon.svelte";
  import { groupColour, sectionDrop, type Problem } from "./order";
  import { prefs } from "./prefs.svelte";

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
    held,
    heldNewer,
    favourites,
    fresh,
    hidden,
    kinds,
    neededBy,
    tags,
    collapsed,
    picked,
    selected,
    dragFrom = $bindable(),
    editingSep = $bindable(),
    onRowClick,
    onRowKey,
    onToggle,
    onToggleSection,
    onSubscribe,
    onDrop,
    onRename,
    onRemoveSep,
    onMenu,
    onSepMenu,
    sortBy,
    onSort,
    onHide,
    onHold,
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
    /** Mods whose updates are held, and the held ones the Workshop has a newer version of (when it came out). */
    held: Set<string>;
    heldNewer: Record<string, number>;
    /** Mods starred or hidden in the game's mod menu or a Workshop mod manager (see Lists). */
    favourites: Set<string>;
    /** Mods new to this profile (this session): a New pill. */
    fresh: Set<string>;
    hidden: Set<string>;
    /** Each mod's kinds (framework, patch, ...), once read. */
    kinds: Record<string, string[]>;
    /** For each mod, the enabled mods that require it. */
    neededBy: Map<string, string[]>;
    /** The player's own tags, by mod id. */
    tags: Record<string, string[]>;
    collapsed: Set<string>;
    picked: Set<string>;
    selected: string | null;
    dragFrom: number | null;
    editingSep: number | null;
    onRowClick: (e: MouseEvent, id: string) => void;
    onRowKey: (e: KeyboardEvent, i: number, r: Row) => void;
    onToggle: (id: string) => void;
    onToggleSection: (name: string) => void;
    /** The Workshop chip: subscribe to the mod's item in Steam (`on`), or unsubscribe from it. */
    onSubscribe: (id: string, on: boolean) => void;
    /** A row (or a separator with its section) dropped before row `to`. */
    onDrop: (from: number, to: number) => void;
    onRename: (i: number, name: string) => void;
    onRemoveSep: (i: number) => void;
    /** A mod row's right-click menu, at the pointer. */
    onMenu: (id: string, x: number, y: number) => void;
    /** A separator's right-click menu, at the pointer. */
    onSepMenu: (i: number, x: number, y: number) => void;
    /** The list's sort (a header shows it; "order" is the library's own). */
    sortBy: string;
    /** A sortable header clicked. */
    onSort: (by: string) => void;
    /** The eye: hide the mod from the list, or show it again. */
    onHide: (id: string) => void;
    /** The lock: hold the mod's updates, or allow them again. */
    onHold: (id: string) => void;
  } = $props();
  const holdTip = (id: string) =>
    held.has(id) && heldNewer[id] ? t("modTable.heldNewerTip", { date: fmtDate(heldNewer[id], "long") }) : t(held.has(id) ? "modTable.heldTip" : "modTable.holdTip");

  /** The headers that sort, and the sort each one picks. */
  const HEADS: [string, string, string][] = [
    ["num", "order", "#"],
    ["name", "name", "modTable.col.mod"],
    ["meta", "size", "modTable.col.size"],
    ["meta", "installed", "modTable.col.installed"],
    ["meta", "updated", "modTable.col.lastChanged"],
  ];

  let dropAt = $state<number | null>(null);

  /** The group chips' colours, by `groupColour` (the game's palette too). */
  const GROUP_COLOURS = ["#c0504d", "#d9883b", "#c9b03c", "#6aa84f", "#3d9e9e", "#4a86c5", "#8064a2", "#c25b9b"];

  const SOURCE_HINTS: Record<string, string> = {
    steam: "modTable.sourceHint.steam",
    workshop: "modTable.sourceHint.workshop",
    github: "modTable.sourceHint.github",
    local: "modTable.sourceHint.local",
  };

  const place = $derived(new Map(order.map((id, i) => [id, i + 1])));
  const names = $derived(new Map(rows.flatMap((r) => (r.kind === "mod" ? [[r.id, r.name || r.id]] : []))));
  /** A translation's languages: a translation kind, or texts only in languages other than English. */
  const langsOf = (r: ModRow) => (kinds[r.id]?.includes("translation") || (r.languages.length && !r.languages.includes("EN")) ? r.languages : []);
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

<table use:dragScroll>
  <thead>
    <tr>
      <!-- each header cell has its column's class (and padding), so it lines up with the cells under it -->
      <th class="grip"><span class="visually-hidden">{t("modTable.col.move")}</span></th><th class="check"><span class="visually-hidden">{t("common.enabled")}</span></th><th class="eye"><span class="visually-hidden">{t("modTable.col.hide")}</span></th><th class="eye"><span class="visually-hidden">{t("modTable.col.hold")}</span></th>
      {#each HEADS as [cls, by, key]}
        <th class={cls} aria-sort={sortBy === by && by !== "order" ? "descending" : undefined}>
          <!-- i18n-ignore: "#" is the load order number -->
          <button class="head" class:on={sortBy === by} title={t("modTable.sortByTip")} onclick={() => onSort(by)}>{key === "#" ? "#" : t(key)}{#if sortBy === by && by !== "order"}<Icon name="chevron-down" size={12} />{/if}</button>
        </th>
      {/each}
      <th class="overlap overlap-head">{t("modTable.col.overwrites")}</th><th class="src">{t("modTable.col.source")}</th>
    </tr>
  </thead>
  <tbody>
    {#each visible as { r, i }, k (r.kind === "mod" ? `m:${r.id}` : `s:${i}:${r.name}`)}
      <!-- each section is a card: a separator starts one, the row before the next separator ends it -->
      {@const start = k === 0 || r.kind === "sep" || loose(k) || loose(k - 1)}
      {@const end = k === visible.length - 1 || visible[k + 1].r.kind === "sep" || loose(k) || loose(k + 1)}
      {#if start && k > 0}<tr class="gap" aria-hidden="true"><td colspan="11"></td></tr>{/if}
      {#if r.kind === "sep"}
        <tr class="sep" class:start class:end class:drop={dropShown === i} class:dragging={dragFrom === i} data-index={i} tabindex="0" draggable={!filtering && editingSep !== i}
          ondragstart={() => (dragFrom = i)} ondragend={() => { dragFrom = dropAt = null; }}
          ondragover={(e) => { e.preventDefault(); dropAt = i; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}
          ondblclick={() => !busy && (editingSep = i)} oncontextmenu={(e) => { if (editingSep === i) return; e.preventDefault(); onSepMenu(i, e.clientX, e.clientY); }} onkeydown={(e) => editingSep !== i && onRowKey(e, i, r)}>
          <td class="grip" aria-hidden="true"><Icon name="grip" size={14} fill /></td>
          <td colspan="10">
            {#if editingSep === i}
              <!-- svelte-ignore a11y_autofocus -->
              <input class="tape-edit" type="text" value={r.name} autofocus aria-label={t("modTable.sepNameLabel")}
                onblur={(e) => onRename(i, e.currentTarget.value)}
                onkeydown={(e) => { if (e.key === "Enter" || e.key === "Escape") e.stopPropagation(); if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") editingSep = null; /* i18n-ignore: key names */ }} />
            {:else}
              <button class="quiet fold" aria-expanded={!collapsed.has(r.name)} aria-label={t(collapsed.has(r.name) ? "modTable.expandLabel" : "modTable.collapseLabel", { name: r.name })}
                onclick={(e) => { e.stopPropagation(); onToggleSection(r.name); }}><Icon name={collapsed.has(r.name) ? "chevron-right" : "chevron-down"} size={14} /></button>
              <span class="tape" title={t("modTable.renameTip")}>{r.name}</span>
              {#if sectionCounts.get(r.name)}
                {@const c = sectionCounts.get(r.name)!}
                <span class="sec-count">{t("modTable.sectionCount", { on: fmtNum(c.on), total: fmtNum(c.total) })}</span>
              {/if}
              <button class="quiet icon sep-remove" disabled={busy || filtering} title={t("modTable.removeSepTip")}
                aria-label={t("modTable.removeSepLabel", { name: r.name })} onclick={(e) => { e.stopPropagation(); onRemoveSep(i); }}><Icon name="trash" size={14} /></button>
            {/if}
          </td>
        </tr>
      {:else}
        {@const src = describeSource(r.source)}
        <tr class="mod" class:start class:end class:off={!r.on} class:hidden-mod={hidden.has(r.id)} class:selected={selected === r.id} class:picked={picked.size >= 2 && picked.has(r.id)} class:drop={dropShown === i} class:dragging={dragFrom === i || (sectionDrag && dragFrom !== null && i > dragFrom && i < dragEnd)}
          data-index={i} tabindex="0" draggable={!filtering}
          aria-current={selected === r.id || undefined}
          onclick={(e) => onRowClick(e, r.id)} onkeydown={(e) => onRowKey(e, i, r)}
          oncontextmenu={(e) => { e.preventDefault(); onMenu(r.id, e.clientX, e.clientY); }}
          ondragstart={() => (dragFrom = i)} ondragend={() => { dragFrom = dropAt = null; }}
          ondragover={(e) => { e.preventDefault(); dropAt = i; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}>
          <td class="grip" aria-hidden="true">{#if !filtering}<Icon name="grip" size={14} fill />{/if}</td>
          <td class="check">
            <input type="checkbox" checked={r.on} disabled={busy} aria-label={t("modTable.enableLabel", { name: r.name || r.id })}
              onclick={(e) => e.stopPropagation()} onchange={() => onToggle(r.id)} />
          </td>
          <td class="eye">
            <button class="quiet icon" class:on={hidden.has(r.id)} disabled={busy} title={t(hidden.has(r.id) ? "modTable.unhideTip" : "modTable.hideTip")}
              aria-label={t(hidden.has(r.id) ? "modTable.unhideTip" : "modTable.hideTip")} aria-pressed={hidden.has(r.id)}
              onclick={(e) => { e.stopPropagation(); onHide(r.id); }} onkeydown={(e) => e.stopPropagation()}><Icon name={hidden.has(r.id) ? "eye-off" : "eye"} size={14} /></button>
          </td>
          <td class="eye hold">
            {#if holdable(r)}
              <button class="quiet icon" class:on={held.has(r.id)} disabled={busy} title={holdTip(r.id)} aria-label={holdTip(r.id)} aria-pressed={held.has(r.id)}
                onclick={(e) => { e.stopPropagation(); onHold(r.id); }} onkeydown={(e) => e.stopPropagation()}><Icon name={held.has(r.id) ? "lock" : "unlock"} size={14} /></button>
            {/if}
          </td>
          <td class="num mono">{place.get(r.id) ?? ""}</td>
          <td class="name">
            {#if favourites.has(r.id)}<span class="fav" title={t("modTable.favourite")}><Icon name="star" size={12} /></span>{/if}
            <span class="title" title={r.author ? t("modTable.byAuthor", { name: r.name || r.id, author: r.author }) : r.name || r.id}>{r.name || r.id}</span>
            {#if prefs.showIds && r.name && r.name !== r.id}<span class="id mono">{r.id}</span>{/if}
            <!-- out of library order the tapes are gone: each mod says its group -->
            {#if filtering && sectionOf[i]}<span class="group-chip" style:--group={GROUP_COLOURS[groupColour(sectionOf[i])]}>{sectionOf[i]}</span>{/if}
            {#if fresh.has(r.id)}<span class="build-badge new" title={t("modTable.newTip")}>{t("modTable.new")}</span>{/if}
            {#each kinds[r.id] ?? [] as k}<span class="build-badge kind">{t(`kinds.${k}`)}</span>{/each}
            {#if langsOf(r).length}
              <!-- i18n-ignore: language codes as the game's Translate folders name them -->
              <span class="build-badge kind mono" title={t("modTable.languagesTip", { languages: r.languages.join(", ") })}>{langsOf(r).slice(0, 3).join(" ")}{langsOf(r).length > 3 ? " …" : ""}</span>
            {/if}
            {#if neededBy.get(r.id)?.length}
              {@const by = neededBy.get(r.id)!}
              <span class="build-badge need" title={t("modTable.neededByTip", { names: by.map((x) => names.get(x) ?? x).join(", ") })}>{t("modTable.neededBy", { count: by.length })}</span>
            {/if}
            {#each tags[r.id] ?? [] as x}<span class="build-badge tag">{x}</span>{/each}
            {#if r.only41 && (build ?? 42) !== 41}
              <span class="build-badge" title={t("modTable.b41OnlyTip")}>{t("modTable.b41Only")}</span>
            {:else if r.only42 && build === 41}
              <span class="build-badge" title={t("modTable.b42OnlyTip")}>{t("modTable.b42Only")}</span>
            {/if}
          </td>
          <td class="meta mono">{stats[r.id] ? fmtSize(stats[r.id].size) : ""}</td>
          <td class="meta mono">{stats[r.id]?.first_seen ? fmtDate(stats[r.id].first_seen, "short") : ""}</td>
          <td class="meta mono">{stats[r.id] ? fmtDate(stats[r.id].modified, "short") : ""}</td>
          <td class="overlap">
            {#if r.on && overlaps[r.id]}
              {@const o = overlaps[r.id]}
              <span class="overlaps" title={overlapLabel(o)}><LayerIcon wins={o.wins_over.length > 0} loses={o.loses_to.length > 0} label={overlapLabel(o)} /><span class="mono">{fmtNum(new Set([...o.wins_over, ...o.loses_to]).size)}</span></span>
            {/if}
          </td>
          <td class="src">
            {#if r.missing}
              <span class="missing">{t("modTable.notInstalled")}</span>
            {:else if problems.has(r.id)}
              {@const p = problems.get(r.id)!}
              <span class="missing" title={t(p.key, { id: p.id })}><Icon name="alert" size={12} />{t(p.key, { id: p.id })}</span>
            {:else if updates.has(r.id)}
              <span class="update"><Icon name="update" size={12} />{t("modTable.updateAvailable")}</span>
            {:else if held.has(r.id) && heldNewer[r.id]}
              <span class="update held" title={t("modTable.heldChipTip", { date: fmtDate(heldNewer[r.id], "long") })}><Icon name="lock" size={12} />{t("modTable.held")}</span>
            {:else}
              {@const kind = r.source.split(":")[0]}
              {@const hint = t(SOURCE_HINTS[kind] ?? "modTable.sourceHint.manual")}
              {#if r.unsubscribed}
                <span class="badge" data-kind="unsubscribed" title={t("modTable.sourceHint.unsubscribed")}><Icon name="x" size={12} />{t("source.unsubscribed")}</span>
              {:else if (kind === "workshop" || kind === "steam") && src.url}
                <button class="badge go" data-kind={kind} title="{hint} · {t(kind === 'steam' ? 'modTable.unsubscribeTip' : 'modTable.subscribeTip')}"
                  onclick={(e) => { e.stopPropagation(); onSubscribe(r.id, kind !== "steam"); }}><Icon name={kind === "steam" ? "check" : "steam"} size={12} />{kind === "steam" ? t("source.steam") : t("source.workshop")}</button>
              {:else}
                <span class="badge" data-kind="local" title={src.url ? `${hint} · ${src.url}` : hint}><Icon name="folder" size={12} />{t("source.localShort")}</span>
              {/if}
            {/if}
          </td>
        </tr>
      {/if}
    {/each}
    {#if !filtering}
      <tr class="tail" class:drop={dropShown === rows.length} aria-hidden="true"
        ondragover={(e) => { e.preventDefault(); dropAt = rows.length; }}
        ondrop={(e) => { e.preventDefault(); drop(rows.length); }}>
        <td colspan="11"></td>
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
  :global(html.compact) tr.mod td {
    height: 30px;
  }
  tr.mod {
    cursor: default;
  }
  /* hovered, picked and selected rows look like a selected tab: the hover / selected overlay on the card
     under on-selected text, with a chevron on the left */
  tr.mod:hover td {
    background: linear-gradient(var(--hover), var(--hover)), var(--panel);
  }
  tr.mod.picked td,
  tr.mod.selected td {
    background: linear-gradient(var(--accent-soft), var(--accent-soft)), var(--panel);
  }
  tr.mod:hover td,
  tr.mod.picked td,
  tr.mod.selected td {
    color: var(--on-selected);
    /* the highlight's 1px hover-line edge: top and bottom on every cell, the sides on the end ones */
    box-shadow: inset 0 1px 0 var(--hover-line), inset 0 -1px 0 var(--hover-line);
  }
  :is(tr.mod:hover, tr.mod.picked, tr.mod.selected) td:first-child {
    box-shadow: inset 0 1px 0 var(--hover-line), inset 0 -1px 0 var(--hover-line), inset 1px 0 0 var(--hover-line);
  }
  :is(tr.mod:hover, tr.mod.picked, tr.mod.selected) td:last-child {
    box-shadow: inset 0 1px 0 var(--hover-line), inset 0 -1px 0 var(--hover-line), inset -1px 0 0 var(--hover-line);
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
    color: color-mix(in oklab, var(--on-selected) 80%, transparent);
  }
  /* pills and problems on a highlighted row take its text colour: muted grey and warning red fall under 4.5:1
     on Daylight's pale red */
  tr.mod:hover :is(.build-badge, .badge, .missing, .fav, .group-chip),
  tr.mod.picked :is(.build-badge, .badge, .missing, .fav, .group-chip),
  tr.mod.selected :is(.build-badge, .badge, .missing, .fav, .group-chip) {
    color: var(--on-selected);
  }
  /* the row the details show, among picked ones: a solid on-selected edge where the others have the hover line */
  tr.mod.selected td {
    box-shadow: inset 0 2px 0 var(--on-selected), inset 0 -2px 0 var(--on-selected);
  }
  tr.mod.selected td:first-child {
    box-shadow: inset 0 2px 0 var(--on-selected), inset 0 -2px 0 var(--on-selected), inset 2px 0 0 var(--on-selected);
  }
  tr.mod.selected td:last-child {
    box-shadow: inset 0 2px 0 var(--on-selected), inset 0 -2px 0 var(--on-selected), inset -2px 0 0 var(--on-selected);
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
    background-image: linear-gradient(to bottom, color-mix(in oklab, var(--accent) 85%, var(--on-accent)) 0, color-mix(in oklab, var(--accent) 55%, transparent) 3px, transparent 20px);
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
  /* the eye: faint until the row is pointed at, always there once the mod is hidden */
  .eye {
    width: 26px;
    padding-left: 0;
    padding-right: 0;
  }
  .eye button {
    padding: 2px;
    color: var(--muted);
    opacity: 0.4;
  }
  /* a held mod's lock: full text colour (white in the dark theme, as in the game), always there */
  .hold button.on {
    color: var(--text);
  }
  .eye button.on,
  tr.mod:hover .eye button,
  tr.mod:focus-within .eye button {
    opacity: 1;
  }
  /* the header row: sticks flush to the top of the list (.list has no top padding, which sticky would leave
     a strip of scrolled rows above it), opaque, over the rows' chevrons (z-index 1); the side padding is the
     cells', so the headings line up with them */
  thead th {
    position: sticky;
    top: 0;
    z-index: 2;
    background: var(--bg);
    padding-left: 8px;
    padding-right: 8px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    text-align: left;
    white-space: nowrap;
    padding-top: 4px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--line);
  }
  thead th.num,
  thead th.meta,
  thead th.src {
    text-align: right;
  }
  .head {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    color: inherit;
    text-transform: inherit;
    letter-spacing: inherit;
    cursor: pointer;
  }
  .head:hover,
  .head.on {
    color: var(--text);
  }
  .overlap-head {
    text-align: center;
  }
  .overlaps {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 12px;
    color: var(--muted);
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
  /* a mod's kinds: quieter than the build warnings */
  .build-badge.kind {
    font-weight: 600;
    color: var(--muted);
    border-color: var(--line);
  }
  /* new to the profile: the accent, to be spotted wherever it was filed */
  .build-badge.new {
    font-weight: 600;
    color: var(--accent-text);
    border-color: var(--accent-soft);
  }
  /* needed by an enabled mod: a fact, not a problem, so plain text rather than the warning red */
  .build-badge.need {
    color: var(--text);
    border-color: var(--line);
  }
  /* the player's own tags: pills */
  .build-badge.tag {
    font-weight: 600;
    color: var(--text);
    border-color: var(--line);
    border-radius: 999px;
    background: var(--raised);
  }
  /* a mod's group out of library order: the group's colour as an edge, its name muted, no fill (reads in both themes) */
  .group-chip {
    margin-left: 8px;
    padding: 0 6px;
    border-left: 3px solid var(--group);
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
  }
  .fav {
    color: var(--accent-text);
    margin-right: 4px;
    vertical-align: -1px;
  }
  tr.hidden-mod .title {
    font-style: italic;
    opacity: 0.7;
  }
  .meta {
    width: 78px;
    text-align: right;
    color: var(--muted);
    white-space: nowrap;
  }
  /* its own column so the icons line up whatever the badge beside them says */
  .overlap {
    width: 44px;
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
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 1px 8px;
  }
  /* Subscribed: Steam keeps it; Workshop: PZMM's copy of a Workshop item; both open its page.
     Local: anything from outside Steam (GitHub, a file, a folder) */
  .badge.go {
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    background: none;
  }
  .badge[data-kind="workshop"] {
    color: var(--text);
    border-color: currentColor;
  }
  .badge[data-kind="steam"] {
    color: var(--ok);
    border-color: color-mix(in oklab, var(--ok) 45%, transparent);
  }
  .badge.go:hover,
  .badge.go:focus-visible {
    color: var(--on-selected);
    background: var(--hover);
    border-color: var(--hover-line);
  }
  .update {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 8px;
    border: 1px solid color-mix(in oklab, var(--warn) 45%, transparent);
    border-radius: 999px;
    color: var(--warn);
    font-weight: 700;
    font-size: 12px;
  }
  /* held with a newer version: the update is there, muted, as nothing waits on it */
  .update.held {
    border-color: var(--line);
    color: var(--muted);
    font-weight: 600;
  }
  .missing {
    display: inline-block;
    max-width: 220px; /* the full text is its tooltip: the name keeps the room */
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
  /* a narrow list (a side panel open, long translations): size and date go before the name has to */
  @container (max-width: 760px) {
    .meta {
      display: none;
    }
    .missing {
      max-width: 140px;
    }
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
