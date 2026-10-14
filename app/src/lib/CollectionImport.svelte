<script lang="ts">
  import type { Pack } from "./api";
  import { fmtNum } from "./format";
  import { intlLang, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import { trap } from "./ui";

  // Workshop collections, read before anything is installed, to choose from: into the active profile, or a new one.
  let { pack, asProfile = false, onImport, onAdd, onCancel }: {
    pack: Pack;
    /** A new profile is the main choice (Import), else adding to the active one (Browse Workshop). */
    asProfile?: boolean;
    /** The items to install and use as a new profile, and the mods to leave out. */
    onImport: (items: string[], skip: string[]) => void;
    /** The items to install into the active profile, and the mods to leave out. */
    onAdd: (items: string[], skip: string[]) => void;
    onCancel: () => void;
  } = $props();

  const mods = (item: string) => pack.item_mods[item] ?? [];
  // svelte-ignore state_referenced_locally
  let on = $state<Record<string, boolean>>(Object.fromEntries(pack.items.flatMap(mods).map((m) => [m, pack.mods.includes(m)])));
  let onlyHave = $state(false);

  const title = (item: string) => pack.titles[item] || item;
  /** Items with alternative versions or optional parts: the player picks. */
  const choices = $derived(pack.items.filter((i) => mods(i).length > 1));
  /** A delisted item still counts when it's installed. */
  const gone = $derived(pack.delisted.filter((i) => !pack.installed.includes(i)));
  const items = $derived(pack.items.filter((i) => !gone.includes(i) && (!onlyHave || pack.installed.includes(i)) && (!mods(i).length || mods(i).some((m) => on[m]))));
  const skip = $derived(Object.keys(on).filter((m) => !on[m]));
  const count = $derived(items.flatMap(mods).filter((m) => on[m]).length);
</script>

<div class="dim" onclick={onCancel} aria-hidden="true"></div>
<div class="pick" role="dialog" tabindex="-1" aria-modal="true" aria-label={t("collection.title", { title: pack.title || t("collection.untitled") })} use:trap
  onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), onCancel())}>
  <p><strong>{t("collection.title", { title: pack.title || t("collection.untitled") })}</strong></p>
  <p class="hint">{t("collection.summary", { items: fmtNum(pack.items.length), mods: fmtNum(pack.mods.length), have: fmtNum(pack.installed.length) })}</p>
  {#if pack.installed.length < pack.items.length}
    <label class="check"><input type="checkbox" bind:checked={onlyHave} />{t("collection.onlyHave", { count: pack.installed.length })}</label>
  {/if}
  {#if gone.length}
    <p class="warn"><Icon name="alert" size={14} />{t("collection.delisted", { count: gone.length, items: new Intl.ListFormat(intlLang(), { type: "unit" }).format(gone.map(title)) })}</p>
  {/if}
  {#if choices.length}
    <p>{t("collection.choose")}</p>
    <div class="choices">
      {#each choices as i (i)}
        <fieldset disabled={onlyHave && !pack.installed.includes(i)}>
          <legend>{title(i)}</legend>
          {#each mods(i) as m (m)}
            <label class="check"><input type="checkbox" bind:checked={on[m]} /><span class="mono">{m}</span></label>
          {/each}
        </fieldset>
      {/each}
    </div>
  {/if}
  {#if pack.notes.length}
    <details>
      <summary>{t("collection.notes", { count: pack.notes.length })}</summary>
      <ul>{#each pack.notes as n}<li>{n}</li>{/each}</ul>
    </details>
  {/if}
  <div class="row">
    {#snippet add(primary: boolean)}
      <button class:primary onclick={() => onAdd(items, skip)} disabled={!items.length}><Icon name="download" size={14} />{t("collection.add", { count })}</button>
    {/snippet}
    {#if !asProfile}{@render add(true)}{/if}
    <button class:primary={asProfile} onclick={() => onImport(items, skip)} disabled={!items.length}><Icon name="plus" size={14} />{t("collection.import", { count })}</button>
    {#if asProfile}{@render add(false)}{/if}
    <button class="quiet" onclick={onCancel}><Icon name="x" size={14} />{t("common.cancel")}</button>
  </div>
</div>

<style>
  /* a box in the middle over the dimmed window, like every question */
  .dim {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: rgb(0 0 0 / 0.45);
  }
  .pick {
    position: fixed;
    z-index: 31;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    overflow: auto;
    margin: 0;
    padding: 20px 22px;
    border-radius: 10px;
    background: var(--panel);
    box-shadow: var(--shadow);
  }
  .pick p {
    margin: 0 0 6px;
  }
  .row {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .warn {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    color: var(--map-over);
  }
  .choices {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 8px;
    max-height: 320px;
    overflow: auto;
    margin-bottom: 6px;
  }
  fieldset {
    margin: 0;
    padding: 4px 10px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    min-width: 0;
  }
  legend {
    padding: 0 4px;
    overflow-wrap: anywhere;
  }
  ul {
    margin: 4px 0;
    padding-left: 18px;
    overflow-wrap: anywhere;
  }
</style>
