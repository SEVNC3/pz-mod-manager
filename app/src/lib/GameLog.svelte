<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api, type LogView as View } from "./api";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import { fmtNum } from "./format";
  import { trap } from "./ui";
  import Icon from "./Icon.svelte";
  import LogView from "./LogView.svelte";

  // The game's and servers' logs to read: each error once (how often, its trace, the mods it points
  // at), the whole log to search, and it keeps reading while the game writes.
  let { names, onClose, onSelect, onBisect }: {
    names: (id: string) => string;
    onClose: () => void;
    onSelect: (id: string) => void;
    /** Find the broken mod, testing first the mods a game log points at. */
    onBisect?: (ids: string[]) => void;
  } = $props();

  let files = $state<string[]>([]);
  let file = $state<string | null>(null);
  let view = $state<View | null>(null);
  let following = $state(true);
  let focus = $state<number | null>(null);
  let error = $state("");
  let loaded = $state(false);
  const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const older = $derived((view?.lines[0]?.seq ?? 0) > 0);
  /** The installed mods a game log (not a server's) points at. */
  const suspects = $derived(view && !/(server|coop)-console\.txt$/i.test(view.file) ? view.mods.flatMap((m) => (m.id ? [m.id] : [])) : []);

  /** `fresh`: read it whatever its size. */
  async function load(fresh = false) {
    try {
      const got = await api.gameLog(file, fresh ? 0 : (view?.size ?? 0));
      files = got.files;
      if (got.view) [view, file] = [got.view, got.view.file];
      error = "";
    } catch (e) {
      error = t(...keyedError(e));
    }
    loaded = true;
  }

  async function show(line: number) {
    focus = null;
    await tick();
    focus = line - 1;
  }

  function pick(id: string) {
    onSelect(id);
    onClose();
  }

  onMount(() => {
    load(true);
    const timer = setInterval(() => following && load(), 2000);
    return () => clearInterval(timer);
  });
</script>

<div class="backdrop" onclick={onClose} aria-hidden="true"></div>
<div class="panel" role="dialog" aria-modal="true" aria-label={t("gameLog.title")} tabindex="-1" use:trap={{ self: true }} onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), onClose())}>
  <header>
    <h2>{t("gameLog.title")}</h2>
    {#if files.length > 1}
      <select bind:value={file} onchange={() => { view = null; load(true); }} aria-label={t("gameLog.file")} title={t("gameLog.file")}>
        {#each files as f}<option value={f}>{fileName(f)}</option>{/each}
      </select>
    {/if}
    <label class="check" title={t("gameLog.followTip")}><input type="checkbox" bind:checked={following} onchange={() => following && load()} />{t("gameLog.follow")}</label>
    <button class="quiet icon" onclick={onClose} aria-label={t("common.close")} title={t("common.close")}><Icon name="x" size={16} /></button>
  </header>

  <div class="scroll">
    {#if error}<p class="error">{error}</p>{/if}
    {#if loaded && !files.length}
      <p class="hint">{t("gameLog.none")}</p>
    {:else if view}
      <h3>{view.groups.length ? t("gameLog.errors", { count: view.groups.length }) : t("gameLog.noErrors")}</h3>
      {#if view.groups.length}
        <ol class="groups">
          {#each view.groups as g}
            {@const [head, ...rest] = g.text.split("\n")}
            <li>
              <div class="row">
                <span class="count" title={t("gameLog.timesTip")}>×{fmtNum(g.count)}</span>
                <span class="hint">{g.count === 1 ? t("gameLog.once", { line: g.line }) : t("gameLog.times", { line: g.line, last: g.last })}</span>
                {#if g.ids.length}
                  <span class="hint">{t("findings.pointsAt")}</span>
                  <!-- the mods as a list in the app's language ("A, B and C"), each a button -->
                  <span>{#each new Intl.ListFormat(intlLang(), { type: "conjunction" }).formatToParts(g.ids) as p}{#if p.type === "element"}<button class="quiet link" onclick={() => pick(p.value)}>{names(p.value)}</button>{:else}{p.value}{/if}{/each}</span>
                {:else}
                  <span class="hint">{t("gameLog.noMod")}</span>
                {/if}
                <button class="quiet small" onclick={() => show(g.line)} disabled={(view?.lines[0]?.seq ?? 0) > g.line - 1}>{t("gameLog.showInLog")}</button>
              </div>
              {#if rest.length}
                <details>
                  <summary class="mono">{head}</summary>
                  <pre class="mono">{rest.join("\n")}</pre>
                </details>
              {:else}
                <p class="mono one">{head}</p>
              {/if}
            </li>
          {/each}
        </ol>
      {/if}
      {#if view.mods.length}
        <p class="by-mod">
          <span class="hint">{t("gameLog.byMod")}</span>
          {#each view.mods as m}
            <button class="quiet link" disabled={!m.id} onclick={() => m.id && pick(m.id)}>{m.name} ×{fmtNum(m.count)}</button>
          {/each}
        </p>
      {/if}
      {#if onBisect && suspects.length}<p><button onclick={() => onBisect(suspects)}><Icon name="wand" size={14} />{t("findings.findBroken")}</button></p>{/if}
      <div class="viewer">
        <LogView lines={view.lines} {focus} status={older ? t("gameLog.older", { count: view.lines.length }) : fileName(view.file)} label={t("gameLog.title")} />
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.45);
    z-index: 32; /* over Check setup, which opens it */
  }
  .panel:focus {
    outline: none;
  }
  .panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(980px, 96vw);
    z-index: 33;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-left: 1px solid var(--line);
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0;
    font-size: 20px;
    flex: 1;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
  }
  .icon {
    padding: 4px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .scroll {
    overflow-y: auto;
    padding: 12px 16px 24px;
  }
  .groups {
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
    max-height: 34vh;
    overflow-y: auto;
  }
  .groups li {
    padding: 6px 0;
    border-bottom: 1px solid var(--line);
  }
  .row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .count {
    font-weight: 700;
    color: var(--danger);
    min-width: 2.5em;
  }
  .link {
    padding: 0 4px;
  }
  /* the hover fill's padding, without pushing the list's commas away */
  .row .link {
    padding: 0 2px;
    margin: 0 -2px;
  }
  .small {
    padding: 1px 10px;
    font-size: 12px;
    margin-left: auto;
  }
  summary,
  .one {
    margin: 4px 0 0;
    font-size: 12px;
    overflow-wrap: anywhere;
    cursor: pointer;
  }
  .one {
    cursor: default;
  }
  pre {
    margin: 4px 0 0;
    padding: 6px 8px;
    background: var(--raised);
    border-radius: 6px;
    font-size: 12px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 16em;
    overflow-y: auto;
  }
  .by-mod {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin: 0 0 6px;
  }
  .viewer {
    --log-height: 46vh;
  }
</style>
