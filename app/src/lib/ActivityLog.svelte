<script lang="ts">
  import { tick } from "svelte";
  import { api } from "./api";
  import { KIND_ICONS, type LogEntry } from "./activity.svelte";
  import { fmtNum, fmtTime } from "./format";
  import { t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";

  let { log, busy, busyLabel, progress, open = $bindable(), onCancel, onCopy, onExpand, onHistory }: {
    log: LogEntry[];
    busy: boolean;
    busyLabel: string;
    /** Workshop items downloaded, of how many (while a download runs). */
    progress: [number, number] | null;
    open: boolean;
    /** Stop the download: what finished still installs. */
    onCancel: () => void;
    onCopy: () => void;
    /** The activity in a large window, with what each line is about. */
    onExpand: () => void;
    onHistory: () => void;
  } = $props();

  /** Repeats of the same line in a row show once, with a count. */
  const rows = $derived.by(() => {
    const out: (LogEntry & { times: number })[] = [];
    for (const l of log) {
      const prev = out.at(-1);
      if (prev && prev.text === l.text && prev.kind === l.kind) out[out.length - 1] = { ...prev, time: l.time, times: prev.times + 1 };
      else out.push({ ...l, times: 1 });
    }
    return out;
  });
  const last = $derived(log.at(-1));
  /** An error came in since the list was last open: the dot says so until it's opened. */
  let seen = $state(0);
  $effect(() => {
    if (open) seen = log.at(-1)?.time ?? 0;
  });
  // by time, not count: the log keeps a fixed number of lines, so its length stops growing
  const unseenError = $derived(!open && log.some((l) => l.time > seen && l.kind === "error"));
  /** A count fills the bar; one item at 0 of 1 would just sit empty. */
  const counted = $derived(!!progress && progress[1] > 1);

  // new lines scroll into view, unless the player scrolled up to read
  let list = $state<HTMLOListElement>();
  let pinned = true;
  $effect.pre(() => {
    void rows.length;
    if (list) pinned = list.scrollHeight - list.scrollTop - list.clientHeight < 24;
  });
  $effect(() => {
    void rows.length;
    if (open && list && pinned) tick().then(() => list && (list.scrollTop = list.scrollHeight));
  });
</script>

<footer class="activity" class:open>
  <div class="head">
    <button class="quiet toggle" aria-expanded={open} aria-controls="activity-list" onclick={() => (open = !open)}>
      <Icon name={open ? "chevron-down" : "chevron-up"} size={14} />
      <span class="dot" class:busy class:alert={unseenError} aria-hidden="true"></span>
      <span class="label">{busy ? (busyLabel || t("activity.wait")) + "…" : t("activity.title")}</span>
    </button>
    {#if busy}
      <!-- a count fills it; without one it's the moving kind, so a slow job reads as working, not stuck -->
      {#if counted}<progress value={progress![0]} max={progress![1]} aria-label={busyLabel}></progress>{:else}<progress aria-label={busyLabel}></progress>{/if}
      {#if progress}<span class="count">{t("activity.progress", { done: fmtNum(progress[0]), total: fmtNum(progress[1]) })}</span>{/if}
    {/if}
    <span class="last" class:error={last?.kind === "error"}>{last?.text ?? ""}</span>
    {#if busy && progress}
      <button class="quiet" onclick={onCancel} title={t("activity.cancelTip")}><Icon name="x" size={14} />{t("common.cancel")}</button>
    {/if}
    <button class="quiet icon" onclick={onHistory} title={t("activity.history")} aria-label={t("activity.history")}><Icon name="download" size={14} /></button>
    <button class="quiet icon" onclick={onExpand} title={t("activity.expand")} aria-label={t("activity.expand")}><Icon name="maximize" size={14} /></button>
    <button class="quiet icon" onclick={onCopy} disabled={!log.length} title={t("activity.copyTip")} aria-label={t("activity.copy")}><Icon name="copy" size={14} /></button>
  </div>
  <!-- read out: the newest line, errors at once -->
  <span class="sr" aria-live={last?.kind === "error" ? "assertive" : "polite"}>{last?.text ?? ""}</span>
  {#if open}
    <ol class="log" id="activity-list" bind:this={list}>
      {#each rows as l, i (i)}
        <li class={l.kind}>
          <span class="time">{fmtTime(l.time)}</span>
          <Icon name={KIND_ICONS[l.kind]} size={14} />
          <span class="text">{l.text}{#if l.url}<button class="quiet link" onclick={() => api.openUrl(l.url!)}>{t("activity.whatChanged")}</button>{/if}</span>
          {#if l.times > 1}<span class="times">×{l.times}</span>{/if}
        </li>
      {:else}
        <li class="empty">{t("activity.empty")} <button class="link" onclick={onHistory}>{t("activity.history")}</button></li>
      {/each}
    </ol>
  {/if}
</footer>

<style>
  .activity {
    border-top: 1px solid var(--line);
    background: var(--panel);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 8px 0 4px;
  }
  .toggle {
    min-width: 0;
    flex-shrink: 1;
    max-width: 40%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 6px;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 700;
    color: var(--text);
  }
  progress {
    flex: 0 1 120px;
    min-width: 48px;
    height: 6px;
    accent-color: var(--accent-text);
  }
  .count,
  .time,
  .times {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  .count {
    white-space: nowrap;
    font-size: 12px;
    color: var(--muted);
  }
  .last {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    color: var(--muted);
  }
  .last.error {
    color: var(--danger);
  }
  .icon {
    padding: 6px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
    opacity: 0.6;
  }
  .dot.busy {
    opacity: 1;
    background: var(--accent-text);
    animation: pulse 1s ease-in-out infinite alternate;
  }
  .dot.alert {
    opacity: 1;
    background: var(--danger);
    animation: none;
  }
  @keyframes pulse {
    to {
      opacity: 0.3;
    }
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .log {
    list-style: none;
    margin: 0;
    padding: 4px 8px 10px;
    max-height: 200px;
    overflow-y: auto;
    border-top: 1px solid var(--line);
  }
  /* time, icon, text, repeat count */
  .log li {
    display: grid;
    grid-template-columns: max-content 14px minmax(0, 1fr) auto;
    align-items: start;
    gap: 10px;
    padding: 3px 8px;
    border-left: 2px solid transparent;
    border-radius: 4px;
    font-size: 13px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .log li :global(.icon) {
    margin-top: 2px;
    color: var(--muted);
  }
  .log li.install :global(.icon),
  .log li.update :global(.icon) {
    color: var(--accent-text);
  }
  .log li.error {
    border-left-color: var(--danger);
    background: color-mix(in oklab, var(--danger) 10%, transparent);
    color: var(--danger);
  }
  .log li.error :global(.icon) {
    color: var(--danger);
  }
  .time {
    font-size: 11px;
    color: var(--muted);
    padding-top: 2px;
  }
  .times {
    font-size: 11px;
    color: var(--muted);
  }
  .link {
    margin-left: 6px;
  }
  .log li.empty {
    display: block;
    padding: 14px 8px;
    text-align: center;
    color: var(--muted);
  }
  @media (prefers-reduced-motion: reduce) {
    .dot.busy {
      animation: none;
    }
  }
</style>
