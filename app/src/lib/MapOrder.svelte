<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { autosortMaps, BASE_MAP as BASE, sortMaps } from "./order";

  // a Map= line (Servers & saves, Maps): drag, Alt+arrows or the buttons to arrange; Muldraugh stays last
  // `onarrange`: the player moved something (not called when `maps` is set from outside)
  // `lotsOf`: a folder's map.info lots= (the maps it builds on), for Autosort
  // `titleOf`, `clashesOf`: a folder's map.info title and the maps it shares cells with (shown when given)
  let { maps = $bindable(), onarrange, lotsOf = () => [], titleOf, clashesOf }: {
    maps: string[];
    onarrange?: (maps: string[]) => void;
    lotsOf?: (folder: string) => string[];
    titleOf?: (folder: string) => string | undefined;
    clashesOf?: (folder: string) => string[];
  } = $props();

  let mapFrom = $state<number | null>(null);
  let mapAt = $state<number | null>(null);

  function arrange(next: string[]) {
    maps = sortMaps(next);
    onarrange?.(maps);
  }

  /** The map at `from` dropped before position `to`. */
  function moveMap(from: number, to: number) {
    if (to === from || to === from + 1 || to < 0 || to > maps.length || maps[from] === BASE) return;
    const next = [...maps];
    const [m] = next.splice(from, 1);
    next.splice(to > from ? to - 1 : to, 0, m);
    arrange(next);
  }

  /** Alt+arrows move the focused map (and keep it focused). */
  async function mapKey(e: KeyboardEvent, i: number) {
    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    e.preventDefault();
    const li = e.currentTarget as HTMLElement;
    moveMap(i, e.key === "ArrowUp" ? i - 1 : i + 2);
    await tick();
    li.focus();
  }

  function dropMap(to: number) {
    if (mapFrom !== null) moveMap(mapFrom, to);
    mapFrom = mapAt = null;
  }
</script>

<ol class="drag-list maps" role="listbox" aria-label={t("mapOrder.label")}>
  {#each maps as m, i (m)}
    {@const fixed = m === BASE}
    {@const title = titleOf?.(m)}
    {@const clash = clashesOf?.(m) ?? []}
    <li role="option" aria-selected="false" class:drop={mapAt === i} class:dragging={mapFrom === i} class:fixed draggable={!fixed} tabindex="0"
      title={fixed ? t("mapOrder.fixedTip") : undefined}
      onkeydown={(e) => mapKey(e, i)}
      ondragstart={() => (mapFrom = i)} ondragend={() => { mapFrom = mapAt = null; }}
      ondragover={(e) => { e.preventDefault(); mapAt = i; }}
      ondrop={(e) => { e.preventDefault(); dropMap(i); }}>
      <span class="num mono">{i + 1}</span>{#if title && title !== m}{title} <span class="folder mono">{m}</span>{:else}{m}{/if}
      {#if clash.length}<span class="clash" title={clash.join("\n")}>{t("mapOrder.clashes", { count: clash.length })}</span>{/if}
      {#if fixed}<span class="pin"><Icon name="lock" size={12} />{t("mapOrder.fixed")}</span>
      {:else}
        <span class="moves">
          {#if i > 0}<button class="quiet" onclick={() => moveMap(i, 0)} title={t("mapOrder.topTip")} aria-label={t("mapOrder.topLabel", { name: m })}>{t("mapOrder.top")}</button>{/if}
          <button class="quiet" disabled={i === 0} onclick={() => moveMap(i, i - 1)} title={t("mapOrder.upTip")} aria-label={t("mapOrder.upLabel", { name: m })}><Icon name="chevron-up" size={14} /></button>
          <button class="quiet" disabled={maps[i + 1] === undefined || maps[i + 1] === BASE} onclick={() => moveMap(i, i + 2)} title={t("mapOrder.downTip")} aria-label={t("mapOrder.downLabel", { name: m })}><Icon name="chevron-down" size={14} /></button>
        </span>
      {/if}
    </li>
  {/each}
  <li class="tail" class:drop={mapAt === maps.length} aria-hidden="true"
    ondragover={(e) => { e.preventDefault(); mapAt = maps.length; }}
    ondrop={(e) => { e.preventDefault(); dropMap(maps.length); }}></li>
</ol>
<button class="quiet" onclick={() => arrange(autosortMaps(maps, lotsOf))} title={t("mapOrder.autosortTip")}><Icon name="wand" size={14} />{t("mapOrder.autosort")}</button>

<style>
  .maps {
    margin-bottom: 6px;
  }
  .maps .num {
    width: 20px;
    text-align: right;
    color: var(--accent-text);
  }
  .maps li:hover .num {
    color: var(--on-selected);
  }
  .folder {
    font-size: 12px;
    color: var(--muted);
  }
  .clash {
    font-size: 11px;
    padding: 0 5px;
    border-radius: 4px;
    color: var(--danger);
    border: 1px solid currentColor;
  }
  .maps li:hover :is(.folder, .clash) {
    color: inherit;
  }
  .moves,
  .pin {
    margin-left: auto;
    font-size: 12px;
  }
  .moves {
    display: inline-flex;
    gap: 2px;
  }
  .moves button {
    padding: 0 6px;
  }
  .pin {
    padding: 0 6px;
  }
  .maps li.fixed {
    cursor: default;
  }
  .pin {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
  }
  .maps li.fixed:hover .pin {
    color: inherit;
  }
</style>
