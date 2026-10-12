<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChunkArea, type ServerBackup, type ServerCard } from "./api";
  import { fmtDateTime } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";

  let { server, ask, onChanged }: {
    server: ServerCard;
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
    /** Something changed: the new server's name after a copy. */
    onChanged: (name?: string) => void;
  } = $props();

  let copyName = $state("");
  let keepCharacters = $state(true);
  let keepZones = $state(false);
  let worlds = $state<string[]>([]);
  let world = $state("");
  let areas = $state<ChunkArea[]>([{ x1: 0, y1: 0, x2: 0, y2: 0 }]);
  let inside = $state(false);
  let plan = $state<[number, number] | null>(null);
  let error = $state("");
  let done = $state("");
  let working = $state(false);
  /** Backups of the safehouses, zones and factions alone, newest first. */
  let zones = $state<ServerBackup[]>([]);
  const loadZones = () => api.hostZones(server.name).then((z) => (zones = z)).catch(() => {});

  const stopped = $derived(server.status === "stopped" || server.status === "crashed");

  async function run(f: () => Promise<string>) {
    error = done = "";
    working = true;
    try {
      done = await f();
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      working = false;
      refreshPlan();
    }
  }

  const copy = () =>
    run(async () => {
      const name = copyName.trim();
      await api.hostClone(server.name, name);
      copyName = "";
      onChanged(name);
      return t("hosting.world.copied", { name });
    });

  async function wipe() {
    const lines = [t("hosting.world.wipeBackup"), t(keepCharacters ? "hosting.world.wipeKeepsCharacters" : "hosting.world.wipeResetsCharacters"), t(keepZones ? "hosting.world.wipeKeepsZones" : "hosting.world.wipeResetsZones")];
    if ((await ask(t("hosting.world.wipeTitle", { name: server.name }), lines, [{ label: t("hosting.world.wipe"), icon: "trash" }])) !== 0) return;
    run(async () => t("hosting.world.wiped", { count: await api.hostWipe(server.name, keepCharacters, keepZones) }));
  }

  async function migrate() {
    if ((await ask(t("hosting.world.migrateTitle", { world, name: server.name }), [t("hosting.world.migrateLine"), t("hosting.world.wipeBackup")], [{ label: t("hosting.world.migrate"), icon: "swap" }])) !== 0) return;
    run(async () => {
      await api.hostMigrate(server.name, world);
      return t("hosting.world.migrated", { world });
    });
  }

  /** The areas the player filled in (all zeros: an unused row). */
  const used = () => areas.filter((a) => a.x1 || a.y1 || a.x2 || a.y2);

  async function refreshPlan() {
    plan = await api.hostChunkPlan(server.name, used(), inside).catch(() => null);
  }

  async function resetChunks() {
    const count = plan?.[0] ?? 0;
    const lines = [t(inside ? "hosting.world.chunksInsideLine" : "hosting.world.chunksOutsideLine"), t("hosting.world.wipeBackup")];
    if ((await ask(t("hosting.world.chunksTitle", { count }), lines, [{ label: t("hosting.world.chunksReset"), icon: "trash" }])) !== 0) return;
    run(async () => t("hosting.world.chunksDone", { count: await api.hostChunkReset(server.name, used(), inside) }));
  }

  const saveZones = () =>
    run(async () => {
      await api.hostZonesSave(server.name);
      loadZones();
      return t("hosting.world.zonesSaved");
    });

  async function restoreZones(b: ServerBackup) {
    const when = fmtDateTime(b.time);
    const lines = [...(stopped ? [] : [t("hosting.backups.restoreStops")]), t("hosting.world.zonesFlags"), t("hosting.world.zonesKeeps")];
    if ((await ask(t("hosting.world.zonesRestoreTitle", { when }), lines, [{ label: t("hosting.backups.restore"), icon: "rollback" }])) !== 0) return;
    run(async () => {
      await api.hostZonesRestore(server.name, b.file);
      loadZones();
      return t("hosting.world.zonesRestored", { when });
    });
  }

  async function script() {
    const dir = await api.pickFolder("");
    if (dir) run(async () => t("hosting.world.scriptSaved", { file: await api.hostBundle(server.name, dir) }));
  }

  onMount(() => {
    api.hostCoopWorlds().then((w) => ((worlds = w), (world = w[0] ?? ""))).catch(() => {});
    refreshPlan();
    loadZones();
  });
</script>

