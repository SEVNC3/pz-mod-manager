<script lang="ts">
  import { onMount } from "svelte";
  import { api, type LineProblem, type ServersAndSaves } from "./api";
  import { modChanges } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { keyedError, t, th } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import MapOrder from "./MapOrder.svelte";
  import LeaveOut from "./LeaveOut.svelte";
  import type { Row } from "./api";

  let { busy, lock = "", profile, rows, onImport, ask }: {
    busy: boolean;
    /** The active profile's library: the mods Write sends, less the server's leave-out list. */
    rows: Row[];
    /** Why switching profile is blocked now ("" when it isn't): Use as profile waits. */
    lock?: string;
    profile: string;
    onImport: (name: string) => Promise<boolean>;
    /** A preview to confirm: resolves to the choice picked, -1 for Cancel. */
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
  } = $props();

  let data = $state<ServersAndSaves | null>(null);
  /** The inis of servers PZMM runs: they get their mods in their own Mods tab, so Write leaves them out. */
  let hosted = $state<string[]>([]);
  const norm = (path: string) => path.replaceAll("\\", "/").toLowerCase();
  const ini = (name: string) => norm(`${data?.server_dir ?? ""}/${name}.ini`);
  const configs = $derived(data?.servers.filter((s) => !hosted.includes(ini(s))) ?? []);
  /** An existing config, or NEW to type a name for a new one. */
  const NEW = "\u0000new";
  let picked = $state("");
  let newName = $state("servertest");
  const target = $derived(picked === NEW || !configs.length ? newName : picked);
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
    Promise.all([api.serversAndSaves(), api.hostView().catch(() => null)])
      .then(([d, h]) => {
        data = d;
        hosted = h?.servers.filter((s) => s.managed && !s.remote).map((s) => norm(s.ini)) ?? [];
        // default to a config that exists, so "Write to" doesn't create a stray servertest.ini
        if (configs.length && picked !== NEW && !configs.includes(picked)) picked = configs[0];
        // a new config's name a hosted server has would leave Write disabled: the first unused servertestN instead
        const used = new Set([...d.servers.map(ini), ...hosted]);
        for (let n = 2; hosted.includes(ini(newName.trim())); n++) if (!used.has(ini(`servertest${n}`))) newName = `servertest${n}`;
      })
      .catch((e) => (error = t(...keyedError(e))));

  async function exportTo(e: Event) {
    e.preventDefault();
    error = "";
    notes = [];
    const name = target.trim();
    try {
      // what changes first: the mods, Map=, a new file; and the check Hosting runs (a bad Workshop ID crashes the server)
      const [p, problems] = await Promise.all([api.serverPreview(name, maps), api.serverCheck(name, maps)]);
      const say = (q: LineProblem) => t(`hosting.check.${q.key}`, { id: q.id, item: q.item });
      if (problems.some((q) => q.error)) {
        error = problems.filter((q) => q.error).map(say).join(" ");
        return;
      }
      const lines = [
        ...(p.new ? [t("preview.newFile", { file: `${name}.ini` })] : []),
        ...modChanges(p.added, p.removed),
        ...(p.maps_from.join(";") !== p.maps_to.join(";") ? [t("preview.map", { from: p.maps_from.join("; ") || "—", to: p.maps_to.join("; ") })] : []),
        ...problems.map(say),
      ];
      if ((await ask(t("preview.serverTitle", { file: `${name}.ini` }), lines, [{ label: t("preview.write"), icon: "upload" }])) !== 0) return;
      notes = await api.serverExport(name, maps);
      refresh();
    } catch (err) {
      error = t(...keyedError(err));
    }
  }

  onMount(refresh);
</script>

<h2>{t("servers.title")}</h2>
<p class="lede">
  {@html th("servers.lede", { profile })}
</p>
<form class="row" onsubmit={exportTo}>
  <label for="srv">{t("servers.config")}</label>
  {#if configs.length}
    <select id="srv" bind:value={picked}>
      <!-- i18n-ignore: file names -->
      {#each configs as s}<option value={s}>{s}.ini</option>{/each}
      <option value={NEW}>{t("servers.newConfig")}</option>
    </select>
  {/if}
  {#if picked === NEW || !configs.length}
    <!-- i18n-ignore: file name -->
    <input type="text" bind:value={newName} spellcheck="false" aria-label={t("servers.newNameLabel")} placeholder="servertest" />
  {/if}
  <button class="primary" type="submit" disabled={busy || !target.trim() || hosted.includes(ini(target.trim()))}><Icon name="upload" size={14} />{t("servers.writeTo", { name: target.trim() || "…" })}</button>
</form>
{#if hosted.length}<p class="hint">{t("servers.hostedHint")}</p>{/if}
{#if !hosted.includes(ini(target.trim()))}<LeaveOut server={target} {rows} />{/if}
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
        <button disabled={busy || !!lock} title={lock} onclick={async () => { if (await onImport(s)) refresh(); }}><Icon name="upload" size={14} />{t("servers.useAsProfile")}</button>
      </li>
    {/each}
  </ul>
{:else if data}
  <p class="hint">{@html th("servers.noConfigs", { dir: data.server_dir })}</p>
{/if}

<style>
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
    border-radius: 4px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .list li:hover {
    color: var(--on-selected);
    background: var(--hover);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .name {
    min-width: 0;
    overflow-wrap: anywhere;
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
