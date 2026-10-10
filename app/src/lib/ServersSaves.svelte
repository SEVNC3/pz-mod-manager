<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Save, type ServersAndSaves } from "./api";
  import { fmtDateTime, modChanges } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { t, th } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import MapOrder from "./MapOrder.svelte";

  let { busy, profile, enabledIds, onImport, onUseSave, ask }: {
    busy: boolean;
    profile: string;
    enabledIds: string[];
    onImport: (name: string) => Promise<boolean>;
    /** Switch to a save's mods (after a preview). */
    onUseSave: (save: Save) => Promise<boolean>;
    /** A preview to confirm: resolves to the choice picked, -1 for Cancel. */
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
  } = $props();

  let data = $state<ServersAndSaves | null>(null);
  /** An existing config, or NEW to type a name for a new one. */
  const NEW = "\u0000new";
  let picked = $state("");
  let newName = $state("servertest");
  const target = $derived(picked === NEW || !data?.servers.length ? newName : picked);
  let notes = $state<string[]>([]);
  let error = $state("");

  // the Map= line Write sets (empty when no enabled mod adds a map)
  let maps = $state<string[]>([]);
  $effect(() => {
    const name = target.trim();
    if (!name) return;
    api.serverMaps(name).then((m) => target.trim() === name && (maps = m)).catch(() => (maps = []));
  });

  const refresh = () =>
    api
      .serversAndSaves()
      .then((d) => {
        data = d;
        // default to a config that exists, so "Write to" doesn't create a stray servertest.ini
        if (d.servers.length && picked !== NEW && !d.servers.includes(picked)) picked = d.servers[0];
      })
      .catch((e) => (error = String(e)));

  async function exportTo(e: Event) {
    e.preventDefault();
    error = "";
    notes = [];
    const name = target.trim();
    try {
      // what changes first: the mods, Map=, a new file
      const p = await api.serverPreview(name, maps);
      const lines = [
        ...(p.new ? [t("preview.newFile", { file: `${name}.ini` })] : []),
        ...modChanges(p.added, p.removed),
        ...(p.maps_from.join(";") !== p.maps_to.join(";") ? [t("preview.map", { from: p.maps_from.join("; ") || "—", to: p.maps_to.join("; ") })] : []),
      ];
      if ((await ask(t("preview.serverTitle", { file: `${name}.ini` }), lines, [{ label: t("preview.write"), icon: "upload" }])) !== 0) return;
      notes = await api.serverExport(name, maps);
      refresh();
    } catch (err) {
      error = String(err);
    }
  }

  function diff(mods: string[]) {
    const mine = new Set(enabledIds);
    const theirs = new Set(mods);
    const extra = mods.filter((m) => !mine.has(m)).length;
    const lacking = enabledIds.filter((m) => !theirs.has(m)).length;
    if (!extra && !lacking) return t("servers.sameMods");
    return [extra && t("servers.notInProfile", { count: extra }), lacking && t("servers.notInTheirs", { count: lacking })].filter(Boolean).join(" · ");
  }

  async function start(name: string) {
    error = "";
    try {
      notes = [t("servers.started", { name, how: await api.serverStart(name) })];
    } catch (e) {
      error = String(e);
    }
  }

  const when = (time: number) => fmtDateTime(time);

  onMount(refresh);
</script>

<section class="ss blueprint" aria-label={t("servers.label")}>
  <div class="inner">
  <div class="card">
    <h2>{t("servers.title")}</h2>
    <p class="lede">
      {@html th("servers.lede", { profile })}
    </p>
    <form class="row" onsubmit={exportTo}>
      <label for="srv">{t("servers.config")}</label>
      {#if data?.servers.length}
        <select id="srv" bind:value={picked}>
          <!-- i18n-ignore: file names -->
          {#each data.servers as s}<option value={s}>{s}.ini</option>{/each}
          <option value={NEW}>{t("servers.newConfig")}</option>
        </select>
      {/if}
      {#if picked === NEW || !data?.servers.length}
        <!-- i18n-ignore: file name -->
        <input type="text" bind:value={newName} spellcheck="false" aria-label={t("servers.newNameLabel")} placeholder="servertest" />
      {/if}
      <button class="primary" type="submit" disabled={busy || !target.trim()}><Icon name="upload" size={14} />{t("servers.writeTo", { name: target.trim() || "…" })}</button>
    </form>
    {#if maps.length}
      <h3>{t("servers.mapOrder")}</h3>
      <p class="hint">{@html th("servers.mapOrderHint")}</p>
      <MapOrder bind:maps />
    {/if}
    {#if notes.length}<ul class="notes">{#each notes as n}<li>{n}</li>{/each}</ul>{/if}
    {#if error}<p class="error">{error}</p>{/if}

    {#if data?.servers.length}
      <h3>{t("servers.join")}</h3>
      <p class="hint">{t("servers.joinHint")}</p>
      <ul class="list">
        {#each data.servers as s}
          <li>
            <!-- i18n-ignore: file name -->
            <span class="name mono">{s}.ini</span>
            <span class="btns">
              <button disabled={busy} onclick={() => start(s)} title={t("servers.startTip")}><Icon name="server" size={14} />{t("servers.start")}</button>
              <button disabled={busy} onclick={async () => { if (await onImport(s)) refresh(); }}><Icon name="upload" size={14} />{t("servers.useAsProfile")}</button>
            </span>
          </li>
        {/each}
      </ul>
    {:else if data}
      <p class="hint">{@html th("servers.noConfigs", { dir: data.server_dir })}</p>
    {/if}

  </div>

  <div class="card">
    <h2>{t("servers.saves.title")}</h2>
    <p class="lede">{t("servers.saves.lede")}</p>
    {#if data && data.saves.length === 0}
      <p class="hint">{t("servers.saves.none")}</p>
    {:else if data}
      <ul class="list">
        {#each data.saves as s}
          <li>
            <span class="name">
              <strong>{s.name}</strong>
              <span class="hint">{t("servers.saves.info", { mode: s.mode, count: s.mods.length, when: when(s.modified), diff: diff(s.mods) })}</span>
            </span>
            <button disabled={busy} onclick={() => onUseSave(s)}><Icon name="swap" size={14} />{t("servers.saves.useMods")}</button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  </div>
</section>

<style>
  .ss {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    max-width: 820px;
    margin: 0 auto;
    padding: 24px 16px 40px;
    display: grid;
    gap: 16px;
  }
  .card {
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
    padding: 20px 24px 22px;
    min-width: 0;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
  }
  h3 {
    margin: 24px 0 4px;
    font-size: 15px;
  }
  .lede {
    margin: 0 0 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .row label {
    color: var(--muted);
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
    padding: 9px 8px;
    margin: 0 -8px;
    border-radius: 6px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .list li:hover {
    background: color-mix(in oklab, var(--accent) 16%, var(--panel));
  }
  .name {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  .hint {
    display: block;
  }
  .notes {
    margin: 10px 0 0;
    padding-left: 18px;
    color: var(--muted);
  }
</style>