<div class="world">
  {#if !stopped}<p class="hint"><Icon name="info" size={14} /> {t("hosting.world.stopFirst")}</p>{/if}

  <section>
    <h3>{t("hosting.world.copyTitle")}</h3>
    <form class="row" onsubmit={(e) => (e.preventDefault(), copy())}>
      <input type="text" bind:value={copyName} maxlength="60" placeholder={t("hosting.world.copyName")} aria-label={t("hosting.world.copyName")} />
      <button type="submit" disabled={working || !copyName.trim()}><Icon name="copy" size={14} />{t("hosting.world.copy")}</button>
    </form>
    <p class="hint">{t("hosting.world.copyHint")}</p>
  </section>

  <section>
    <h3>{t("hosting.world.wipeTitleShort")}</h3>
    <label class="check"><input type="checkbox" bind:checked={keepCharacters} />{t("hosting.world.keepCharacters")}</label>
    <label class="check"><input type="checkbox" bind:checked={keepZones} />{t("hosting.world.keepZones")}</label>
    <p class="hint">{t("hosting.world.keepZonesHint")}</p>
    <button class="danger" onclick={wipe} disabled={working || !stopped}><Icon name="trash" size={14} />{t("hosting.world.wipeButton")}</button>
  </section>

  <section>
    <h3>{t("hosting.world.zonesHeading")}</h3>
    <p class="hint">{t("hosting.world.zonesHint")}</p>
    <p class="hint"><Icon name="info" size={14} /> {t("hosting.world.zonesFlags")}</p>
    <div class="row">
      <button onclick={saveZones} disabled={working}><Icon name="download" size={14} />{t("hosting.world.zonesSave")}</button>
    </div>
    {#each zones as b (b.file)}
      <div class="row">
        <span>{fmtDateTime(b.time)}</span>
        <span class="hint grow">{t(b.reason === "restore" ? "hosting.world.zonesBeforeRestore" : "hosting.world.zonesManual")}</span>
        <button onclick={() => restoreZones(b)} disabled={working}><Icon name="rollback" size={14} />{t("hosting.backups.restore")}</button>
      </div>
    {/each}
  </section>

  <section>
    <h3>{t("hosting.world.chunksHeading")}</h3>
    <p class="hint">{t("hosting.world.chunksHint")}</p>
    {#each areas as a, i}
      <div class="row">
        <span class="hint">{t("hosting.world.from")}</span>
        <input type="number" bind:value={a.x1} onchange={refreshPlan} aria-label={t("hosting.world.x1")} />
        <input type="number" bind:value={a.y1} onchange={refreshPlan} aria-label={t("hosting.world.y1")} />
        <span class="hint">{t("hosting.world.to")}</span>
        <input type="number" bind:value={a.x2} onchange={refreshPlan} aria-label={t("hosting.world.x2")} />
        <input type="number" bind:value={a.y2} onchange={refreshPlan} aria-label={t("hosting.world.y2")} />
        <button class="quiet" onclick={() => (areas.splice(i, 1), refreshPlan())} title={t("common.remove")} aria-label={t("common.remove")}><Icon name="x" size={14} /></button>
      </div>
    {/each}
    <div class="row">
      <button onclick={() => areas.push({ x1: 0, y1: 0, x2: 0, y2: 0 })}><Icon name="plus" size={14} />{t("hosting.world.addArea")}</button>
    </div>
    <div class="row" role="radiogroup" aria-label={t("hosting.world.chunksHeading")}>
      <label class="check"><input type="radio" name="chunk-mode" value={false} bind:group={inside} onchange={refreshPlan} />{t("hosting.world.outside")}</label>
      <label class="check"><input type="radio" name="chunk-mode" value={true} bind:group={inside} onchange={refreshPlan} />{t("hosting.world.inside")}</label>
    </div>
    <div class="row">
      <button class="danger" onclick={resetChunks} disabled={working || !stopped || !plan?.[0] || !used().length}><Icon name="trash" size={14} />{t("hosting.world.chunksButton", { count: plan?.[0] ?? 0 })}</button>
      {#if plan}<span class="hint">{t("hosting.world.chunksOf", { count: plan[1] })}</span>{/if}
    </div>
  </section>

  <section>
    <h3>{t("hosting.world.migrateHeading")}</h3>
    {#if !worlds.length}
      <p class="hint">{t("hosting.world.noCoop")}</p>
    {:else}
      <div class="row">
        <select bind:value={world} aria-label={t("hosting.world.migrateHeading")}>
          {#each worlds as w}<option value={w}>{w}</option>{/each}
        </select>
        <button onclick={migrate} disabled={working || !stopped || !world}><Icon name="swap" size={14} />{t("hosting.world.migrate")}</button>
      </div>
      <p class="hint">{t("hosting.world.migrateHint")}</p>
    {/if}
  </section>

  <section>
    <h3>{t("hosting.world.scriptHeading")}</h3>
    <div class="row">
      <button onclick={script} disabled={working}><Icon name="download" size={14} />{t("hosting.world.script")}</button>
      <span class="hint">{t("hosting.world.scriptHint")}</span>
    </div>
  </section>

  {#if done}<p class="hint" role="status">{done}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  section {
    margin-bottom: 16px;
    padding-bottom: 12px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  h3 {
    margin: 0 0 8px;
    font-size: 15px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 6px 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  input[type="number"] {
    width: 90px;
  }
  .grow {
    flex: 1;
  }
</style>
