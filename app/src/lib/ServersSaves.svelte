<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ServersAndSaves } from "./api";

  let { busy, profile, enabledIds, onImport, onUseSave }: {
    busy: boolean;
    profile: string;
    enabledIds: string[];
    onImport: (name: string) => Promise<boolean>;
    onUseSave: (mode: string, name: string) => Promise<boolean>;
  } = $props();

  let data = $state<ServersAndSaves | null>(null);
  let target = $state("servertest");
  let notes = $state<string[]>([]);
  let error = $state("");

  const refresh = () =>
    api
      .serversAndSaves()
      .then((d) => {
        data = d;
        // default to a config that exists, so "Write to" doesn't create a stray servertest.ini
        if (d.servers.length && !d.servers.includes(target)) target = d.servers[0];
      })
      .catch((e) => (error = String(e)));

  async function exportTo(e: Event) {
    e.preventDefault();
    error = "";
    notes = [];
    try {
      notes = await api.serverExport(target.trim());
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

<section class="ss" aria-label="Servers and saves">
  <div class="inner">
    <h2>Dedicated server</h2>
    <p class="lede">
      Send the "{profile}" profile to a server config, so the server loads the same mods in the same order. It sets
      <span class="mono">Mods=</span>, <span class="mono">WorkshopItems=</span> and, when your mods add maps,
      <span class="mono">Map=</span>. All other settings are kept.
    </p>
    <form class="row" onsubmit={exportTo}>
      <label for="srv">Server config</label>
      <input id="srv" type="text" list="servers" bind:value={target} spellcheck="false" />
      <datalist id="servers">{#each data?.servers ?? [] as s}<option value={s}></option>{/each}</datalist>
      <button class="primary" type="submit" disabled={busy || !target.trim()}>Write to {target.trim() || "…"}.ini</button>
    </form>
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
              <button disabled={busy} onclick={() => start(s)} title="Start this dedicated server">Start server</button>
              <button disabled={busy} onclick={async () => { if (await onImport(s)) refresh(); }}>Use as profile</button>
            </span>
          </li>
        {/each}
      </ul>
    {:else if data}
      <p class="hint">No server configs in <span class="mono">{data.server_dir}</span> yet.</p>
    {/if}

    <h2 class="gap">Saved games</h2>
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
            <button disabled={busy} onclick={() => onUseSave(s.mode, s.name)}>Use this save's mods</button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .ss {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    max-width: 760px;
    padding: 28px 24px 40px;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
  }
  h2.gap {
    margin-top: 36px;
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
    padding: 9px 0;
    border-bottom: 1px solid var(--line);
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
    color: var(--muted);
    font-size: 13px;
  }
  .notes {
    margin: 10px 0 0;
    padding-left: 18px;
    color: var(--muted);
  }
  .error {
    color: var(--danger);
  }
</style>
