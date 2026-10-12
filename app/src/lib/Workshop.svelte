<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, newQuery, VIEWPOINT, type Browse, type Query, type WorkshopItem } from "./api";
  import { compact, fmtDate, fmtNum, plain } from "./format";
  import { keyedError, t, th } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import InstallButton from "./InstallButton.svelte";
  import Stars from "./Stars.svelte";
  import WorkshopPanel from "./WorkshopPanel.svelte";
  import { prefs } from "./prefs.svelte";
  import { arrowPick } from "./ui";

  let { busy, defaultBuild, installed, onInstall, onCollection, viewpoint = false, hidden = false }: {
    busy: boolean;
    /** Kept mounted while another tab shows, so searches, filters and the open panel survive. */
    hidden?: boolean;
    defaultBuild: number | null;
    /** Workshop items installed now: the Installed marks follow it (installs and removals made anywhere). */
    installed: Set<string>;
    onInstall: (id: string) => Promise<boolean>;
    /** A collection's Install all: read it first, to choose what to take (Import's collection picker). */
    onCollection: (id: string) => void;
    /** Project Viewpoint mode: offer its add-ons and community. */
    viewpoint?: boolean;
  } = $props();

  // Steam's own sorts and filters for the Project Zomboid Workshop (docs/WORKSHOP-PARITY.md)
  // [value, label key]: labels are translated where they're shown
  const ITEM_SORTS: [string, string][] = [
    ["trend", "workshop.sorts.popular"],
    ["totaluniquesubscribers", "workshop.sorts.subscribed"],
    ["toprated", "workshop.sorts.topRated"],
    ["mostrecent", "workshop.sorts.newest"],
    ["lastupdated", "workshop.sorts.updated"],
    ["libraries", "workshop.sorts.libraries"],
    ["num_parent_collections", "workshop.sorts.collections"],
  ];
  const COLLECTION_SORTS = ITEM_SORTS.filter(([v]) => ["trend", "toprated", "mostrecent", "lastupdated"].includes(v));
  const DAYS: [number, string][] = [[1, "workshop.days.today"], [7, "workshop.days.week"], [30, "workshop.days.month"], [90, "workshop.days.months3"], [180, "workshop.days.months6"], [365, "workshop.days.year"]];
  // Steam's tag values; shown as workshop.tagNames.<value without punctuation>
  const TAGS = ["Animals", "Audio", "Balance", "Building", "Clothing/Armor", "Farming", "Food", "Framework", "Hardmode", "Interface", "Items",
    "Language/Translation", "Literature", "Map", "Military", "Misc", "Models", "Multiplayer", "Pop Culture", "QoL", "Realistic", "Silly/Fun",
    "Skills", "Textures", "Traits", "Vehicles", "Weapons", "WIP"];
  const tagName = (tag: string) => t(`workshop.tagNames.${tag.replace(/\W/g, "")}`);
  const HOME_ROWS: [string, Partial<Query>][] = [
    ["workshop.home.popularWeek", { sort: "trend", days: 7 }],
    ["workshop.home.subscribed", { sort: "totaluniquesubscribers" }],
    ["workshop.home.newest", { sort: "mostrecent" }],
    ["workshop.home.updated", { sort: "lastupdated" }],
    ["workshop.home.required", { sort: "totaluniquesubscribers", libraries: true }],
  ];

  let view = $state<"home" | "browse" | "author">(prefs.workshopView);
  let q = $state<Query>(newQuery({ per_page: prefs.perPage }));
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
    return /429|limiting requests/i.test(s) ? t("workshop.errors.rateLimited") : t("workshop.errors.unreachable", { error: s.replace(/^Error:\s*/, "") });
  };
  let loading = $state(false);
  let error = $state("");
  let installing = $state<string | null>(null);
  let open = $state<WorkshopItem | null>(null);
  let seq = 0;

  // --- Back: the places the item panel's links left (another item, "Mods that need this", an author) ---
  type Place = { open: WorkshopItem | null; view: typeof view; context: typeof context; q: Query; sortChoice: string; authorPage: number; data: Browse | null };
  let history = $state<Place[]>([]);
  /** Remember where we are before a panel link goes elsewhere. */
  const leave = () => (history = [...history.slice(-29), $state.snapshot({ open, view, context, q, sortChoice, authorPage, data }) as Place]);
  function back() {
    const p = history.at(-1);
    if (!p) return;
    history = history.slice(0, -1);
    ({ open, view, context, q, sortChoice, authorPage, data } = p);
    seq++; // answers still on their way are for the place we left
    loading = false;
    error = "";
  }
  /** Closing the panel ends its trail, unless a list it led to (an author, what needs it) is still open. */
  function closePanel() {
    open = null;
    if (!context) history = [];
  }
  function backKey(e: KeyboardEvent) {
    if (!hidden && e.altKey && e.key === "ArrowLeft" && history.length) {
      e.preventDefault();
      back();
    }
  }
  /** Leave out what's installed (prefs: "Hide installed"). */
  const shown = (list: WorkshopItem[]) => (prefs.hideInstalled ? list.filter((i) => !i.have) : list);

  const WITHIN: [number, string][] = [[0, "workshop.within.any"], [1, "workshop.within.day"], [7, "workshop.within.week"], [30, "workshop.within.month"], [90, "workshop.within.months3"], [180, "workshop.within.months6"], [365, "workshop.within.year"]];
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
      if (mine === seq) error = t(...keyedError(e));
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
      if (mine === seq) rowErrors[title] = t(...keyedError(e));
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
    leave();
    open = null;
    view = "browse";
    context = { kind: "child", item: it };
    q = newQuery({ child: it.id, collections: false });
    sortChoice = "totaluniquesubscribers";
    go(1);
  }
  function inCollections(it: WorkshopItem) {
    leave();
    open = null;
    view = "browse";
    context = { kind: "child", item: it };
    q = newQuery({ child: it.id, collections: true });
    sortChoice = "trend";
    go(1);
  }
  function byAuthor(it: WorkshopItem) {
    leave();
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
    if ([open, ...(data?.items ?? []), ...Object.values(rows).flat()].find((i) => i?.id === id)?.collection) return onCollection(id);
    installing = id;
    await onInstall(id);
    installing = null;
  }
  // the Installed marks as the library has them (lists loaded later come marked already)
  $effect(() => {
    const have = (i: WorkshopItem) => i.collection ? i.have : installed.has(i.id);
    const mark = (list: WorkshopItem[] | null | undefined) => list?.map((i) => (i.have === have(i) ? i : { ...i, have: have(i) })) ?? null;
    untrack(() => {
      if (data) data.items = mark(data.items)!;
      for (const k of Object.keys(rows)) rows[k] = mark(rows[k]);
      if (open && open.have !== have(open)) open = { ...open, have: have(open) };
    });
  });

  const perPage = $derived(view === "author" ? 30 : q.per_page);
  const pages = $derived(data ? Math.max(1, Math.min(1000, Math.ceil(data.total / perPage))) : 1);
  const page = $derived(view === "author" ? authorPage : q.page);
  const builds = (tags: string[]) => tags.filter((t) => /^Build \d+$/.test(t)).map((t) => t.replace("Build ", "B"));
  const sorts = $derived(q.collections ? COLLECTION_SORTS : ITEM_SORTS);

  onMount(() => (view === "home" ? loadHome() : load()));
