<script lang="ts">
  import { track } from "./activity.svelte";
  import { onMount } from "svelte";
  import { api, type BackupView, type ServerBackup, type ServerCard } from "./api";
  import { fmtDateTime, fmtSize } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";

  let { server, ask }: {
    server: ServerCard;
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
  } = $props();

  let view = $state<BackupView | null>(null);
  let error = $state("");
  let done = $state("");
  let working = $state(false);
  const REASONS = ["manual", "start", "update", "scheduled", "restore", "wipe", "chunks", "migrate", "import"];

  const load = () =>
    api
      .hostBackups(server.name)
      .then((v) => (view = v))
      .catch((e) => (error = t(...keyedError(e))));

  async function run(f: () => Promise<unknown>, ok: string) {
    error = done = "";
    working = true;
    try {
      await f();
      if (ok) done = ok;
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      working = false;
      load();
    }
  }

  async function restore(b: ServerBackup) {
    const when = fmtDateTime(b.time);
    const lines = [
      ...(server.external ? [t("hosting.backups.restoreExternal", { kind: t(`hosting.external.${server.external.kind}`) })] : server.status === "stopped" || server.status === "crashed" ? [] : [t("hosting.backups.restoreStops")]),
      t("hosting.backups.restoreKeeps"),
    ];
    if ((await ask(t("hosting.backups.restoreTitle", { name: server.name, when }), lines, [{ label: t("hosting.backups.restore"), icon: "rollback" }])) !== 0) return;
    run(() => api.hostRestore(server.name, b.file), t("hosting.backups.restored", { when }));
  }

  async function exportTo(b: ServerBackup) {
    const dir = await api.pickFolder("");
    if (dir) run(async () => (done = t("hosting.backups.exported", { file: await api.hostBackupExport(server.name, b.file, dir) })), "");
  }

  async function importZip() {
    const [file] = await api.pickInstall(false);
    if (file) run(() => api.hostBackupImport(server.name, file), t("hosting.backups.imported"));
  }

  const reason = (r: string) => (REASONS.includes(r) ? t(`hosting.backups.reason.${r}`) : r);

  onMount(() => {
    load();
  });

  // the activity bar moves while this tab works
  $effect(() => {
    if (working) return track();
  });
</script>

<div class="backups">
  {#if view?.empty_cells.length}
    <div class="warn" role="alert">
      <Icon name="alert" size={14} />
      <span>{t("hosting.backups.emptyCells", { count: view.empty_cells.length })} <span class="mono">{new Intl.ListFormat(intlLang(), { type: "unit" }).format(view.empty_cells.slice(0, 5))}{view.empty_cells.length > 5 ? "…" : ""}</span></span>
    </div>
  {/if}
  <div class="row">
    <button class="primary" onclick={() => run(() => api.hostBackup(server.name), t("hosting.backups.done"))} disabled={working}><Icon name="download" size={14} />{t("hosting.backups.now")}</button>
    <button onclick={importZip} disabled={working} title={t("hosting.backups.importTip")}><Icon name="upload" size={14} />{t("hosting.backups.import")}</button>
    <span class="hint">{t("hosting.backups.what")}</span>
  </div>
  {#if view}
    <p class="hint">
      {view.game_on_start ? t("hosting.backups.gameOn", { count: view.game_count || "5" }) : t("hosting.backups.gameOff")}
      <span class="mono">{view.game_dir}</span>
    </p>
    {#if !view.backups.length}
      <p class="hint">{t("hosting.backups.none")}</p>
    {:else}
      <ul class="list" aria-label={t("hosting.backups.listLabel")}>
        {#each view.backups as b (b.file)}
          <li>
            <span><strong>{fmtDateTime(b.time)}</strong> <span class="hint">{fmtSize(b.size)} · {reason(b.reason)}</span></span>
            <span class="btns">
              <button class="quiet" onclick={() => exportTo(b)} disabled={working} title={t("hosting.backups.exportTip")}><Icon name="download" size={14} />{t("hosting.backups.export")}</button>
              <button onclick={() => restore(b)} disabled={working}><Icon name="rollback" size={14} />{t("hosting.backups.restore")}</button>
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
  {#if done}<p class="hint" role="status">{done}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .warn {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-bottom: 10px;
    color: var(--danger);
    overflow-wrap: anywhere;
  }
  .list {
    list-style: none;
    padding: 0;
    margin: 8px 0 0;
    border-top: 1px solid var(--line);
  }
  .list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 8px;
    margin: 0 -8px;
    border-radius: 4px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .list li:hover {
    color: var(--on-selected);
    background: var(--hover);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .mono {
    overflow-wrap: anywhere;
  }
  .btns {
    display: flex;
    gap: 6px;
  }
</style>
