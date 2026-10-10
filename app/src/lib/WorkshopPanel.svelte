<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ChangeNote, type Comments, type WorkshopInfo, type WorkshopItem } from "./api";
  import { bbcode } from "./bbcode";
  import { compact, fmtDate, fmtSize, workshopUrl } from "./format";
  import Icon from "./Icon.svelte";
  import InstallButton from "./InstallButton.svelte";
  import Stars from "./Stars.svelte";
  import { t } from "./i18n.svelte";

  // The item panel: everything Steam's item page shows, plus Install.
  let { item, busy, installing, onInstall, onClose, onDependents, onInCollections, onAuthor, onOpen }: {
    /** The card it was opened from (has the rating and author, which the details API lacks). */
    item: WorkshopItem;
    busy: boolean;
    installing: string | null;
    onInstall: (id: string) => void;
    onClose: () => void;
    onDependents: (it: WorkshopItem) => void;
    onInCollections: (it: WorkshopItem) => void;
    onAuthor: (it: WorkshopItem) => void;
    onOpen: (it: WorkshopItem) => void;
  } = $props();

  let info = $state<WorkshopInfo | null>(null);
  // one error per tab, so a failed comments load doesn't hide the description
  let errors = $state({ about: "", changes: "", comments: "" });
  // answers for an item or page we've moved away from are dropped
  let req = 0;
  let reqChanges = 0;
  let reqComments = 0;
  let tab = $state<"about" | "changes" | "comments">("about");
  let shot = $state(0);
  let changes = $state<ChangeNote[] | null>(null);
  let changesPage = $state(1);
  let comments = $state<Comments | null>(null);
  let commentsStart = $state(0);

  $effect(() => {
    const id = item.id;
    info = null;
    errors = { about: "", changes: "", comments: "" };
    tab = "about";
    shot = 0;
    changes = null;
    comments = null;
    changesPage = 1;
    commentsStart = 0;
    const mine = ++req;
    api.workshopInfo(id).then((r) => mine === req && (info = r)).catch((e) => mine === req && (errors.about = String(e)));
  });

  // the card's numbers (rating, author) win; the details call adds the full description
  const it = $derived<WorkshopItem>(info?.item ? { ...info.item, stars: item.stars, votes: item.votes, author: item.author, have: item.have } : item);
  const images = $derived(info ? (info.screenshots.length ? info.screenshots : [it.preview_url].filter(Boolean)) : [it.preview_url].filter(Boolean));

  async function loadChanges(p: number) {
    const mine = ++reqChanges;
    changesPage = p;
    changes = null;
    errors.changes = "";
    try {
      const r = await api.workshopChanges(item.id, p);
      if (mine === reqChanges) changes = r;
    } catch (e) {
      if (mine === reqChanges) errors.changes = String(e);
    }
  }
  async function loadComments(start: number) {
    const mine = ++reqComments;
    commentsStart = start;
    comments = null;
    errors.comments = "";
    try {
      const r = await api.workshopComments(it.creator, item.id, start);
      if (mine === reqComments) comments = r;
    } catch (e) {
      if (mine === reqComments) errors.comments = String(e);
    }
  }

  // a modal dialog: focus moves in, Tab stays inside, Esc closes, focus returns where it was
  let panel = $state<HTMLElement>();
  onMount(() => {
    const back = document.activeElement as HTMLElement | null;
    panel?.focus();
    return () => back?.focus?.();
  });
  function dialogKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    } else if (e.key === "Tab" && panel) {
      const f = [...panel.querySelectorAll<HTMLElement>("button:not(:disabled), a[href], [tabindex]:not([tabindex='-1'])")];
      if (!f.length) return;
      const [first, last] = [f[0], f[f.length - 1]];
      if (e.shiftKey && (document.activeElement === first || document.activeElement === panel)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }
  function pick(t: typeof tab) {
    tab = t;
    if (t === "changes" && !changes) loadChanges(1);
    if (t === "comments" && !comments) loadComments(0);
  }

  /** Links in descriptions open in the browser, never inside the app window. */
  function linkClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a[data-href]");
    if (!a) return;
    e.preventDefault();
    api.openUrl(a.getAttribute("data-href")!);
  }

  const date = (t: number) => fmtDate(t, "long") || "—";
</script>

