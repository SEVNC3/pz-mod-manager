<script lang="ts">
  import { api, type WorkshopItem } from "./api";
  import { KIND_ICONS, type LogEntry, type LogKind } from "./activity.svelte";
  import { fmtDateTime, fmtNum, workshopUrl } from "./format";
  import { t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  /** The activity in a large window: filter by kind, and see what a line is about (a removed mod's
   * Workshop listing with Reinstall, an error in full). */
  let { log, installed, onReinstall, onClose }: {
    log: LogEntry[];
    /** Workshop items installed now (Reinstall shows only for the others). */
    installed: Set<string>;
    onReinstall: (item: string, steam: boolean) => void;
    onClose: () => void;
  } = $props();

  const FILTERS: ("all" | LogKind)[] = ["all", "install", "update", "remove", "error"];
  let filter = $state<"all" | LogKind>("all");
  const shown = $derived(log.filter((l) => filter === "all" || l.kind === filter).toReversed());
  /** The line picked (itself, not its place: the log drops old lines as new ones come). */
  let entry = $state<LogEntry | null>(null);

  /** The picked line's Workshop listings. */
  let items = $state<WorkshopItem[]>([]);
  $effect(() => {
    const ids = entry?.items ?? [];
    items = [];
    // a slow answer for a line picked before doesn't replace this one's
    if (ids.length) api.workshopItems(ids).then((got) => entry?.items === ids && (items = got)).catch(() => {});
  });
</script>

<Modal title={t("activity.title")} width={1000} height={680} fixed {onClose}>
  {#snippet head()}
    <div class="chips" role="group" aria-label={t("activity.filter")}>
      {#each FILTERS as f}
        <button class="chip" aria-pressed={filter === f} onclick={() => ((filter = f), (entry = null))}>{t(`activity.kind.${f}`)}</button>
      {/each}
    </div>
  {/snippet}
  <div class="body">
    <ol class="rows">
      {#each shown as l (l)}
        <li>
          <button class="row {l.kind}" aria-current={entry === l || undefined} onclick={() => (entry = l)}>
            <span class="time">{fmtDateTime(l.time / 1000)}</span>
            <Icon name={KIND_ICONS[l.kind]} size={14} />
            <span class="text">{l.text}</span>
          </button>
        </li>
      {:else}
        <li class="empty">{t("activity.empty")}</li>
      {/each}
    </ol>
    <aside class="detail" aria-live="polite">
      {#if !entry}
        <p class="muted">{t("activity.pickLine")}</p>
      {:else}
        {@const sel = entry}
        <p class="full" class:error={sel.kind === "error"}>{sel.text}</p>
        <p class="muted small">{fmtDateTime(sel.time / 1000)}</p>
        {#each items as it (it.id)}
          <div class="card">
            {#if it.preview_url}<img src={it.preview_url} alt="" loading="lazy" />{/if}
            <div>
              <strong>{it.title}</strong>
              <p class="muted small">{t("details.subscribers", { count: it.subscriptions, n: fmtNum(it.subscriptions) })}</p>
              <div class="actions">
                {#if !installed.has(it.id)}
                  <button class="primary" onclick={() => onReinstall(it.id, String(sel.vars.source ?? "").startsWith("steam:") || !!sel.key?.endsWith("subscribed"))}><Icon name="download" size={14} />{t("activity.reinstall")}</button>
                {/if}
                <button class="quiet" onclick={() => api.openUrl(workshopUrl(it.id))}><Icon name="steam" size={14} />{t("details.workshopPage")}</button>
              </div>
            </div>
          </div>
        {/each}
        {#if sel.kind === "error"}
          <button class="quiet" onclick={() => navigator.clipboard.writeText(sel.text).catch(() => {})}><Icon name="copy" size={14} />{t("activity.copy")}</button>
        {/if}
        {#if sel.url}<button class="quiet" onclick={() => api.openUrl(sel.url!)}><Icon name="external" size={14} />{t("activity.whatChanged")}</button>{/if}
      {/if}
    </aside>
  </div>
</Modal>

<style>
  .chips {
    display: flex;
    gap: 6px;
    flex: 1;
    flex-wrap: wrap;
  }
  .chip {
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 12px;
  }
  .chip[aria-pressed="true"] {
    background: var(--accent-soft);
    color: var(--text);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 8px;
    overflow-y: auto;
  }
  .row {
    display: grid;
    grid-template-columns: max-content 14px minmax(0, 1fr);
    white-space: normal;
    gap: 10px;
    align-items: start;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-left: 2px solid transparent;
    border-radius: 4px;
    background: none;
    text-align: left;
    font-size: 14px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .row[aria-current] {
    background: var(--hover);
  }
  .row :global(.icon) {
    margin-top: 3px;
    color: var(--muted);
  }
  .row.install :global(.icon),
  .row.update :global(.icon) {
    color: var(--accent-text);
  }
  .row.error {
    border-left-color: var(--danger);
    color: var(--danger);
  }
  .row.error :global(.icon) {
    color: var(--danger);
  }
  .time {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    font-size: 11px;
    color: var(--muted);
    padding-top: 3px;
  }
  .empty {
    padding: 24px;
    text-align: center;
    color: var(--muted);
  }
  .detail {
    padding: 16px;
    border-left: 1px solid var(--line);
    overflow-y: auto;
  }
  .full {
    margin: 0 0 4px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .full.error {
    color: var(--danger);
  }
  .small {
    font-size: 12px;
    margin: 0 0 12px;
  }
  .muted {
    color: var(--muted);
  }
  .card {
    display: grid;
    gap: 10px;
    margin-bottom: 12px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .card img {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    border-radius: 6px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
</style>
