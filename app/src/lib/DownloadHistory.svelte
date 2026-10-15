<script lang="ts">
  import { onMount } from "svelte";
  import { api, type HistoryRecord } from "./api";
  import { confirmed } from "./ask.svelte";
  import { fmtAgo } from "./format";
  import { t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  /** What PZMM downloaded, updated and removed (kept on disk): put a mod back, take one out, or forget it. */
  let { installed, onReinstall, onRemove, onClose }: {
    /** Workshop items installed now. */
    installed: Set<string>;
    onReinstall: (item: string, steam: boolean) => Promise<unknown>;
    onRemove: (r: HistoryRecord) => Promise<unknown>;
    onClose: () => void;
  } = $props();

  let records = $state<HistoryRecord[] | null>(null);
  const load = () => api.historyList().then((r) => (records = r)).catch(() => (records = []));
  onMount(load);

  const item = (r: HistoryRecord) => r.source.match(/^(?:workshop|steam):(\d+)$/)?.[1] ?? "";
  /** One row per mod: its newest record. */
  const latest = $derived((records ?? []).filter((r, i, all) => all.findIndex((o) => o.id === r.id) === i));

  async function clearAll() {
    if (await confirmed(t("history.clearAsk"), { label: t("history.clear"), icon: "trash" })) records = await api.historyClear().catch(() => records);
  }
</script>

<Modal title={t("activity.history")} width={720} height={640} {onClose}>
  {#if records === null}
    <p class="empty">{t("activity.wait")}…</p>
  {:else if !latest.length}
    <p class="empty">{t("history.none")}</p>
  {:else}
    <ul class="list">
      {#each latest as r (r.id)}
        {@const it = item(r)}
        {@const have = r.action !== "removed" && (!it || installed.has(it))}
        <li>
          <div class="what">
            <strong>{r.name || r.id}</strong>
            <span class="meta">{t(`history.action.${r.action}`)} · {fmtAgo(r.at)} · <span class="mono">{r.id}</span></span>
          </div>
          <div class="actions">
            {#if it && !have}
              <button onclick={() => onReinstall(it, r.source.startsWith("steam:")).then(load)}><Icon name="download" size={14} />{t("activity.reinstall")}</button>
            {:else if have}
              <button class="quiet" onclick={() => onRemove(r).then(load)}><Icon name="trash" size={14} />{t("common.remove")}</button>
            {/if}
            <button class="quiet" onclick={() => api.historyClear([r.id]).then((got) => (records = got)).catch(() => {})} title={t("history.forget")} aria-label={t("history.forget")}><Icon name="eye-off" size={14} /></button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet foot()}
    <span class="muted kept">{t("history.kept")}</span>
    <button class="quiet" onclick={clearAll} disabled={!records?.length}><Icon name="trash" size={14} />{t("history.clear")}</button>
  {/snippet}
</Modal>

<style>
  .kept {
    flex: 1;
    font-size: 12px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 6px 10px;
    overflow-y: auto;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px;
    border-bottom: 1px solid var(--line);
  }
  .what {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .what strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    font-size: 12px;
    color: var(--muted);
  }
  .mono {
    font-family: var(--font-mono);
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .empty {
    padding: 28px;
    text-align: center;
    color: var(--muted);
  }
  .muted {
    color: var(--muted);
  }
</style>
