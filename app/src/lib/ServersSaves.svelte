<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api, type ServersAndSaves } from "./api";
  import Icon from "./Icon.svelte";
  import { sortMaps } from "./order";

  let { busy, profile, enabledIds, onImport, onUseSave }: {
    busy: boolean;
    profile: string;
    enabledIds: string[];
    onImport: (name: string) => Promise<boolean>;
    onUseSave: (mode: string, name: string) => Promise<boolean>;
  } = $props();

  let data = $state<ServersAndSaves | null>(null);
  /** An existing config, or NEW to type a name for a new one. */
  const NEW = "\u0000new";
  let picked = $state("");
  let newName = $state("servertest");
  const target = $derived(picked === NEW || !data?.servers.length ? newName : picked);
  let notes = $state<string[]>([]);
  let error = $state("");

  // the Map= line Write sets (empty when no enabled mod adds a map): drag or Alt+arrows to arrange
  let maps = $state<string[]>([]);
  let mapFrom = $state<number | null>(null);
  let mapAt = $state<number | null>(null);
  $effect(() => {
    const name = target.trim();
    if (!name) return;
    api.serverMaps(name).then((m) => target.trim() === name && (maps = m)).catch(() => (maps = []));
  });

  /** The map at `from` dropped before position `to`. */
  function moveMap(from: number, to: number) {
    if (to === from || to === from + 1 || to < 0 || to > maps.length) return;
    const next = [...maps];
    const [m] = next.splice(from, 1);
    next.splice(to > from ? to - 1 : to, 0, m);
    maps = next;
  }

  /** Alt+arrows move the focused map (and keep it focused). */
  async function mapKey(e: KeyboardEvent, i: number) {
    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    e.preventDefault();
    const li = e.currentTarget as HTMLElement;
    moveMap(i, e.key === "ArrowUp" ? i - 1 : i + 2);
    await tick();
    li.focus();
  }

  function dropMap(to: number) {
    if (mapFrom !== null) moveMap(mapFrom, to);
    mapFrom = mapAt = null;
  }

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
    try {
      notes = await api.serverExport(target.trim(), maps);
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
    if (!extra && !lacking) return "Same mods as your current profile";
    return [extra && `${extra} not in your profile`, lacking && `${lacking} of yours not in it`].filter(Boolean).join(" · ");
  }

  async function start(name: string) {
    error = "";
    try {
      notes = [`Started the ${name} server with ${await api.serverStart(name)}. Its console window shows progress.`];
    } catch (e) {
      error = String(e);
    }
  }

  const when = (t: number) => new Date(t * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

  onMount(refresh);
</script>

<section class="ss blueprint" aria-label="Servers and saves">
  <div class="inner">
  <div class="card">
    <h2>Dedicated server</h2>
    <p class="lede">
      Send the "{profile}" profile to a server config, so the server loads the same mods in the same order. It sets
      <span class="mono">Mods=</span>, <span class="mono">WorkshopItems=</span> and, when your mods add maps,
      <span class="mono">Map=</span>. All other settings are kept.
    </p>
    <form class="row" onsubmit={exportTo}>
      <label for="srv">Server config</label>
      {#if data?.servers.length}
        <select id="srv" bind:value={picked}>
          {#each data.servers as s}<option value={s}>{s}.ini</option>{/each}
          <option value={NEW}>New config…</option>
        </select>
      {/if}
      {#if picked === NEW || !data?.servers.length}
        <input type="text" bind:value={newName} spellcheck="false" aria-label="New server config name" placeholder="servertest" />
      {/if}
      <button class="primary" type="submit" disabled={busy || !target.trim()}><Icon name="upload" size={14} />Write to {target.trim() || "…"}.ini</button>
    </form>
    {#if maps.length}
      <h3>Map order</h3>
      <p class="hint">The server loads maps top to bottom: mod maps go before <span class="mono">Muldraugh, KY</span>. Drag or Alt+↑/↓ to arrange; Write saves it.</p>
      <ol class="drag-list maps" role="listbox" aria-label="Map order">
        {#each maps as m, i (m)}
          <li role="option" aria-selected="false" class:drop={mapAt === i} class:dragging={mapFrom === i} draggable="true" tabindex="0"
            onkeydown={(e) => mapKey(e, i)}
            ondragstart={() => (mapFrom = i)} ondragend={() => { mapFrom = mapAt = null; }}
            ondragover={(e) => { e.preventDefault(); mapAt = i; }}
            ondrop={(e) => { e.preventDefault(); dropMap(i); }}>
            <span class="num mono">{i + 1}</span>{m}
          </li>
        {/each}
        <li class="tail" class:drop={mapAt === maps.length} aria-hidden="true"
          ondragover={(e) => { e.preventDefault(); mapAt = maps.length; }}
          ondrop={(e) => { e.preventDefault(); dropMap(maps.length); }}></li>
      </ol>
      <button class="quiet" onclick={() => (maps = sortMaps(maps))} title="Mod maps first, in their current order, then Muldraugh, KY"><Icon name="wand" size={14} />Autosort maps</button>
    {/if}
    {#if notes.length}<ul class="notes">{#each notes as n}<li>{n}</li>{/each}</ul>{/if}
    {#if error}<p class="error">{error}</p>{/if}

    {#if data?.servers.length}
      <h3>Join a server's setup</h3>
      <p class="hint">Make a profile from a server config: missing Workshop mods are installed and the mod order is copied.</p>
      <ul class="list">
        {#each data.servers as s}
          <li>
            <span class="name mono">{s}.ini</span>
            <span class="btns">
              <button disabled={busy} onclick={() => start(s)} title="Start this dedicated server"><Icon name="server" size={14} />Start server</button>
              <button disabled={busy} onclick={async () => { if (await onImport(s)) refresh(); }}><Icon name="upload" size={14} />Use as profile</button>
            </span>
          </li>
        {/each}
      </ul>
    {:else if data}
      <p class="hint">No server configs in <span class="mono">{data.server_dir}</span> yet.</p>
    {/if}

  </div>

  <div class="card">
    <h2>Saved games</h2>
    <p class="lede">Each save remembers the mods it was played with. Load a save's mods before you continue it, so nothing goes missing.</p>
    {#if data && data.saves.length === 0}
      <p class="hint">No saves with a mod list yet.</p>
    {:else if data}
      <ul class="list">
        {#each data.saves as s}
          <li>
            <span class="name">
              <strong>{s.name}</strong>
              <span class="hint">{s.mode} · {s.mods.length} mods · played {when(s.modified)} · {diff(s.mods)}</span>
            </span>
            <button disabled={busy} onclick={() => onUseSave(s.mode, s.name)}><Icon name="swap" size={14} />Use this save's mods</button>
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
  .maps {
    margin-bottom: 6px;
  }
  .maps .num {
    width: 20px;
    text-align: right;
    color: var(--accent-text);
  }
  .maps li:hover .num {
    color: var(--on-selected);
  }
  .notes {
    margin: 10px 0 0;
    padding-left: 18px;
    color: var(--muted);
  }
</style>
