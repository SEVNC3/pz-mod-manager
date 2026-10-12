<script lang="ts">
  import { tick, untrack, type Snippet } from "svelte";
  import type { LogLine } from "./api";
  import { fmtNum } from "./format";
  import { t } from "./i18n.svelte";
  import { isKind, KINDS, type Kind } from "./logkind";
  import Icon from "./Icon.svelte";

  // A log to read: filter, errors only, copy, and it stays at the newest line while you're there.
  // `focus`: a line (seq) to bring into view; `tools`: more controls at the start of the bar;
  // `chips`: pick kinds of line (errors, warnings, chat, admin, mods) instead of errors only; `id`: the log box's (tabs in `tools` control it).
  let { lines, status, label, focus = null, tools, chips = false, id }: { lines: LogLine[]; status: string; label: string; focus?: number | null; tools?: Snippet; chips?: boolean; id?: string } = $props();

  const SHOWN = 2000;
  let filter = $state("");
  let errorsOnly = $state(false);
  /** The kinds picked (none: every line). */
  let kinds = $state<Kind[]>([]);
  const counts = $derived(chips ? Object.fromEntries(KINDS.map((k) => [k, lines.filter((l) => isKind(l, k)).length])) : {});
  let copied = $state(false);
  let box = $state<HTMLElement>();
  /** At the bottom: new lines keep it there. */
  let following = true;

  const shown = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return lines.filter((l) => (!errorsOnly || l.level === "error" || l.note) && (!kinds.length || kinds.some((k) => isKind(l, k))) && (!f || l.text.toLowerCase().includes(f) || l.note?.text.toLowerCase().includes(f))).slice(-SHOWN);
  });

  $effect(() => {
    shown;
    if (following) tick().then(() => box?.scrollTo({ top: box.scrollHeight }));
  });

  $effect(() => {
    if (focus === null) return;
    const at = focus;
    if (!untrack(() => shown).some((l) => l.seq === at)) filter = "";
    following = false;
    tick().then(() => box?.querySelector(`[data-seq="${focus}"]`)?.scrollIntoView({ block: "center" }));
  });

  async function copyLog() {
    await navigator.clipboard.writeText(shown.map((l) => l.text).join("\n")).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<div class="tools">
  {@render tools?.()}
  <input type="search" bind:value={filter} placeholder={t("hosting.console.filter")} aria-label={t("hosting.console.filter")} />
  {#if chips}
    <span class="chips" role="group" aria-label={t("hosting.console.kinds")}>
      {#each KINDS as k}
        <button class="chip" aria-pressed={kinds.includes(k)} onclick={() => (kinds = kinds.includes(k) ? kinds.filter((x) => x !== k) : [...kinds, k])}>{t(`hosting.console.kind.${k}`)} <span class="hint">{fmtNum(counts[k])}</span></button>
      {/each}
    </span>
  {:else}
    <label class="check"><input type="checkbox" bind:checked={errorsOnly} />{t("hosting.console.errorsOnly")}</label>
  {/if}
  <span class="hint grow">{status}</span>
  <button onclick={copyLog} disabled={!shown.length}><Icon name="copy" size={14} />{copied ? t("hosting.console.copied") : t("hosting.console.copy")}</button>
</div>
<!-- svelte-ignore a11y_no_noninteractive_tabindex: a scrolling log takes focus so the keyboard can scroll it -->
<div class="log mono" role="log" aria-label={label} {id} bind:this={box} tabindex="0" onscroll={() => box && (following = box.scrollHeight - box.scrollTop - box.clientHeight < 60)}>
  {#each shown as l (l.seq)}
    <div class="line {l.level}" class:focus={l.seq === focus} data-seq={l.seq}>{l.text}</div>
    {#if l.note}<div class="note" class:bad={l.level === "error" || ["terminated", "badItem", "itemFailed", "modMissing", "scriptErrors"].includes(l.note.key)}><Icon name="info" size={12} />{t(`hosting.note.${l.note.key}`, l.note.vars)}</div>{/if}
  {:else}
    <p class="hint empty">{t("hosting.console.empty")}</p>
  {/each}
</div>

<style>
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  .chips {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .chip {
    padding: 2px 10px;
    border-radius: 999px;
    font-size: 12px;
  }
  .chip[aria-pressed="true"] {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-color: var(--hover-line);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .log {
    margin: 8px 0;
    height: var(--log-height, 340px);
    overflow: auto;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--bg);
    border: 1px solid var(--line);
    font-size: 11.5px;
    line-height: 1.5;
  }
  .line {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .line.error {
    color: var(--danger);
  }
  .line.warn {
    color: var(--map-over);
  }
  .line.focus {
    background: var(--accent-soft);
  }
  .note {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 2px 0 4px;
    padding: 3px 8px;
    border-radius: 4px;
    font-family: var(--font-body);
    font-size: 12px;
    background: var(--accent-soft);
    color: var(--text);
  }
  .note.bad {
    background: color-mix(in oklab, var(--danger) 22%, transparent);
  }
  .empty {
    margin: 0;
  }
</style>