</script>

{#snippet stars(it: WorkshopItem)}
  {#if it.votes > 0}
    <span class="stars" title={t("workshop.starsTip", { stars: it.stars, count: it.votes, votes: fmtNum(it.votes) })}>
      <Stars stars={it.stars} />
      <span class="votes">{compact.format(it.votes)}</span>
    </span>
  {/if}
{/snippet}

{#snippet card(it: WorkshopItem, small = false)}
  <li class="card" class:small>
    <button class="thumb" onclick={() => (open = it)} aria-label={t("workshop.detailsLabel", { title: it.title })}>
      {#if it.preview_url}<img src={it.preview_url} alt="" loading="lazy" />{/if}
      {#if it.collection}<span class="kind"><Icon name="folder" size={12} /> {t("workshop.collectionBadge", { count: it.num_children })}</span>{/if}
    </button>
    <div class="body">
      <button class="title" onclick={() => (open = it)} title={it.title}>{it.title}</button>
      {#if it.author && !small}
        <button class="quiet author" onclick={() => byAuthor(it)} title={t("workshop.moreByAuthorTip")}>{t("workshop.byAuthor", { author: it.author })}</button>
      {/if}
      <p class="meta">
        {@render stars(it)}
        <span title={t("workshop.subscribersTip")}>{t("workshop.subs", { count: it.subscriptions, n: compact.format(it.subscriptions) })}</span>
        {#if !small}
          <span title={t("workshop.favouritesTip")}><Icon name="star" size={12} /> {compact.format(it.favorited)}</span>
          <span title={t("workshop.commentsTip")}><Icon name="message" size={12} /> {compact.format(it.comments)}</span>
          <span title={t("workshop.postedTip", { date: fmtDate(it.time_created, "long") })}>{t("workshop.updatedOn", { date: fmtDate(it.time_updated, "long") })}</span>
          {#each builds(it.tags) as b}<span class="tag">{b}</span>{/each}
        {/if}
      </p>
      {#if it.description && !small}<p class="desc">{plain(it.description)}</p>{/if}
      {#if it.num_children > 0 && !it.collection && !small}
        <p class="needs">{t("workshop.needs", { count: it.num_children })}</p>
      {/if}
    </div>
    <div class="actions">
      <InstallButton item={it} {busy} {installing} onInstall={install} />
      <button class="quiet" onclick={() => (open = it)}>{t("workshop.details")}</button>
    </div>
  </li>
{/snippet}

<svelte:window onkeydown={backKey} onmouseup={(e) => !hidden && e.button === 3 && history.length && (e.preventDefault(), back())} />

<section class="workshop" aria-label={t("workshop.label")} {hidden}>
  <aside class="side" aria-label={t("workshop.optionsLabel")}>
    <div class="subtabs" role="tablist" aria-label={t("workshop.viewLabel")} tabindex="-1" onkeydown={(e) => arrowPick(e, ["browse", "home"], view === "home" ? "home" : "browse", (v) => show(v as "browse" | "home"))}>
      <button type="button" role="tab" id="ws-tab-browse" aria-controls="ws-panel" aria-selected={view !== "home"} tabindex={view !== "home" ? 0 : -1} class:active={view !== "home"} onclick={() => show("browse")}>
        <Icon name="search" size={14} /> {t("workshop.browse")}
      </button>
      <button type="button" role="tab" id="ws-tab-home" aria-controls="ws-panel" aria-selected={view === "home"} tabindex={view === "home" ? 0 : -1} class:active={view === "home"} onclick={() => show("home")}>
        <Icon name="flame" size={14} /> {t("workshop.homeTab")}
      </button>
    </div>

    <form class="search" onsubmit={submit}>
      <input type="search" bind:value={queryText} placeholder={t("workshop.searchPlaceholder")} aria-label={t("workshop.searchPlaceholder")}
        title={t("workshop.searchTip")} />
      <div class="pair">
        <select bind:value={q.scope} aria-label={t("workshop.searchInLabel")} title={t("workshop.searchInTip")}>
          <option value={0}>{t("workshop.anywhere")}</option>
          <option value={1}>{t("workshop.titles")}</option>
          <option value={2}>{t("workshop.descriptions")}</option>
        </select>
        <button type="submit"><Icon name="search" size={14} /> {t("workshop.search")}</button>
      </div>
      {#if q.search}
        <button type="button" class="quiet small" onclick={() => { queryText = q.search = ""; go(1); }}><Icon name="x" size={12} /> {t("workshop.clearTerm", { term: q.search })}</button>
      {/if}
    </form>

    <label class="field"><span>{t("workshop.gameBuild")}</span>
      <select bind:value={build} onchange={() => { rows = {}; view === "home" ? loadHome() : go(1); }}>
        <option value="42">{t("workshop.buildN", { n: 42 })}</option>
        <option value="41">{t("workshop.buildN", { n: 41 })}</option>
        <option value="">{t("workshop.anyBuild")}</option>
      </select>
    </label>

    {#if view === "browse"}
      <div class="field"><span>{t("workshop.show")}</span>
        <div class="seg" role="radiogroup" aria-label={t("workshop.show")}>
          <button type="button" role="radio" aria-checked={!q.collections} class:active={!q.collections} onclick={() => setSection(false)}>{t("workshop.mods")}</button>
          <button type="button" role="radio" aria-checked={q.collections} class:active={q.collections} onclick={() => setSection(true)}>{t("workshop.collections")}</button>
        </div>
      </div>
      {#if !q.search}
        <label class="field"><span>{t("workshop.sortBy")}</span>
          <select bind:value={sortChoice} onchange={() => go(1)}>
            {#each sorts as [v, label]}<option value={v}>{t(label)}</option>{/each}
          </select>
        </label>
        {#if sortChoice === "trend"}
          <label class="field"><span>{t("workshop.popularOver")}</span>
            <select bind:value={q.days} onchange={() => go(1)}>
              {#each DAYS as [d, label]}<option value={d}>{t(label)}</option>{/each}
            </select>
          </label>
        {/if}
      {/if}
      <label class="field"><span>{t("workshop.perPage")}</span>
        <select bind:value={q.per_page} onchange={() => go(1)}>
          <option value={30}>30</option>
          <option value={50}>50</option>
        </select>
      </label>

      <div class="field">
        <span>{q.required.length + q.excluded.length ? t("workshop.tagsHeadingCount", { count: q.required.length + q.excluded.length }) : t("workshop.tagsHeading")}</span>
        <p class="hint">{t("workshop.tagsHint")}</p>
        <div class="chips">
          {#each TAGS as tag}
            {@const state = q.required.includes(tag) ? "in" : q.excluded.includes(tag) ? "out" : ""}
            <button type="button" class="chip {state}" onclick={() => cycleTag(tag)} aria-pressed={state !== ""}
              title={state === "in" ? t("workshop.tagOnlyTip", { tag }) : state === "out" ? t("workshop.tagNoTip", { tag }) : tag}>
              <span class="box">{#if state === "in"}<Icon name="check" size={12} />{:else if state === "out"}<Icon name="x" size={12} />{/if}</span>{tagName(tag)}
            </button>
          {/each}
        </div>
      </div>
      <label class="field"><span>{t("workshop.posted")}</span>
        <select bind:value={within.posted} onchange={() => go(1)}>
          {#each WITHIN as [d, label]}<option value={d}>{t(label)}</option>{/each}
        </select>
      </label>
      <label class="field"><span>{t("workshop.updated")}</span>
        <select bind:value={within.updated} onchange={() => go(1)}>
          {#each WITHIN as [d, label]}<option value={d}>{t(label)}</option>{/each}
        </select>
      </label>
      <label class="check"><input type="checkbox" bind:checked={q.incompatible} onchange={() => go(1)} /> {t("workshop.onlyIncompatible")}</label>
      {#if filterCount}<button type="button" class="quiet small" onclick={clearFilters}><Icon name="x" size={12} /> {t("workshop.clearFilters")}</button>{/if}
    {/if}
    <label class="check" title={t("workshop.hideInstalledTip")}><input type="checkbox" bind:checked={prefs.hideInstalled} /> {t("workshop.hideInstalled")}</label>

    {#if viewpoint}
      <!-- i18n-ignore: the mod's name, also the search sent to Steam -->
      <div class="field vp"><span>Project Viewpoint</span>
        <button type="button" onclick={() => { queryText = q.search = "Project Viewpoint"; view = "browse"; context = null; go(1); }}>
          <Icon name="layers" size={14} /> {t("workshop.viewpointAddons")}
        </button>
        <button type="button" class="quiet" onclick={() => api.openUrl(VIEWPOINT.discord)} title={t("workshop.viewpointDiscordTip")}>
          <img src="/brand/discord.png" alt="" width="16" height="16" class="brand-img" /> {t("workshop.viewpointDiscord")}
        </button>
      </div>
    {/if}
  </aside>

  <div class="main" id="ws-panel" role="tabpanel" aria-labelledby={view === "home" ? "ws-tab-home" : "ws-tab-browse"}>
  {#if context}
    <div class="context">
      {#if context.kind === "author"}
        <span>{@html th(data ? "workshop.modsByCount" : "workshop.modsBy", { author: context.item.author || t("workshop.thisAuthor"), count: data?.total ?? 0 })}</span>
      {:else if q.collections}
        <span>{@html th("workshop.collectionsWith", { title: context.item.title })}</span>
      {:else}
        <span>{@html th("workshop.modsThatNeed", { title: context.item.title })}</span>
      {/if}
      {#if history.length}<button class="quiet" onclick={back} title={t("workshop.backTip")}><Icon name="chevron-left" size={14} /> {t("workshop.back")}</button>{/if}
      <button class="quiet" onclick={clearContext}><Icon name="x" size={14} /> {t("workshop.backToBrowsing")}</button>
    </div>
  {/if}

  <div class="results blueprint" aria-busy={loading}>
    {#if error}
      <div class="message">
        <p>{plainError(error)}</p>
        <button onclick={() => (view === "home" ? loadHome() : load())}><Icon name="refresh" size={14} /> {t("workshop.tryAgain")}</button>
      </div>
    {:else if view === "home"}
      {#each HOME_ROWS as [title, over]}
        <section class="row">
          <header>
            <h2>{t(title)}</h2>
            <button class="quiet" onclick={() => viewAll(over)}>{t("workshop.viewAll")}</button>
          </header>
          {#if rows[title]}
            <ul class="strip">{#each shown(rows[title]!) as it (it.id)}{@render card(it, true)}{/each}</ul>
          {:else if rowErrors[title]}
            <div class="message inline"><p>{plainError(rowErrors[title])}</p>
              <button onclick={() => loadRow(seq, title, over)}><Icon name="refresh" size={14} />{t("workshop.tryAgain")}</button></div>
          {:else}
            <p class="loading-row">{t("workshop.loading")}</p>
          {/if}
        </section>
      {/each}
    {:else if !data && loading}
      <div class="message"><p>{t("workshop.loadingWorkshop")}</p></div>
    {:else if data && data.items.length === 0 && !loading}
      <div class="message"><p>{t("workshop.nothingMatches")}</p>
        <div class="msg-actions">
          {#if q.search}<button onclick={() => { queryText = q.search = ""; go(1); }}><Icon name="x" size={14} />{t("workshop.clearSearch")}</button>{/if}
          {#if filterCount}<button onclick={clearFilters}><Icon name="x" size={14} />{t("workshop.clearFilters")}</button>{/if}
        </div>
      </div>
    {:else if data}
      {@const counted = view !== "author" && q.collections ? "workshop.countCollections" : "workshop.countMods"}
      <p class="count">{t(q.search ? `${counted}For` : counted, { count: data.total, n: compact.format(data.total), search: q.search })}{loading ? t("workshop.loadingSuffix") : ""}</p>
      <ul class="grid" class:dim={loading}>
        {#each shown(data.items) as it (it.id)}{@render card(it)}{/each}
      </ul>
      <nav class="pager" aria-label={t("workshop.pagesLabel")}>
        <button disabled={page <= 1 || loading} onclick={() => go(page - 1)}><Icon name="chevron-left" size={14} />{t("workshop.previous")}</button>
        <span>{t("workshop.pageOf", { page, pages: compact.format(pages) })}</span>
        <button disabled={page >= pages || loading} onclick={() => go(page + 1)}>{t("workshop.next")}<Icon name="chevron-right" size={14} /></button>
      </nav>
    {/if}
  </div>
  </div>
</section>

{#if open && !hidden}
  <WorkshopPanel item={open} {busy} {installing} onInstall={install} onClose={closePanel}
    onDependents={dependents} onInCollections={inCollections} onAuthor={byAuthor} onOpen={(it) => { leave(); open = it; }} onBack={history.length ? back : null} />
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
  .seg button {
    flex: 1;
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
    /* Discord's logo is white: darkened on the light theme */
    filter: var(--glyph);
  }
  .hint {
    margin: 0;
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
    white-space: normal; /* longer tag names wrap */
    overflow-wrap: anywhere;
    hyphens: auto;
    text-align: left;
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
    color: var(--on-accent);
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
  .meta :global(span) {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .stars {
    gap: 1px !important;
    color: var(--line);
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
  .pager {
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
