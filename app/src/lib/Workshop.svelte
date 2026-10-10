<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Browse, type WorkshopItem } from "./api";
  import Icon from "./Icon.svelte";

  let { busy, defaultBuild, onInstall, viewpoint = false }: {
    busy: boolean;
    defaultBuild: number | null;
    onInstall: (id: string) => Promise<boolean>;
    /** Project Viewpoint mode: offer its add-ons and community. */
    viewpoint?: boolean;
  } = $props();

  const SORTS = [
    ["totaluniquesubscribers", "Most subscribed"],
    ["toprated", "Top rated"],
    ["trend", "Popular this week"],
    ["mostrecent", "Newest"],
    ["lastupdated", "Recently updated"],
  ];
  const PER_PAGE = 30;
  const VIEWPOINT_DISCORD = "https://discord.gg/g83XvPhEgQ";

  /** "hot": this week's most popular, ranked. "browse": search and every sort. */
  let mode = $state<"hot" | "browse">("hot");
  let query = $state("");
  let search = $state("");
  let sort = $state("totaluniquesubscribers");
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
      const r = mode === "hot"
        ? await api.browse("", "trend", 1, build ? Number(build) : null)
        : await api.browse(search, sort, page, build ? Number(build) : null);
      if (mine === seq) data = r;
    } catch (e) {
      if (mine === seq) error = String(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  function show(m: "hot" | "browse") {
    mode = m;
    page = 1;
    load();
  }

  function submit(e: Event) {
    e.preventDefault();
    search = query.trim();
    mode = "browse";
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

  const steamPage = (it: WorkshopItem) => api.openUrl(`https://steamcommunity.com/sharedfiles/filedetails/?id=${it.id}`);
  const pages = $derived(data ? Math.max(1, Math.ceil(data.total / PER_PAGE)) : 1);
  const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
  const date = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  const plain = (s: string) => s.replace(/\[\/?[a-z0-9*]+(=[^\]]*)?\]/gi, "").replace(/\s+/g, " ").trim();
  const builds = (tags: string[]) => tags.filter((t) => /^Build \d+$/.test(t)).map((t) => t.replace("Build ", "B"));

  onMount(load);
</script>

{#snippet stars(it: WorkshopItem)}
  {#if it.votes > 0}
    <span class="stars" title={`${it.stars} of 5 stars from ${it.votes.toLocaleString()} votes`}>
      {#each [1, 2, 3, 4, 5] as n}<span class:lit={n <= it.stars}><Icon name="star" size={12} fill /></span>{/each}
      <span class="votes">{compact.format(it.votes)}</span>
    </span>
  {/if}
{/snippet}

{#snippet installButton(it: WorkshopItem)}
  {#if it.have}
    <span class="have"><Icon name="check" size={14} /> Installed</span>
  {:else}
    <button class="primary" disabled={busy || installing !== null} onclick={() => install(it.id)}>
      <Icon name="download" size={14} />{installing === it.id ? "Installing…" : "Install"}
    </button>
  {/if}
{/snippet}

<section class="workshop" aria-label="Steam Workshop">
  <form class="bar" onsubmit={submit}>
    <div class="views" role="tablist" aria-label="Workshop view">
      <button type="button" role="tab" aria-selected={mode === "hot"} class:active={mode === "hot"} onclick={() => show("hot")}>
        <Icon name="flame" size={15} /> What's hot
      </button>
      <button type="button" role="tab" aria-selected={mode === "browse"} class:active={mode === "browse"} onclick={() => show("browse")}>
        <Icon name="search" size={15} /> Browse
      </button>
    </div>
    <input type="search" bind:value={query} placeholder="Search the Steam Workshop" aria-label="Search the Steam Workshop" />
    <button type="submit"><Icon name="search" size={14} /> Search</button>
    {#if mode === "browse" && search}
      <button type="button" class="quiet" onclick={() => { query = search = ""; page = 1; load(); }}>Clear search</button>
    {:else if mode === "browse"}
      <select bind:value={sort} onchange={() => go(1)} aria-label="Sort">
        {#each SORTS as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    {/if}
    <select bind:value={build} onchange={() => go(1)} aria-label="Game build">
      <option value="42">Build 42</option>
      <option value="41">Build 41</option>
      <option value="">Any build</option>
    </select>
    {#if viewpoint}
      <button type="button" onclick={() => { query = search = "Project Viewpoint"; mode = "browse"; page = 1; load(); }}>
        <Icon name="layers" size={14} /> Viewpoint add-ons
      </button>
      <button type="button" class="quiet" onclick={() => api.openUrl(VIEWPOINT_DISCORD)} title="Project Viewpoint's community Discord: help, bug reports, news">
        <img src="/brand/discord.png" alt="" width="16" height="16" class="brand-img" /> Viewpoint Discord
      </button>
    {/if}
    <span class="count">
      {#if loading}Loading…{:else if data && mode === "browse"}{compact.format(data.total)} {search ? `results for "${search}"` : "mods"}{:else if data}Most popular this week{/if}
    </span>
  </form>

  <div class="results" aria-busy={loading}>
    {#if error}
      <div class="message">
        <p>Couldn't reach the Steam Workshop: {error}</p>
        <button onclick={load}><Icon name="refresh" size={14} /> Try again</button>
      </div>
    {:else if data && data.items.length === 0 && !loading}
      <div class="message"><p>No mods match{search ? ` "${search}"` : ""}. Try fewer words or another build.</p></div>
    {:else if data && mode === "hot"}
      <ol class="hot" class:dim={loading}>
        {#each data.items as it, i (it.id)}
          <li>
            <span class="rank" class:top={i < 3}>{i + 1}</span>
            <button class="hot-thumb" onclick={() => steamPage(it)} aria-label={`Open ${it.title} on Steam`}>
              {#if it.preview_url}<img src={it.preview_url} alt="" loading="lazy" />{/if}
            </button>
            <div class="hot-body">
              <h3 title={it.title}>{it.title}</h3>
              <p class="meta">
                {@render stars(it)}
                <span>{compact.format(it.subscriptions)} subscribers</span>
                <span>Updated {date(it.time_updated)}</span>
                {#each builds(it.tags) as b}<span class="tag">{b}</span>{/each}
              </p>
              {#if it.description}<p class="desc one">{plain(it.description)}</p>{/if}
            </div>
            <div class="hot-actions">
              {@render installButton(it)}
              <button class="quiet" onclick={() => steamPage(it)} title="Open on Steam"><Icon name="steam" size={15} /> Steam</button>
            </div>
          </li>
        {/each}
      </ol>
    {:else if data}
      <ul class="grid" class:dim={loading}>
        {#each data.items as it (it.id)}
          <li class="card">
            <button class="thumb" onclick={() => steamPage(it)} aria-label={`Open ${it.title} on Steam`}>
              {#if it.preview_url}<img src={it.preview_url} alt="" loading="lazy" />{/if}
            </button>
            <div class="body">
              <h3 title={it.title}>{it.title}</h3>
              <p class="meta">
                {@render stars(it)}
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
              {@render installButton(it)}
              <button class="quiet" onclick={() => steamPage(it)}><Icon name="steam" size={15} /> Steam page</button>
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
    gap: 8px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
  }
  .bar input[type="search"] {
    width: 240px;
  }
  .views {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
    margin-right: 6px;
  }
  .views button {
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--muted);
  }
  .views button.active {
    background: var(--accent-soft);
    color: var(--accent-text);
    font-weight: 700;
  }
  .brand-img {
    border-radius: 3px;
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
  .dim {
    opacity: 0.5;
  }

  /* What's hot: a ranked list */
  .hot {
    list-style: none;
    margin: 0 auto;
    padding: 0;
    max-width: 1100px;
  }
  .hot li {
    display: grid;
    grid-template-columns: 44px 112px 1fr auto;
    gap: 14px;
    align-items: center;
    padding: 10px 8px;
    border-bottom: 1px solid var(--line);
  }
  .rank {
    font-family: var(--font-mono);
    font-size: 22px;
    text-align: right;
    color: var(--muted);
  }
  .rank.top {
    color: var(--accent-text);
    font-weight: 700;
  }
  .hot-thumb {
    padding: 0;
    border: 0;
    border-radius: 6px;
    width: 112px;
    aspect-ratio: 16 / 10;
    overflow: hidden;
    background: var(--raised);
  }
  .hot-thumb img,
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .hot-body {
    min-width: 0;
  }
  .hot-actions {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  /* Browse: a card grid */
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    gap: 14px;
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
    align-items: center;
    gap: 4px 10px;
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .stars {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    color: var(--line);
  }
  .stars .lit {
    color: var(--accent);
  }
  .votes {
    margin-left: 4px;
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
  .desc.one {
    -webkit-line-clamp: 1;
    line-clamp: 1;
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
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
