<script lang="ts">
  import { onMount } from "svelte";
  import { api, type PerfView } from "./api";
  import { fmtSize, workshopUrl } from "./format";
  import { keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import { confirmed } from "./ask.svelte";

  const PZOPT_ITEM = "3805285544";
  const MB = 1 << 20;

  let view = $state<PerfView | null>(null);
  let pick = $state(0);
  let error = $state("");
  let done = $state("");
  let output = $state("");
  let working = $state(false);
  let gc = $state("");
  let jvmArgs = $state("");
  const GCS = ["Z", "G1", "Shenandoah"];
  /** Under 4 GB of RAM even the suggested size would leave Windows less than 2 GB (the "too high" line): none is suggested. */
  const noRoomFor = (v: PerfView) => !!v.ram_mb && v.suggested_mb > v.ram_mb - 2048;
  const noRoom = $derived(!!view && noRoomFor(view));

  const load = () =>
    api
      .perfView()
      .then((v) => {
        if (!v) return;
        view = { ...v, largest: v.largest ?? [], jvm: v.jvm ? { gc: v.jvm.gc ?? "", args: v.jvm.args ?? [] } : null };
        pick = v.memory_mb ?? (noRoomFor(v) ? 2048 : v.suggested_mb);
        gc = view.jvm?.gc ?? "";
        jvmArgs = view.jvm?.args.join("\n") ?? "";
      })
      .catch((e) => (error = t(...keyedError(e))));

  /** Whole GB from 2 up to what leaves 2 GB for Windows (the "too high" line), plus the current and suggested sizes. */
  const sizes = $derived.by(() => {
    if (!view) return [];
    const top = Math.max(2, Math.floor((view.ram_mb - 2048) / 1024));
    const all = new Set([...Array.from({ length: top - 1 }, (_, i) => (i + 2) * 1024), ...(noRoom ? [] : [view.suggested_mb]), ...(view.memory_mb ? [view.memory_mb] : [])]);
    return [...all].sort((a, b) => a - b);
  });

  const advice = $derived.by(() => {
    if (!view || view.memory_mb === null) return view ? t("perf.memory.unset") : "";
    if (view.ram_mb && view.memory_mb > view.ram_mb - 2048) return t("perf.memory.tooHigh");
    if (!noRoom && view.memory_mb < view.suggested_mb) return t("perf.memory.raise", { size: fmtSize(view.suggested_mb * MB), count: view.mods });
    return t("perf.memory.fine");
  });

  async function run(f: () => Promise<unknown>, ok: string) {
    error = done = "";
    working = true;
    try {
      await f();
      done = ok;
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      working = false;
      load();
    }
  }

  async function pzopt(uninstall: boolean) {
    const yes = uninstall ? ({ label: t("common.remove"), icon: "trash" } as const) : ({ label: t("common.install"), icon: "download" } as const);
    if (!(await confirmed(uninstall ? t("perf.pzopt.confirmRemove") : t("perf.pzopt.confirmInstall"), yes))) return;
    output = "";
    await run(async () => (output = await api.pzoptRun(uninstall)), uninstall ? t("perf.pzopt.removed") : t("perf.pzopt.installedNow"));
  }

  const opt = $derived(view?.pzopt);
  const matches = $derived(!!opt?.game_revision && opt.installer_revision === opt.game_revision);

  onMount(() => {
    load();
  });
</script>

<!-- cards for the Settings page it sits on (its .card and h3 styles) -->
{#if done || error}
  <div class="card">
    {#if done}<p class="status" role="status">{done}</p>{/if}
    {#if error}<p class="status error">{error}</p>{/if}
  </div>
{/if}
{#if !view && !error}
  <div class="card"><p class="hint">{t("perf.reading")}</p></div>
{/if}
{#if view}
  <div class="card">
    <h3>{t("perf.memory.title")}</h3>
    {#if !view.launcher}
      <p class="desc">{t("perf.gameNotFound")}</p>
    {:else}
      <p class="desc">{view.memory_mb ? t("perf.memory.now", { size: fmtSize(view.memory_mb * MB), ram: fmtSize(view.ram_mb * MB) }) : ""} {advice}</p>
      {#if view.memory_by_pzopt}<p class="hint">{t("perf.memory.pzopt")}</p>{/if}
      {#if noRoom}<p class="hint">{t("perf.memory.noRoom", { ram: fmtSize(view.ram_mb * MB) })}</p>{/if}
      <div class="row">
        <select bind:value={pick} aria-label={t("perf.memory.title")} disabled={working}>
          {#each sizes as mb}<option value={mb}>{fmtSize(mb * MB)}{mb === view.suggested_mb && !noRoom ? ` · ${t("perf.memory.suggested")}` : ""}</option>{/each}
        </select>
        <button onclick={() => run(() => api.perfMemorySet(pick), t("perf.memory.set", { size: fmtSize(pick * MB) }))} disabled={working || pick === view.memory_mb}><Icon name="check" size={14} />{t("perf.memory.apply")}</button>
      </div>
      <p class="hint foot">{t("perf.memory.hint")}</p>
    {/if}
  </div>

  {#if view.jvm}
    <div class="card">
      <h3>{t("perf.jvm.title")}</h3>
      <div class="row">
        <label for="gc">{t("perf.jvm.gc")}</label>
        <select id="gc" bind:value={gc} disabled={working}>
          {#each [...new Set([...GCS, ...(view.jvm.gc ? [view.jvm.gc] : [])])] as g}<option value={g}>{t(`perf.jvm.gcs.${GCS.includes(g) ? g : "other"}`, { name: g })}</option>{/each}
        </select>
      </div>
      <label class="above" for="jvmargs">{t("perf.jvm.args")}</label>
      <!-- i18n-ignore: JVM flags -->
      <textarea id="jvmargs" class="mono" rows="6" bind:value={jvmArgs} spellcheck="false" disabled={working}></textarea>
      <div class="row">
        <button onclick={() => run(() => api.perfJvmSet(gc, jvmArgs.split("\n").map((a) => a.trim()).filter(Boolean)), t("perf.jvm.saved"))}
          disabled={working || !GCS.includes(gc) || (gc === view.jvm.gc && jvmArgs === view.jvm.args.join("\n"))}><Icon name="check" size={14} />{t("perf.memory.apply")}</button>
      </div>
      <p class="hint foot">{t("perf.jvm.hint")}</p>
    </div>
  {/if}

  <div class="card">
    <h3>{t("perf.pzopt.title")}</h3>
    <p class="desc">{t("perf.pzopt.what")} <button class="link" onclick={() => api.openUrl("https://github.com/xD3I/PZ_Optimization")}>{t("perf.pzopt.github")}</button></p>
    {#if opt}
      {#if !opt.game_revision}
        <p class="hint">{t("perf.gameNotFound")}</p>
      {:else}
        {#if opt.installed === null}
          <p>{t("perf.pzopt.notInstalled")}</p>
        {:else if opt.installed === opt.game_revision}
          <p class="ok"><Icon name="check" size={14} />{t("perf.pzopt.ok", { rev: opt.game_revision })}</p>
        {:else}
          <p class="warn"><Icon name="alert" size={14} />{t("perf.pzopt.mismatch", { built: opt.installed || "?", game: opt.game_revision })}</p>
        {/if}
        {#if !opt.installer}
          <p class="hint">{t("perf.pzopt.noCopy")}</p>
          <button onclick={() => api.openUrl(workshopUrl(PZOPT_ITEM))}><Icon name="external" size={14} />{t("perf.pzopt.workshop")}</button>
        {:else if !matches}
          <p class="hint">{t("perf.pzopt.copyOld", { rev: opt.installer_revision || "?", game: opt.game_revision })}</p>
        {/if}
        <div class="row">
          {#if opt.installer && matches && opt.installed !== opt.game_revision}
            <button class="primary" onclick={() => pzopt(false)} disabled={working || view.game_running}><Icon name="download" size={14} />{opt.installed === null ? t("perf.pzopt.install") : t("perf.pzopt.update")}</button>
          {/if}
          {#if opt.installer && opt.installed !== null}
            <button onclick={() => pzopt(true)} disabled={working || view.game_running}><Icon name="trash" size={14} />{t("perf.pzopt.uninstall")}</button>
          {/if}
        </div>
        {#if view.game_running && opt.installer}<p class="hint">{t("perf.closeGame")}</p>{/if}
      {/if}
    {/if}
    {#if output}<pre class="out">{output}</pre>{/if}
  </div>

  <div class="card">
    <h3>{t("perf.mods.title")}</h3>
    <p class="desc">{t("perf.mods.count", { count: view.mods, size: fmtSize(view.mods_size) })}{#if view.mods > 150} {t("perf.mods.many")}{/if}</p>
    {#if view.largest.length}
      <p class="hint">{t("perf.mods.largest")}</p>
      <ul class="largest">
        {#each view.largest as [id, name, size] (id)}<li><span>{name || id}</span> <span class="hint">{fmtSize(size)}</span></li>{/each}
      </ul>
    {/if}
    <p class="hint foot">{t("perf.mods.hint")}</p>
  </div>
{/if}

<style>
  .status {
    margin: 0;
  }
  .foot {
    margin: 12px 0 0;
  }
  .above {
    display: block;
    margin: 12px 0 6px;
  }
  .hint {
    display: block;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 8px;
  }
  .ok,
  .warn {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .warn {
    color: var(--danger);
  }
  .largest {
    margin: 4px 0;
    padding-left: 18px;
  }
  .largest .hint {
    display: inline;
  }
  .out {
    max-height: 200px;
    overflow: auto;
    white-space: pre-wrap;
    font-size: 12px;
    background: var(--bg);
    padding: 8px;
    border-radius: 6px;
  }
</style>
