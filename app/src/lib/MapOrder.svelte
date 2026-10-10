<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { sortMaps } from "./order";

  // a Map= line (Servers & saves, Maps): drag or Alt+arrows to arrange
  let { maps = $bindable() }: { maps: string[] } = $props();

  let mapFrom = $state<number | null>(null);
  let mapAt = $state<number | null>(null);

  /** The map at `from` dropped before position `to`. */
  function moveMap(from: number, to: number) {
    if (to === from || to === from + 1 || to < 0 || to > maps.length) return;
    const next = [...maps];
    const [m] = next.splice(from, 1);
    next.splice(to > from ? to - 1 : to, 0, m);
    maps = next;
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
    <li role="option" aria-selected="false" class:drop={mapAt === i} class:dragging={mapFrom === i} draggable="true" tabindex="0"
      onkeydown={(e) => mapKey(e, i)}
      ondragstart={() => (mapFrom = i)} ondragend={() => { mapFrom = mapAt = null; }}
      ondragover={(e) => { e.preventDefault(); mapAt = i; }}
      ondrop={(e) => { e.preventDefault(); dropMap(i); }}>
      <span class="num mono">{i + 1}</span>{m}
      {#if i > 0 && m !== "Muldraugh, KY"}<button class="quiet top" onclick={() => moveMap(i, 0)} title={t("mapOrder.topTip")} aria-label={t("mapOrder.topLabel", { name: m })}>{t("mapOrder.top")}</button>{/if}
    </li>
  {/each}
  <li class="tail" class:drop={mapAt === maps.length} aria-hidden="true"
    ondragover={(e) => { e.preventDefault(); mapAt = maps.length; }}
    ondrop={(e) => { e.preventDefault(); dropMap(maps.length); }}></li>
</ol>
<button class="quiet" onclick={() => (maps = sortMaps(maps))} title={t("mapOrder.autosortTip")}><Icon name="wand" size={14} />{t("mapOrder.autosort")}</button>

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
  .top {
    margin-left: auto;
    padding: 0 6px;
    font-size: 12px;
  }
</style>
