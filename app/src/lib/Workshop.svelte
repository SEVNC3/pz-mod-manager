<script lang="ts">
  import { onMount } from "svelte";
  import { api, newQuery, type Browse, type Query, type WorkshopItem } from "./api";
  import Icon from "./Icon.svelte";
  import WorkshopPanel from "./WorkshopPanel.svelte";

  let { busy, defaultBuild, onInstall, viewpoint = false }: {
    busy: boolean;
    defaultBuild: number | null;
    onInstall: (id: string) => Promise<boolean>;
    /** Project Viewpoint mode: offer its add-ons and community. */
    viewpoint?: boolean;
  } = $props();

  // Steam's own sorts and filters for the Project Zomboid Workshop (docs/WORKSHOP-PARITY.md)
  const ITEM_SORTS: [string, string][] = [
    ["trend", "Most popular"],
    ["totaluniquesubscribers", "Most subscribed"],
    ["toprated", "Top rated"],
    ["mostrecent", "Newest"],
    ["lastupdated", "Recently updated"],
    ["libraries", "Most required (libraries)"],
    ["num_parent_collections", "In the most collections"],
  ];
  const COLLECTION_SORTS = ITEM_SORTS.filter(([v]) => ["trend", "toprated", "mostrecent", "lastupdated"].includes(v));
  const DAYS: [number, string][] = [[1, "Today"], [7, "This week"], [30, "This month"], [90, "3 months"], [180, "6 months"], [365, "This year"]];
  const TAGS = ["Animals", "Audio", "Balance", "Building", "Clothing/Armor", "Farming", "Food", "Framework", "Hardmode", "Interface", "Items",
    "Language/Translation", "Literature", "Map", "Military", "Misc", "Models", "Multiplayer", "Pop Culture", "QoL", "Realistic", "Silly/Fun",
    "Skills", "Textures", "Traits", "Vehicles", "Weapons", "WIP"];
  const HOME_ROWS: [string, Partial<Query>][] = [
    ["Popular this week", { sort: "trend", days: 7 }],
    ["Most subscribed", { sort: "totaluniquesubscribers" }],
    ["Newest", { sort: "mostrecent" }],
    ["Recently updated", { sort: "lastupdated" }],
    ["Most required", { sort: "totaluniquesubscribers", libraries: true }],
  ];
  const VIEWPOINT_DISCORD = "https://discord.gg/g83XvPhEgQ";

  let view = $state<"home" | "browse" | "author">("browse");
  let q = $state<Query>(newQuery());
  let sortChoice = $state("totaluniquesubscribers"); // "libraries" is a sort here, a filter for Steam
  let queryText = $state("");
  // svelte-ignore state_referenced_locally
  let build = $state(String(defaultBuild ?? 42));
  let filtersOpen = $state(false);
  let dates = $state({ createdFrom: "", createdTo: "", updatedFrom: "", updatedTo: "" });
  /** What a "needs this" / "collections with this" / author list is about. */
  let context = $state<{ kind: "child" | "author"; item: WorkshopItem } | null>(null);
  let authorPage = $state(1);
  let data = $state<Browse | null>(null);
  let rows = $state<Record<string, WorkshopItem[] | null>>({});
  let loading = $state(false);
  let error = $state("");
  let installing = $state<string | null>(null);
  let open = $state<WorkshopItem | null>(null);
  let seq = 0;

  const unix = (d: string, end = false) => (d ? Math.floor(new Date(d + (end ? "T23:59:59" : "T00:00:00")).getTime() / 1000) : 0);
  /** The request as Steam wants it: build in the tags, libraries as a filter, dates as unix times. */
  function request(over: Partial<Query> = {}): Query {
    const required = [...q.required.filter((t) => !t.startsWith("Build ")), ...(build ? [`Build ${build}`] : [])];
    const libraries = sortChoice === "libraries";
    return {
      ...$state.snapshot(q),
      sort: libraries ? "totaluniquesubscribers" : sortChoice,
      libraries,
      required,
      created_from: unix(dates.createdFrom),
      created_to: unix(dates.createdTo, true),
      updated_from: unix(dates.updatedFrom),
      updated_to: unix(dates.updatedTo, true),
      ...over,
    };
  }

  async function load() {
    const mine = ++seq;
    loading = true;
    error = "";
    try {
      const r = view === "author" && context ? await api.workshopAuthor(context.item.creator, authorPage) : await api.browse(request());
      if (mine === seq) data = r;
    } catch (e) {
      if (mine === seq) error = String(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  async function loadHome() {
    const mine = ++seq;
    error = "";
    for (const [title, over] of HOME_ROWS) {
      if (rows[title]) continue;
      try {
        const r = await api.browse(newQuery({ ...over, required: build ? [`Build ${build}`] : [] }));
        if (mine !== seq) return;
        rows[title] = r.items.slice(0, 12);
      } catch (e) {
        if (mine === seq) error = String(e);
        return;
      }
    }
  }

  function show(v: "home" | "browse") {
    view = v;
    context = null;
    if (v === "home") loadHome();
    else go(1);
  }

  function go(p: number) {
    if (view === "author") authorPage = p;
    else q.page = p;
    load();
    document.querySelector(".results")?.scrollTo({ top: 0 });
  }

  function submit(e: Event) {
    e.preventDefault();
    q.search = queryText.trim();
    view = "browse";
    go(1);
  }

  /** Cycle a tag: off → required → excluded → off. */
  function cycleTag(t: string) {
    if (q.required.includes(t)) {
      q.required = q.required.filter((x) => x !== t);
      q.excluded = [...q.excluded, t];
    } else if (q.excluded.includes(t)) q.excluded = q.excluded.filter((x) => x !== t);
    else q.required = [...q.required, t];
    go(1);
  }

  function clearFilters() {
    q.required = [];
    q.excluded = [];
    q.incompatible = false;
    dates = { createdFrom: "", createdTo: "", updatedFrom: "", updatedTo: "" };
    go(1);
  }
  const filterCount = $derived(q.required.length + q.excluded.length + (q.incompatible ? 1 : 0) + Object.values(dates).filter(Boolean).length);

  function viewAll(over: Partial<Query>) {
    view = "browse";
    context = null;
    q = newQuery({ ...over, required: q.required, excluded: q.excluded });
    sortChoice = over.libraries ? "libraries" : (over.sort ?? "trend");
    go(1);
  }

  function setSection(collections: boolean) {
    q.collections = collections;
    if (collections && !COLLECTION_SORTS.some(([v]) => v === sortChoice)) sortChoice = "trend";
    go(1);
  }

  // panel links
  function dependents(it: WorkshopItem) {
    open = null;
    view = "browse";
    context = { kind: "child", item: it };
    q = newQuery({ child: it.id, collections: false });
    sortChoice = "totaluniquesubscribers";
    go(1);
  }
  function inCollections(it: WorkshopItem) {
    open = null;
    view = "browse";
    context = { kind: "child", item: it };
    q = newQuery({ child: it.id, collections: true });
    sortChoice = "trend";
    go(1);
  }
  function byAuthor(it: WorkshopItem) {
    open = null;
    view = "author";
    context = { kind: "author", item: it };
    authorPage = 1;
    load();
  }
  function clearContext() {
    context = null;
    q.child = "";
    view = "browse";
    go(1);
  }

  async function install(id: string) {
    installing = id;
    const ok = await onInstall(id);
    installing = null;
    if (!ok) return;
    const mark = (list: WorkshopItem[] | null | undefined) => list?.map((i) => (i.id === id ? { ...i, have: true } : i)) ?? null;
    if (data) data.items = mark(data.items)!;
    for (const k of Object.keys(rows)) rows[k] = mark(rows[k]);
    if (open?.id === id) open = { ...open, have: true };
  }

  const perPage = $derived(view === "author" ? 30 : q.per_page);
  const pages = $derived(data ? Math.max(1, Math.min(1000, Math.ceil(data.total / perPage))) : 1);
  const page = $derived(view === "author" ? authorPage : q.page);
  const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
  const date = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  const plain = (s: string) => s.replace(/\[\/?[a-z0-9*]+(=[^\]]*)?\]/gi, "").replace(/\s+/g, " ").trim();
  const builds = (tags: string[]) => tags.filter((t) => /^Build \d+$/.test(t)).map((t) => t.replace("Build ", "B"));
  const sorts = $derived(q.collections ? COLLECTION_SORTS : ITEM_SORTS);

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
      <Icon name="download" size={14} />{installing === it.id ? "Installing…" : it.collection ? "Install all" : "Install"}
    </button>
  {/if}
{/snippet}

{#snippet card(it: WorkshopItem, small = false)}
  <li class="card" class:small>
    <button class="thumb" onclick={() => (open = it)} aria-label={`Details for ${it.title}`}>
      {#if it.preview_url}<img src={it.preview_url} alt="" loading="lazy" />{/if}
      {#if it.collection}<span class="kind"><Icon name="folder" size={12} /> Collection · {it.num_children}</span>{/if}
    </button>
    <div class="body">
      <button class="title" onclick={() => (open = it)} title={it.title}>{it.title}</button>
      {#if it.author && !small}
        <button class="quiet author" onclick={() => byAuthor(it)} title="More by this author">by {it.author}</button>
      {/if}
      <p class="meta">
        {@render stars(it)}
        <span title="Current subscribers">{compact.format(it.subscriptions)} subs</span>
        {#if !small}
          <span title="Favourites"><Icon name="star" size={11} /> {compact.format(it.favorited)}</span>
          <span title="Comments"><Icon name="message" size={11} /> {compact.format(it.comments)}</span>
          <span title={`Posted ${date(it.time_created)}`}>Updated {date(it.time_updated)}</span>
          {#each builds(it.tags) as b}<span class="tag">{b}</span>{/each}
        {/if}
      </p>
      {#if it.description && !small}<p class="desc">{plain(it.description)}</p>{/if}
      {#if it.num_children > 0 && !it.collection && !small}
        <p class="needs">Needs {it.num_children} other {it.num_children === 1 ? "mod" : "mods"}, installed with it</p>
      {/if}
    </div>
    <div class="actions">
      {@render installButton(it)}
      <button class="quiet" onclick={() => (open = it)}>Details</button>
    </div>
  </li>
{/snippet}

<section class="workshop" aria-label="Steam Workshop">
  <form class="bar" onsubmit={submit}>
    <div class="views" role="tablist" aria-label="Workshop view">
      <button type="button" role="tab" aria-selected={view !== "home"} class:active={view !== "home"} onclick={() => show("browse")}>
        <Icon name="search" size={15} /> Browse
      </button>
      <button type="button" role="tab" aria-selected={view === "home"} class:active={view === "home"} onclick={() => show("home")}>
        <Icon name="flame" size={15} /> Home
      </button>
    </div>
    <input type="search" bind:value={queryText} placeholder="Search the Workshop" aria-label="Search the Workshop"
      title={'Steam search: "exact phrase", AND, OR, NOT, +must, -exclude'} />
    <select bind:value={q.scope} aria-label="Search in" title="Where the words must appear">
      <option value={0}>Anywhere</option>
      <option value={1}>Titles</option>
      <option value={2}>Descriptions</option>
    </select>
    <button type="submit"><Icon name="search" size={14} /> Search</button>
    {#if q.search}
      <button type="button" class="quiet" onclick={() => { queryText = q.search = ""; go(1); }}>Clear search</button>
    {/if}
    <select bind:value={build} onchange={() => { rows = {}; view === "home" ? loadHome() : go(1); }} aria-label="Game build">
      <option value="42">Build 42</option>
      <option value="41">Build 41</option>
      <option value="">Any build</option>
    </select>
    {#if viewpoint}
      <button type="button" onclick={() => { queryText = q.search = "Project Viewpoint"; view = "browse"; context = null; go(1); }}>
        <Icon name="layers" size={14} /> Viewpoint add-ons
      </button>
      <button type="button" class="quiet" onclick={() => api.openUrl(VIEWPOINT_DISCORD)} title="Project Viewpoint's community Discord: help, bug reports, news">
        <img src="/brand/discord.png" alt="" width="16" height="16" class="brand-img" /> Viewpoint Discord
      </button>
    {/if}
  </form>

  {#if view === "browse"}
    <div class="bar sub">
      <div class="seg" role="radiogroup" aria-label="Show">
        <button type="button" role="radio" aria-checked={!q.collections} class:active={!q.collections} onclick={() => setSection(false)}>Mods</button>
        <button type="button" role="radio" aria-checked={q.collections} class:active={q.collections} onclick={() => setSection(true)}>Collections</button>
      </div>
      {#if !q.search}
        <select bind:value={sortChoice} onchange={() => go(1)} aria-label="Sort">
          {#each sorts as [v, label]}<option value={v}>{label}</option>{/each}
        </select>
        {#if sortChoice === "trend"}
          <select bind:value={q.days} onchange={() => go(1)} aria-label="Popular over">
            {#each DAYS as [d, label]}<option value={d}>{label}</option>{/each}
          </select>
        {/if}
      {/if}
      <button type="button" class:active-filter={filterCount > 0} aria-expanded={filtersOpen} onclick={() => (filtersOpen = !filtersOpen)}>
        <Icon name="settings" size={14} /> Filters{filterCount ? ` (${filterCount})` : ""}
      </button>
      <select bind:value={q.per_page} onchange={() => go(1)} aria-label="Results per page">
        <option value={30}>30 per page</option>
        <option value={50}>50 per page</option>
      </select>
      <span class="count">
        {#if loading}Loading…{:else if data}{compact.format(data.total)} {q.collections ? "collections" : "mods"}{q.search ? ` for "${q.search}"` : ""}{/if}
      </span>
    </div>
    {#if filtersOpen}
      <div class="filters">
        <p class="hint">Click a tag to require it, again to exclude it, once more to clear. Results have every required tag.</p>
        <div class="chips">
          {#each TAGS as t}
            {@const state = q.required.includes(t) ? "in" : q.excluded.includes(t) ? "out" : ""}
            <button type="button" class="chip {state}" onclick={() => cycleTag(t)} aria-pressed={state !== ""}
              title={state === "in" ? `Only ${t}` : state === "out" ? `No ${t}` : t}>
              {#if state === "in"}<Icon name="check" size={12} />{:else if state === "out"}<Icon name="x" size={12} />{/if}{t}
            </button>
          {/each}
        </div>
        <div class="more">
          <label>Posted <input type="date" bind:value={dates.createdFrom} onchange={() => go(1)} /> to <input type="date" bind:value={dates.createdTo} onchange={() => go(1)} /></label>
          <label>Updated <input type="date" bind:value={dates.updatedFrom} onchange={() => go(1)} /> to <input type="date" bind:value={dates.updatedTo} onchange={() => go(1)} /></label>
          <label class="check"><input type="checkbox" bind:checked={q.incompatible} onchange={() => go(1)} /> Only mods marked incompatible</label>
          {#if filterCount}<button type="button" class="quiet" onclick={clearFilters}>Clear filters</button>{/if}
        </div>
      </div>
    {/if}
  {/if}

  {#if context}
    <div class="context">
      {#if context.kind === "author"}
        <span>Mods by <strong>{context.item.author || "this author"}</strong>{data ? ` (${data.total})` : ""}</span>
      {:else if q.collections}
        <span>Collections with <strong>{context.item.title}</strong></span>
      {:else}
        <span>Mods that need <strong>{context.item.title}</strong></span>
      {/if}
      <button class="quiet" onclick={clearContext}><Icon name="x" size={14} /> Back to browsing</button>
    </div>
  {/if}

  <div class="results" aria-busy={loading}>
    {#if error}
      <div class="message">
        <p>Couldn't reach the Steam Workshop: {error}</p>
        <button onclick={() => (view === "home" ? loadHome() : load())}><Icon name="refresh" size={14} /> Try again</button>
      </div>
    {:else if view === "home"}
      {#each HOME_ROWS as [title, over]}
        <section class="row">
          <header>
            <h2>{title}</h2>
            <button class="quiet" onclick={() => viewAll(over)}>View all</button>
          </header>
          {#if rows[title]}
            <ul class="strip">{#each rows[title]! as it (it.id)}{@render card(it, true)}{/each}</ul>
          {:else}
            <p class="loading-row">Loading…</p>
          {/if}
        </section>
      {/each}
    {:else if data && data.items.length === 0 && !loading}
      <div class="message"><p>Nothing matches. Try fewer words, fewer tags or another build.</p></div>
    {:else if data}
      <ul class="grid" class:dim={loading}>
        {#each data.items as it (it.id)}{@render card(it)}{/each}
      </ul>
      <nav class="pager" aria-label="Pages">
        <button disabled={page <= 1 || loading} onclick={() => go(page - 1)}>Previous</button>
        <span>Page {page} of {compact.format(pages)}</span>
        <button disabled={page >= pages || loading} onclick={() => go(page + 1)}>Next</button>
      </nav>
    {/if}
  </div>
</section>

{#if open}
  <WorkshopPanel item={open} {busy} {installing} onInstall={install} onClose={() => (open = null)}
    onDependents={dependents} onInCollections={inCollections} onAuthor={byAuthor} onOpen={(it) => (open = it)} />
{/if}

<style>
  .workshop {
    display: flex;
    flex-direction: column;
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
  .bar.sub {
    padding-top: 6px;
    padding-bottom: 6px;
  }
  .bar input[type="search"] {
    width: 240px;
  }
  .views,
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .views {
    margin-right: 6px;
  }
  .views button,
  .seg button {
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--muted);
  }
  .views button:hover:not(:disabled),
  .seg button:hover:not(:disabled) {
    color: var(--text);
  }
  .views button.active,
  .seg button.active {
    background: var(--accent-soft);
    color: var(--accent-text);
    font-weight: 700;
  }
  .active-filter {
    border-color: var(--accent-text);
    color: var(--accent-text);
  }
  .brand-img {
    border-radius: 3px;
  }
  .count {
    margin-left: auto;
    color: var(--muted);
  }
  .filters {
    padding: 10px 16px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }
  .hint {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 13px;
    color: var(--muted);
    background: transparent;
  }
  .chip.in {
    color: var(--text);
    background: var(--accent-soft);
    border-color: var(--accent-text);
  }
  .chip.out {
    color: var(--danger);
    border-color: color-mix(in oklab, var(--danger) 60%, transparent);
    text-decoration: line-through;
  }
  .more {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 18px;
    margin-top: 10px;
    color: var(--muted);
  }
  .more input[type="date"] {
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 4px 6px;
    color: var(--text);
  }
  .more .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .context {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    background: color-mix(in oklab, var(--accent) 14%, var(--panel));
  }
  .results {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 16px;
  }
  .dim {
    opacity: 0.5;
  }

  /* Home: rows like Steam's hub */
  .row {
    margin-bottom: 22px;
  }
  .row header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .row h2 {
    margin: 0;
    font-size: 18px;
  }
  .strip {
    list-style: none;
    margin: 0;
    padding: 0 0 8px;
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 210px;
    gap: 12px;
    overflow-x: auto;
  }
  .loading-row {
    color: var(--muted);
    height: 220px;
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
    position: relative;
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
  .kind {
    position: absolute;
    left: 8px;
    bottom: 8px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 12px;
    background: rgb(0 0 0 / 0.75);
    color: #fff;
  }
  .body {
    padding: 10px 12px 4px;
    flex: 1;
    min-width: 0;
  }
  .title {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    white-space: normal;
    text-align: left;
    padding: 0;
    border: 0;
    background: transparent;
    font-size: 15px;
    font-weight: 700;
    line-height: 1.25;
    margin-bottom: 2px;
  }
  .title:hover {
    text-decoration: underline;
  }
  .author {
    padding: 0;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
  .author:hover:not(:disabled) {
    background: transparent;
    text-decoration: underline;
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
  .meta span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .stars {
    gap: 1px !important;
    color: var(--line);
  }
  .stars .lit {
    color: var(--accent-text);
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
  .card.small .body {
    padding-bottom: 0;
  }
  .card.small .title {
    font-size: 14px;
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
