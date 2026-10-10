<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Browse } from "./api";

  let { busy, defaultBuild, onInstall, viewpoint = false }: {
    busy: boolean;
    defaultBuild: number | null;
    onInstall: (id: string) => Promise<boolean>;
    /** Project Viewpoint mode: offer a shortcut to its add-ons. */
    viewpoint?: boolean;
  } = $props();

  const SORTS = [
    ["trend", "Popular this week"],
    ["totaluniquesubscribers", "Most subscribed"],
    ["toprated", "Top rated"],
    ["mostrecent", "Newest"],
    ["lastupdated", "Recently updated"],
  ];
  const PER_PAGE = 30;

  let query = $state("");
  let search = $state("");
  let sort = $state("trend");
  // svelte-ignore state_referenced_locally
  let build = $state(String(defaultBuild ?? 42));
  let page = $state(1);
  let data = $state<Browse | null>(null);
  let loading = $state(false);
  let error = $state("");
  let installing = $state<string | null>(null);
  let seq = 0;

  async function load() {
    const mine = ++seq;
    loading = true;
    error = "";
    try {
      const r = await api.browse(search, sort, page, build ? Number(build) : null);
      if (mine === seq) data = r;
    } catch (e) {
      if (mine === seq) error = String(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  function submit(e: Event) {
    e.preventDefault();
    search = query.trim();
    page = 1;
    load();
  }

  function go(p: number) {
    page = p;
    load();
    document.querySelector(".results")?.scrollTo({ top: 0 });
  }

  async function install(id: string) {
    installing = id;
    const ok = await onInstall(id);
    installing = null;
    if (ok && data) data.items = data.items.map((i) => (i.id === id ? { ...i, have: true } : i));
  }

  const pages = $derived(data ? Math.max(1, Math.ceil(data.total / PER_PAGE)) : 1);
  const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
  const date = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  const plain = (s: string) => s.replace(/\[\/?[a-z0-9*]+(=[^\]]*)?\]/gi, "").replace(/\s+/g, " ").trim();
  const builds = (tags: string[]) => tags.filter((t) => /^Build \d+$/.test(t)).map((t) => t.replace("Build ", "B"));

  onMount(load);
</script>

<section class="workshop" aria-label="Steam Workshop">
  <form class="bar" onsubmit={submit}>
    <input type="search" bind:value={query} placeholder="Search the Steam Workshop" aria-label="Search the Steam Workshop" />
    <button type="submit">Search</button>
    {#if search}
      <button type="button" class="quiet" onclick={() => { query = search = ""; page = 1; load(); }}>Clear search</button>
    {:else}
      <label>Sort
        <select bind:value={sort} onchange={() => go(1)}>
          {#each SORTS as [v, label]}<option value={v}>{label}</option>{/each}
        </select>
      </label>
    {/if}
    {#if viewpoint && search !== "Project Viewpoint"}
      <button type="button" onclick={() => { query = search = "Project Viewpoint"; page = 1; load(); }}>Viewpoint add-ons</button>
    {/if}
    <label>Game
      <select bind:value={build} onchange={() => go(1)}>
        <option value="42">Build 42</option>
        <option value="41">Build 41</option>
        <option value="">Any build</option>
      </select>
    </label>
    <span class="count">
      {#if loading}Loading…{:else if data}{compact.format(data.total)} {search ? `results for "${search}"` : "mods"}{/if}
    </span>
  </form>

  <div class="results" aria-busy={loading}>
    {#if error}
      <div class="message">
        <p>Couldn't reach the Steam Workshop: {error}</p>
        <button onclick={load}>Try again</button>
      </div>
    {:else if data && data.items.length === 0 && !loading}
      <div class="message"><p>No mods match{search ? ` "${search}"` : ""}. Try fewer words or another build.</p></div>
    {:else if data}
      <ul class="grid" class:dim={loading}>
        {#each data.items as it (it.id)}
          <li class="card">
            <button class="thumb" onclick={() => api.openUrl(`https://steamcommunity.com/sharedfiles/filedetails/?id=${it.id}`)}
              aria-label={`Open ${it.title} on Steam`}>
              {#if it.preview_url}<img src={it.preview_url} alt="" loading="lazy" />{/if}
            </button>
            <div class="body">
              <h3 title={it.title}>{it.title}</h3>
              <p class="meta">
                <span>{compact.format(it.subscriptions)} subscribers</span>
                <span>Updated {date(it.time_updated)}</span>
                {#each builds(it.tags) as b}<span class="tag">{b}</span>{/each}
              </p>
              {#if it.description}<p class="desc">{plain(it.description)}</p>{/if}
              {#if it.num_children > 0}
                <p class="needs">Needs {it.num_children} other {it.num_children === 1 ? "mod" : "mods"}, installed with it</p>
              {/if}
            </div>
            <div class="actions">
              {#if it.have}
                <span class="have">✓ Installed</span>
              {:else}
                <button class="primary" disabled={busy || installing !== null} onclick={() => install(it.id)}>
                  {installing === it.id ? "Installing…" : "Install"}
                </button>
              {/if}
              <button class="quiet" onclick={() => api.openUrl(`https://steamcommunity.com/sharedfiles/filedetails/?id=${it.id}`)}>Steam page</button>
            </div>
          </li>
        {/each}
      </ul>
      <nav class="pager" aria-label="Pages">
        <button disabled={page <= 1 || loading} onclick={() => go(page - 1)}>Previous</button>
        <span>Page {page} of {compact.format(pages)}</span>
        <button disabled={page >= pages || loading} onclick={() => go(page + 1)}>Next</button>
      </nav>
    {/if}
  </div>
</section>

<style>
  .workshop {
    display: grid;
    grid-template-rows: auto 1fr;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
  }
  .bar input[type="search"] {
    width: 280px;
  }
  .bar label {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--muted);
  }
  .count {
    margin-left: auto;
    color: var(--muted);
  }
  .results {
    overflow-y: auto;
    min-height: 0;
    padding: 16px;
  }
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    gap: 14px;
  }
  .grid.dim {
    opacity: 0.5;
  }
  .card {
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
  }
  .thumb {
    padding: 0;
    border: 0;
    border-radius: 0;
    aspect-ratio: 16 / 10;
    background: var(--raised);
    overflow: hidden;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .body {
    padding: 10px 12px 4px;
    flex: 1;
  }
  h3 {
    margin: 0 0 4px;
    font-size: 15px;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .tag {
    font-family: var(--font-mono);
    color: var(--accent-text);
  }
  .desc {
    margin: 0;
    font-size: 13px;
    color: color-mix(in oklab, var(--text) 80%, var(--muted));
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .needs {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px 12px;
  }
  .have {
    color: var(--accent-text);
    font-weight: 700;
  }
  .pager {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 14px;
    padding: 18px 0 6px;
    color: var(--muted);
  }
  .message {
    max-width: 480px;
    margin: 10vh auto 0;
    text-align: center;
    color: var(--muted);
  }
</style>
