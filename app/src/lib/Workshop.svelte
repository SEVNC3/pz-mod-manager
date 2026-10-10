<script lang="ts">
  import { onMount } from "svelte";
  import { api, newQuery, type Browse, type Query, type WorkshopItem } from "./api";
  import Icon from "./Icon.svelte";
  import WorkshopPanel from "./WorkshopPanel.svelte";

  let { busy, defaultBuild, onInstall, viewpoint = false, hidden = false }: {
    busy: boolean;
    /** Kept mounted while another tab shows, so searches, filters and the open panel survive. */
    hidden?: boolean;
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
  const TAG_LABELS: Record<string, string> = { "Clothing/Armor": "Clothing", "Language/Translation": "Translations", "Silly/Fun": "Silly & fun", "Pop Culture": "Pop culture" };
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
  /** Posted / updated within the last N days (0 = any time): presets instead of the native date picker. */
  let within = $state({ posted: 0, updated: 0 });
  /** What a "needs this" / "collections with this" / author list is about. */
  let context = $state<{ kind: "child" | "author"; item: WorkshopItem } | null>(null);
  let authorPage = $state(1);
  let data = $state<Browse | null>(null);
  let rows = $state<Record<string, WorkshopItem[] | null>>({});
  let rowErrors = $state<Record<string, string>>({});
  /** Steam's errors in plain words (a 429 is Steam asking us to slow down). */
  const plainError = (e: unknown) => {
    const s = String(e);
    return /429|limiting requests/i.test(s) ? "Steam is limiting requests right now. Wait a minute and try again." : `Couldn't reach the Steam Workshop: ${s.replace(/^Error:\s*/, "")}`;
  };
  let loading = $state(false);
  let error = $state("");
  let installing = $state<string | null>(null);
  let open = $state<WorkshopItem | null>(null);
  let seq = 0;

  const WITHIN: [number, string][] = [[0, "Any time"], [1, "Last 24 hours"], [7, "Last week"], [30, "Last month"], [90, "Last 3 months"], [180, "Last 6 months"], [365, "Last year"]];
  const since = (days: number) => (days ? Math.floor(Date.now() / 1000) - days * 86400 : 0);
  /** The request as Steam wants it: build in the tags, libraries as a filter, dates as unix times. */
  function request(over: Partial<Query> = {}): Query {
    const required = [...q.required.filter((t) => !t.startsWith("Build ")), ...(build ? [`Build ${build}`] : [])];
    const libraries = sortChoice === "libraries";
    return {
      ...$state.snapshot(q),
      sort: libraries ? "totaluniquesubscribers" : sortChoice,
      libraries,
      required,
      created_from: since(within.posted),
      created_to: 0,
      updated_from: since(within.updated),
      updated_to: 0,
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

  /** Every Home row at once; a row that fails shows its own retry instead of hiding the others. */
  async function loadHome() {
    const mine = ++seq;
    error = "";
    await Promise.all(HOME_ROWS.filter(([title]) => !rows[title]).map(([title, over]) => loadRow(mine, title, over)));
  }
  async function loadRow(mine: number, title: string, over: Partial<Query>) {
    delete rowErrors[title];
    try {
      const r = await api.browse(newQuery({ ...over, required: build ? [`Build ${build}`] : [] }));
      if (mine === seq) rows[title] = r.items.slice(0, 12);
    } catch (e) {
      if (mine === seq) rowErrors[title] = plainError(e);
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
    within = { posted: 0, updated: 0 };
    go(1);
  }
  const filterCount = $derived(q.required.length + q.excluded.length + (q.incompatible ? 1 : 0) + (within.posted ? 1 : 0) + (within.updated ? 1 : 0));

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
  /** A card's blurb: no BBCode, images or bare links. */
  const plain = (s: string) =>
    s.replace(/\[img\][\s\S]*?\[\/img\]/gi, "").replace(/https?:\/\/\S+/g, "").replace(/\[\/?[a-z0-9*]+(=[^\]]*)?\]/gi, "").replace(/\s+/g, " ").trim();
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

<section class="workshop" aria-label="Steam Workshop" {hidden}>
  <aside class="side" aria-label="Browse options">
    <div class="views" role="tablist" aria-label="Workshop view">
      <button type="button" role="tab" aria-selected={view !== "home"} class:active={view !== "home"} onclick={() => show("browse")}>
        <Icon name="search" size={14} /> Browse
      </button>
      <button type="button" role="tab" aria-selected={view === "home"} class:active={view === "home"} onclick={() => show("home")}>
        <Icon name="flame" size={14} /> Home
      </button>
    </div>

    <form class="search" onsubmit={submit}>
      <input type="search" bind:value={queryText} placeholder="Search the Workshop" aria-label="Search the Workshop"
        title={'Steam search: "exact phrase", AND, OR, NOT, +must, -exclude'} />
      <div class="pair">
        <select bind:value={q.scope} aria-label="Search in" title="Where the words must appear">
          <option value={0}>Anywhere</option>
          <option value={1}>Titles</option>
          <option value={2}>Descriptions</option>
        </select>
        <button type="submit"><Icon name="search" size={14} /> Search</button>
      </div>
      {#if q.search}
        <button type="button" class="quiet small" onclick={() => { queryText = q.search = ""; go(1); }}><Icon name="x" size={12} /> Clear "{q.search}"</button>
      {/if}
    </form>

    <label class="field"><span>Game build</span>
      <select bind:value={build} onchange={() => { rows = {}; view === "home" ? loadHome() : go(1); }}>
        <option value="42">Build 42</option>
        <option value="41">Build 41</option>
        <option value="">Any build</option>
      </select>
    </label>

    {#if view === "browse"}
      <div class="field"><span>Show</span>
        <div class="seg" role="radiogroup" aria-label="Show">
          <button type="button" role="radio" aria-checked={!q.collections} class:active={!q.collections} onclick={() => setSection(false)}>Mods</button>
          <button type="button" role="radio" aria-checked={q.collections} class:active={q.collections} onclick={() => setSection(true)}>Collections</button>
        </div>
      </div>
      {#if !q.search}
        <label class="field"><span>Sort by</span>
          <select bind:value={sortChoice} onchange={() => go(1)}>
            {#each sorts as [v, label]}<option value={v}>{label}</option>{/each}
          </select>
        </label>
        {#if sortChoice === "trend"}
          <label class="field"><span>Popular over</span>
            <select bind:value={q.days} onchange={() => go(1)}>
              {#each DAYS as [d, label]}<option value={d}>{label}</option>{/each}
            </select>
          </label>
        {/if}
      {/if}
      <label class="field"><span>Per page</span>
        <select bind:value={q.per_page} onchange={() => go(1)}>
          <option value={30}>30</option>
          <option value={50}>50</option>
        </select>
      </label>

      <div class="field">
        <span>Tags{q.required.length + q.excluded.length ? ` (${q.required.length + q.excluded.length})` : ""}</span>
        <p class="hint">Click to require, again to exclude, again to clear.</p>
        <div class="chips">
          {#each TAGS as t}
            {@const state = q.required.includes(t) ? "in" : q.excluded.includes(t) ? "out" : ""}
            <button type="button" class="chip {state}" onclick={() => cycleTag(t)} aria-pressed={state !== ""}
              title={state === "in" ? `Only ${t}` : state === "out" ? `No ${t}` : t}>
              <span class="box">{#if state === "in"}<Icon name="check" size={11} />{:else if state === "out"}<Icon name="x" size={11} />{/if}</span>{TAG_LABELS[t] ?? t}
            </button>
          {/each}
        </div>
      </div>
      <label class="field"><span>Posted</span>
        <select bind:value={within.posted} onchange={() => go(1)}>
          {#each WITHIN as [d, label]}<option value={d}>{label}</option>{/each}
        </select>
      </label>
      <label class="field"><span>Updated</span>
        <select bind:value={within.updated} onchange={() => go(1)}>
          {#each WITHIN as [d, label]}<option value={d}>{label}</option>{/each}
        </select>
      </label>
      <label class="check"><input type="checkbox" bind:checked={q.incompatible} onchange={() => go(1)} /> Only mods marked incompatible</label>
      {#if filterCount}<button type="button" class="quiet small" onclick={clearFilters}><Icon name="x" size={12} /> Clear filters</button>{/if}
    {/if}

    {#if viewpoint}
      <div class="field vp"><span>Project Viewpoint</span>
        <button type="button" onclick={() => { queryText = q.search = "Project Viewpoint"; view = "browse"; context = null; go(1); }}>
          <Icon name="layers" size={14} /> Viewpoint add-ons
        </button>
        <button type="button" class="quiet" onclick={() => api.openUrl(VIEWPOINT_DISCORD)} title="Project Viewpoint's community Discord: help, bug reports, news">
          <img src="/brand/discord.png" alt="" width="16" height="16" class="brand-img" /> Viewpoint Discord
        </button>
      </div>
    {/if}
  </aside>

  <div class="main">
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

  <div class="results blueprint" aria-busy={loading}>
    {#if error}
      <div class="message">
        <p>{plainError(error)}</p>
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
          {:else if rowErrors[title]}
            <div class="message inline"><p>{rowErrors[title]}</p>
              <button onclick={() => loadRow(seq, title, over)}><Icon name="refresh" size={14} />Try again</button></div>
          {:else}
            <p class="loading-row">Loading…</p>
          {/if}
        </section>
      {/each}
    {:else if !data && loading}
      <div class="message"><p>Loading the Workshop…</p></div>
    {:else if data && data.items.length === 0 && !loading}
      <div class="message"><p>Nothing matches. Try fewer words, fewer tags or another build.</p>
        <div class="msg-actions">
          {#if q.search}<button onclick={() => { queryText = q.search = ""; go(1); }}><Icon name="x" size={14} />Clear search</button>{/if}
          {#if filterCount}<button onclick={clearFilters}><Icon name="x" size={14} />Clear filters</button>{/if}
        </div>
      </div>
    {:else if data}
      <p class="count">{compact.format(data.total)} {view === "author" ? "mods" : q.collections ? "collections" : "mods"}{q.search ? ` for "${q.search}"` : ""}{loading ? " · loading…" : ""}</p>
      <ul class="grid" class:dim={loading}>
        {#each data.items as it (it.id)}{@render card(it)}{/each}
      </ul>
      <nav class="pager" aria-label="Pages">
        <button disabled={page <= 1 || loading} onclick={() => go(page - 1)}><Icon name="chevron-left" size={14} />Previous</button>
        <span>Page {page} of {compact.format(pages)}</span>
        <button disabled={page >= pages || loading} onclick={() => go(page + 1)}>Next<Icon name="chevron-right" size={14} /></button>
      </nav>
    {/if}
  </div>
  </div>
</section>

{#if open && !hidden}
  <WorkshopPanel item={open} {busy} {installing} onInstall={install} onClose={() => (open = null)}
    onDependents={dependents} onInCollections={inCollections} onAuthor={byAuthor} onOpen={(it) => (open = it)} />
{/if}

<style>
  .workshop {
    display: grid;
    grid-template-columns: 270px 1fr;
    min-height: 0;
  }
  .side {
    overflow-y: auto;
    min-height: 0;
    padding: 12px 14px 24px;
    border-right: 1px solid var(--line);
    background: var(--panel);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }
  .views,
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .views button,
  .seg button {
    flex: 1;
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
    color: var(--on-selected);
    font-weight: 700;
  }
  .search {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .pair {
    display: flex;
    gap: 6px;
  }
  .pair select {
    flex: 1;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .field > span {
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  .field select,
  .field > button {
    width: 100%;
  }
  .vp {
    border-top: 1px solid var(--line);
    padding-top: 12px;
  }
  .small {
    align-self: flex-start;
    padding: 2px 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .brand-img {
    border-radius: 3px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .chips {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 4px;
  }
  .chip {
    justify-content: flex-start;
    gap: 7px;
    padding: 3px 6px;
    font-size: 13px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip .box {
    flex: none;
    width: 15px;
    height: 15px;
    display: grid;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 3px;
  }
  .chip.in {
    background: var(--accent-soft);
    color: var(--on-selected);
  }
  .chip.in .box {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .chip.out {
    color: var(--danger);
    text-decoration: line-through;
  }
  .chip.out .box {
    border-color: var(--danger);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }
  .count {
    margin: 0 0 12px;
    color: var(--muted);
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
  /* words on the blueprint sit on panel strips, like the cards */
  .count,
  .pager,
  .message,
  .row header {
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  .count {
    display: inline-block;
    padding: 4px 12px;
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
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
    padding: 6px 8px 6px 14px;
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
    border-radius: 8px;
    box-shadow: var(--card-shadow);
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
    width: fit-content;
    margin: 18px auto 6px;
    padding: 6px 10px;
    color: var(--muted);
  }
  .message {
    max-width: 480px;
    margin: 10vh auto 0;
    padding: 18px 22px;
    text-align: center;
    color: var(--muted);
  }
  .message.inline {
    margin: 0;
    max-width: none;
  }
  .msg-actions {
    display: flex;
    justify-content: center;
    gap: 8px;
  }
</style>