<div class="backdrop" onclick={onClose} aria-hidden="true"></div>
<div class="panel" role="dialog" aria-modal="true" aria-label={it.title} tabindex="-1" bind:this={panel} onkeydown={dialogKey}>
  <header>
    <div class="titles">
      <h2>{it.title}</h2>
      {#if it.author}
        <button class="quiet link" onclick={() => onAuthor(it)} title={t("panel.moreByAuthorTip")}>{t("panel.byAuthor", { author: it.author })}</button>
      {/if}
    </div>
    <button class="quiet icon" onclick={onClose} aria-label={t("common.close")} title={t("panel.closeTip")}><Icon name="x" size={16} /></button>
  </header>

  <div class="scroll">
    {#if images.length}
      <div class="gallery">
        <img class="main" src={images[shot] ?? images[0]} alt="" />
        {#if images.length > 1 || info?.videos.length}
          <div class="thumbs">
            {#each images as src, i}
              <button class:on={i === shot} onclick={() => (shot = i)} aria-label={t("panel.screenshotLabel", { n: i + 1 })}><img {src} alt="" loading="lazy" /></button>
            {/each}
            {#each info?.videos ?? [] as v}
              <button class="video" onclick={() => api.openUrl(`https://www.youtube.com/watch?v=${v}`)} title={t("panel.youtubeTip")} aria-label={t("panel.youtubeLabel")}>
                <img src={`https://img.youtube.com/vi/${v}/mqdefault.jpg`} alt="" loading="lazy" /><span><Icon name="play" size={18} fill /></span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="actions">
      <InstallButton item={it} {busy} {installing} {onInstall} all={(info?.members.length || it.num_children) ? t("panel.installAll", { count: info?.members.length || it.num_children }) : undefined} />
      <button onclick={() => api.openUrl(workshopUrl(it.id))}><Icon name="steam" size={14} />{t("panel.workshopPage")}</button>
    </div>

    <dl class="stats">
      {#if it.votes > 0}
        <div><dt>{t("panel.rating")}</dt><dd class="stars"><Stars stars={it.stars} /> <span class="muted">{compact.format(it.votes)}</span></dd></div>
      {/if}
      <div><dt>{t("panel.subscribers")}</dt><dd>{compact.format(it.subscriptions)}</dd></div>
      <div><dt>{t("panel.visitors")}</dt><dd>{compact.format(it.views)}</dd></div>
      <div><dt>{t("panel.favourites")}</dt><dd>{compact.format(it.favorited)}</dd></div>
      <div><dt>{t("panel.comments")}</dt><dd>{compact.format(it.comments)}</dd></div>
      {#if !it.collection}<div><dt>{t("panel.size")}</dt><dd>{fmtSize(it.file_size)}</dd></div>{/if}
      <div><dt>{t("panel.posted")}</dt><dd>{date(it.time_created)}</dd></div>
      <div><dt>{t("panel.updated")}</dt><dd>{date(it.time_updated)}</dd></div>
    </dl>
    {#if it.tags.length}
      <p class="tags">{#each it.tags as t}<span>{t}</span>{/each}</p>
    {/if}

    <div class="links">
      {#if !it.collection}
        <button class="quiet" onclick={() => onDependents(it)}><Icon name="layers" size={14} />{t("panel.modsThatNeed")}</button>
        <button class="quiet" onclick={() => onInCollections(it)}><Icon name="folder" size={14} />{t("panel.collectionsWith")}</button>
      {/if}
      {#if it.creator}
        <button class="quiet" onclick={() => onAuthor(it)}><Icon name="search" size={14} />{t("panel.moreBy", { author: it.author || t("panel.thisAuthor") })}</button>
      {/if}
    </div>

    {#if info?.required.length}
      <h3>{t("panel.needs", { count: info.required.length })}</h3>
      <ul class="mini">
        {#each info.required as r (r.id)}
          <li>
            <button class="quiet link" onclick={() => onOpen(r)}>{r.title}</button>
            {#if r.have}<span class="have small"><Icon name="check" size={12} /> {t("common.installed")}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if info?.members.length}
      <h3>{t("panel.items", { count: info.members.length })}</h3>
      <ul class="mini">
        {#each info.members as m (m.id)}
          <li>
            {#if m.preview_url}<img src={m.preview_url} alt="" loading="lazy" />{/if}
            <button class="quiet link" onclick={() => onOpen(m)}>{m.title}</button>
            {#if m.have}<span class="have small"><Icon name="check" size={12} /> {t("common.installed")}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}

    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === "about"} class:on={tab === "about"} onclick={() => pick("about")}>{t("panel.description")}</button>
      <button role="tab" aria-selected={tab === "changes"} class:on={tab === "changes"} onclick={() => pick("changes")}>{t("panel.changeNotes")}</button>
      <button role="tab" aria-selected={tab === "comments"} class:on={tab === "comments"} onclick={() => pick("comments")}>{t("panel.commentsTab", { n: compact.format(it.comments) })}</button>
    </div>

    {#if errors[tab]}<p class="error">{errors[tab]}</p>{/if}
    {#if tab === "about"}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="desc" onclick={linkClick}>
        {#if info?.item}{@html bbcode(info.item.description)}{:else if !errors.about}<p class="muted">{t("panel.loading")}</p>{/if}
      </div>
    {:else if tab === "changes"}
      {#if errors.changes}<!-- shown above -->
      {:else if !changes}<p class="muted">{t("panel.loading")}</p>
      {:else if !changes.length}<p class="muted">{changesPage > 1 ? t("panel.noOlderChanges") : t("panel.noChanges")}</p>
      {:else}
        <ol class="notes">
          {#each changes as c}<li><strong>{date(c.time)}</strong><p>{c.text}</p></li>{/each}
        </ol>
      {/if}
      <nav class="pager">
        <button disabled={changesPage <= 1} onclick={() => loadChanges(changesPage - 1)}><Icon name="chevron-left" size={14} />{t("panel.newer")}</button>
        <button disabled={!changes || changes.length < 10} onclick={() => loadChanges(changesPage + 1)}>{t("panel.older")}<Icon name="chevron-right" size={14} /></button>
      </nav>
    {:else}
      {#if errors.comments}<!-- shown above -->
      {:else if !comments}<p class="muted">{t("panel.loading")}</p>
      {:else if !comments.comments.length}<p class="muted">{t("panel.noComments")}</p>
      {:else}
        <ol class="notes">
          {#each comments.comments as c}<li><strong>{c.author}</strong> <span class="muted">{date(c.time)}</span><p>{c.text}</p></li>{/each}
        </ol>
        <nav class="pager">
          <button disabled={commentsStart <= 0} onclick={() => loadComments(Math.max(0, commentsStart - 20))}><Icon name="chevron-left" size={14} />{t("panel.newer")}</button>
          <span class="muted">{t("panel.commentsRange", { from: commentsStart + 1, to: Math.min(commentsStart + 20, comments.total), total: comments.total })}</span>
          <button disabled={commentsStart + 20 >= comments.total} onclick={() => loadComments(commentsStart + 20)}>{t("panel.older")}<Icon name="chevron-right" size={14} /></button>
        </nav>
      {/if}
      <p class="muted small-note">{t("panel.postHint")}</p>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.45);
    z-index: 20;
  }
  .panel:focus {
    outline: none;
  }
  .panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(620px, 92vw);
    z-index: 21;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-left: 1px solid var(--line);
    box-shadow: var(--shadow);
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--line);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    line-height: 1.2;
  }
  .icon {
    padding: 4px;
  }
  .link {
    color: var(--muted);
    white-space: normal;
    text-align: left;
  }
  .scroll {
    overflow-y: auto;
    padding: 14px 16px 30px;
  }
  .gallery .main {
    display: block;
    width: 100%;
    max-height: 340px;
    object-fit: contain;
    background: #000;
    border-radius: 8px;
  }
  .thumbs {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding: 8px 0 2px;
  }
  .thumbs button {
    flex: none;
    position: relative;
    padding: 0;
    width: 92px;
    height: 52px;
    border-radius: 4px;
    overflow: hidden;
    border: 2px solid transparent;
    background: #000;
  }
  .thumbs button.on {
    border-color: var(--accent-text);
  }
  .thumbs img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .video span {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: #fff;
    background: rgb(0 0 0 / 0.35);
  }
  .actions,
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 12px 0;
  }
  .have {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--accent-text);
    font-weight: 700;
  }
  .have.small {
    font-size: 12px;
    margin-left: 6px;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 6px;
    margin: 0;
  }
  .stats div {
    background: var(--raised);
    border-radius: 6px;
    padding: 6px 10px;
  }
  dt {
    font-size: 12px;
    color: var(--muted);
  }
  dd {
    margin: 0;
    font-weight: 700;
  }
  .stars {
    display: flex;
    align-items: center;
    gap: 1px;
    color: var(--line);
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 10px 0 0;
  }
  .tags span {
    font-size: 12px;
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 1px 8px;
    color: var(--muted);
  }
  h3 {
    margin: 16px 0 6px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  .mini {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .mini li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--line);
  }
  .mini img {
    width: 48px;
    height: 30px;
    object-fit: cover;
    border-radius: 3px;
  }
  .mini .link {
    color: var(--text);
    text-decoration: none;
  }
  .tabs {
    display: flex;
    gap: 2px;
    margin: 18px 0 10px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    padding: 8px 12px 7px;
  }
  .tabs button:hover:not(:disabled),
  .tabs button.on {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-bottom-color: var(--on-selected);
  }
  .desc {
    overflow-wrap: anywhere;
    line-height: 1.55;
  }
  .desc :global(img) {
    max-width: 100%;
    border-radius: 4px;
  }
  .desc :global(a) {
    color: var(--accent-text);
  }
  .desc :global(h3),
  .desc :global(h4),
  .desc :global(h5) {
    margin: 14px 0 6px;
    color: var(--text);
    text-transform: none;
    letter-spacing: 0;
    font-size: 16px;
  }
  .desc :global(blockquote) {
    margin: 8px 0;
    padding: 6px 12px;
    border-left: 3px solid var(--line);
    color: var(--muted);
  }
  .desc :global(pre) {
    white-space: pre-wrap;
    background: var(--raised);
    padding: 8px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .desc :global(.spoiler) {
    background: var(--text);
    color: var(--text);
  }
  .desc :global(.spoiler:hover) {
    color: var(--bg);
  }
  .desc :global(table) {
    border-collapse: collapse;
  }
  .desc :global(td),
  .desc :global(th) {
    border: 1px solid var(--line);
    padding: 3px 6px;
  }
  .notes {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .notes li {
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }
  .notes p {
    margin: 4px 0 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .pager {
    gap: 12px;
    margin-top: 10px;
  }
  .muted {
    color: var(--muted);
  }
  .small-note {
    font-size: 12px;
  }
</style>
