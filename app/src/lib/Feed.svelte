<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api, type Feed, type FeedEntry, type ModRow } from "./api";
  import { bbcode, markdown } from "./bbcode";
  import { fmtDate, fmtDateTime } from "./format";
  import { intlLang, keyedError, numLocale, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import Scrap from "./Scrap.svelte";
  import { linkClick } from "./ui";

  let { mods, order, busy, onShowMod, onUpdate, onCount }: {
    mods: Map<string, ModRow>;
    /** Enabled mod ids. */
    order: string[];
    busy: boolean;
    onShowMod: (id: string) => void;
    onUpdate: (ids: string[]) => Promise<unknown>;
    /** How many updates the feed lists (the tab's count). */
    onCount: (n: number) => void;
  } = $props();

  let feed = $state<Feed | null>(null);
  let error = $state("");
  let loading = $state(false);
  let enabledOnly = $state(false);
  /** History entries that are open, and those showing their older notes too. */
  let opened = $state<Set<string>>(new Set());
  let expanded = $state<Set<string>>(new Set());
  const SHOWN = 3;
  /** Notes pinned to the board; the rest are in the history below. */
  const PINNED = 12;

  async function load() {
    loading = true;
    error = "";
    try {
      feed = await api.feed(true);
      onCount(feed.entries.length);
    } catch (e) {
      error = t(...keyedError(e));
    }
    loading = false;
  }
  onMount(load);
  /** Update an entry's mods, then read the feed again: what's installed now isn't offered again. */
  const update = (ids: string[]) => onUpdate(ids).then(load);

  const toggle = (set: Set<string>, id: string) => {
    const s = new Set(set);
    s.has(id) ? s.delete(id) : s.add(id);
    return s;
  };

  /** Open an entry's full notes in the history and bring it into view. */
  async function readAll(e: FeedEntry) {
    opened = new Set(opened).add(e.item);
    await tick();
    document.getElementById(`feed-${e.item}`)?.scrollIntoView({ behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth", block: "start" });
  }

  const enabled = (e: FeedEntry) => e.mods.some((id) => order.includes(id));
  const shown = $derived((feed?.entries ?? []).filter((e) => !enabledOnly || enabled(e)));
  const pinned = $derived(shown.slice(0, PINNED));
  /** Newest day first, as the entries come. */
  const days = $derived.by(() => {
    const out: [string, FeedEntry[]][] = [];
    const fmt = new Intl.DateTimeFormat(numLocale(), { weekday: "long", month: "long", day: "numeric", year: "numeric" });
    for (const e of shown) {
      const day = fmt.format(new Date(e.updated * 1000));
      if (out.at(-1)?.[0] === day) out.at(-1)![1].push(e);
      else out.push([day, [e]]);
    }
    return out;
  });
  const risky = $derived(shown.filter((e) => e.risks.length && enabled(e)).length);
  const pendingIds = (e: FeedEntry) => e.mods.filter((id) => mods.get(id)?.source.startsWith("workshop:"));

  /** The newest note as plain text, for the pinned scrap. */
  function excerpt(e: FeedEntry): string {
    const raw = e.notes[0]?.text ?? e.bundled?.entries[0]?.text ?? "";
    return raw
      .replace(/\[\/?[a-z*][^\]]*\]/gi, "")
      .replace(/^#+\s*|\s*#+\s*$/gm, "")
      .replace(/[*_`]{1,3}/g, "")
      .replace(/\n{3,}/g, "\n\n")
      .trim();
  }

</script>

<section class="feed blueprint" aria-label={t("feed.title")}>
  <div class="wrap">
    <header class="board-head">
      <div>
        <h2>{t("feed.title")}</h2>
        {#if feed}
          <p class="sub">{feed.basis ? t(`feed.since.${feed.basis}`, { time: fmtDateTime(feed.since) }) : t("feed.since.week")}</p>
        {/if}
      </div>
      <div class="row">
        <label class="check"><input type="checkbox" bind:checked={enabledOnly} />{t("feed.enabledOnly")}</label>
        <button onclick={load} disabled={loading}><Icon name="refresh" size={14} />{loading ? t("feed.loading") : t("feed.refresh")}</button>
      </div>
    </header>

    {#if risky}
      <p class="caution"><Icon name="alert" size={14} /> {t("feed.riskyCount", { count: risky })}</p>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
    {#if feed?.error}<p class="error">{t("feed.notesFailed", { error: feed.error })}</p>{/if}

    {#if !feed && loading}
      <p class="sub">{t("feed.loading")}</p>
    {:else if feed && !shown.length}
      <p class="sub">{t(enabledOnly && feed.entries.length ? "feed.noneEnabled" : "feed.none")}</p>
    {/if}

    {#if pinned.length}
      <ul class="board" aria-label={t("feed.latest")}>
        {#each pinned as e (e.item)}
          {@const text = excerpt(e)}
          <Scrap id={e.item} off={!enabled(e)}>
            <button class="name" onclick={() => onShowMod(e.mods[0])} title={t("feed.showTip")}>{e.title || e.item}</button>
            <p class="meta">
              <span>{fmtDate(e.updated, "long")}</span>{#if e.from}<span class="mono">{e.from} → {e.to}</span>{/if}{#if !enabled(e)}<span>{t("common.disabled")}</span>{/if}
            </p>
            {#if !e.loaded}
              <p class="excerpt muted">{t("feed.notLoaded")}</p>
            {:else if text}
              <p class="excerpt">{text}</p>
            {:else}
              <p class="excerpt muted">{t("feed.noNotes")}</p>
            {/if}
            <div class="actions">
              <button class="ink" onclick={() => readAll(e)}>{t("feed.readAll")}</button>
              {#if e.pending && pendingIds(e).length}
                <button class="ink strong" onclick={() => update(pendingIds(e))} disabled={busy}><Icon name="refresh" size={13} />{t("feed.update")}</button>
              {/if}
              {#if e.risks.length}
                <span class="stamp" title={e.risks.map((r) => t(`feed.risk.${r}`)).join("\n")}>{t("feed.mayBreak")}</span>
              {/if}
            </div>
          </Scrap>
        {/each}
      </ul>
    {/if}

    {#if days.length}
      <div class="inner">
        <h2 class="hist">{t("feed.history")}</h2>
        {#each days as [day, entries] (day)}
          <h3>{day}</h3>
          <ul class="entries">
            {#each entries as e (e.item)}
              {@const open = opened.has(e.item)}
              {@const more = expanded.has(e.item)}
              <li id="feed-{e.item}" class:off={!enabled(e)}>
                <div class="head">
                  <button class="quiet fold" aria-expanded={open} onclick={() => (opened = toggle(opened, e.item))}>
                    <Icon name={open ? "chevron-down" : "chevron-right"} size={14} /><span class="title">{e.title || e.item}</span>
                  </button>
                  {#if e.from}<span class="mono ver">{e.from} → {e.to}</span>{/if}
                  {#if e.risks.length}
                    <span class="risk" title={e.risks.map((r) => t(`feed.risk.${r}`)).join("\n")}><Icon name="alert" size={12} />{t("feed.mayBreak")}</span>
                  {/if}
                  {#if !enabled(e)}<span class="tag">{t("common.disabled")}</span>{/if}
                  <span class="spacer"></span>
                  {#if e.pending && pendingIds(e).length}
                    <button onclick={() => update(pendingIds(e))} disabled={busy}><Icon name="refresh" size={14} />{t("feed.update")}</button>
                  {/if}
                  <button class="quiet" onclick={() => onShowMod(e.mods[0])} title={t("feed.showTip")} aria-label={t("feed.showTip")}><Icon name="eye" size={14} /></button>
                  <button class="quiet" onclick={() => api.openUrl(`https://steamcommunity.com/sharedfiles/filedetails/changelog/${e.item}`)} title={t("feed.changelogTip")}>
                    <Icon name="steam" size={14} />{t("feed.changelog")}</button>
                </div>
                {#if !open}
                  <p class="peek">{excerpt(e).split("\n").find((l) => l.trim()) ?? ""}</p>
                {:else}
                  <p class="ids mono">{new Intl.ListFormat(intlLang(), { type: "unit" }).format(e.mods)}{#if e.risks.length} · <span class="why">{e.risks.map((r) => t(`feed.risk.${r}`)).join(" ")}</span>{/if}</p>
                  {#if !e.loaded}
                    <p class="hint">{t("feed.notLoaded")}</p>
                  {:else if e.notes.length}
                    <ol class="notes">
                      {#each more ? e.notes : e.notes.slice(0, SHOWN) as n}<li><strong>{fmtDate(n.time, "long")}</strong><p>{n.text || t("feed.emptyNote")}</p></li>{/each}
                    </ol>
                    {#if e.notes.length > SHOWN}
                      <button class="quiet small" onclick={() => (expanded = toggle(expanded, e.item))}>
                        {more ? t("feed.fewer") : t("feed.more", { count: e.notes.length - SHOWN })}</button>
                    {/if}
                  {:else if e.bundled}
                    {@const log = e.bundled}
                    <p class="hint">{t("feed.bundled")}</p>
                    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions: links inside open in the browser; they are focusable themselves -->
                    <ol class="notes" onclick={linkClick}>
                      {#each more ? log.entries : log.entries.slice(0, SHOWN) as b}
                        <li><strong>{b.title}</strong><div class="note rich">{@html log.bbcode ? bbcode(b.text) : markdown(b.text)}</div></li>
                      {/each}
                    </ol>
                    {#if log.entries.length > SHOWN}
                      <button class="quiet small" onclick={() => (expanded = toggle(expanded, e.item))}>
                        {more ? t("feed.fewer") : t("feed.more", { count: log.entries.length - SHOWN })}</button>
                    {/if}
                    {#if log.links.length}
                      <p class="links">{#each log.links as [title, url]}<button class="quiet small" onclick={() => api.openUrl(url)} title={url}><Icon name="external" size={12} />{title}</button>{/each}</p>
                    {/if}
                  {:else}
                    <p class="hint">{t("feed.noNotes")}</p>
                  {/if}
                {/if}
              </li>
            {/each}
          </ul>
        {/each}

        {#if feed?.unloaded}
          <button onclick={load} disabled={loading}><Icon name="download" size={14} />{t("feed.loadRest", { count: feed.unloaded })}</button>
        {/if}
        <p class="fine">{t("feed.fine")}</p>
      </div>
    {/if}
  </div>
</section>

<style>
  .feed {
    overflow-y: auto;
    min-height: 0;
  }
  .wrap {
    width: min(1180px, calc(100% - 32px));
    margin: 24px auto 40px;
  }
  .board-head {
    display: flex;
    gap: 16px;
    align-items: flex-end;
    justify-content: space-between;
    flex-wrap: wrap;
    padding: 0 4px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 22px;
  }
  .sub {
    color: var(--muted);
    margin: 4px 0;
  }
  .row,
  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .caution {
    margin: 14px 4px 0;
    padding: 8px 12px;
    border-left: 3px solid var(--danger);
    background: color-mix(in oklab, var(--danger) 10%, var(--panel));
    border-radius: 0 6px 6px 0;
  }

  /* the board: scraps of paper taped onto the blueprint */
  .board {
    list-style: none;
    margin: 26px 0 8px;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 34px 30px;
  }
  /* history: one line per mod until opened */
  .inner {
    margin-top: 30px;
    padding: 22px 26px 24px;
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  .hist {
    font-size: 18px;
  }
  h3 {
    margin: 20px 0 8px;
    font-size: 14px;
    color: var(--muted);
  }
  /* the date's first letter only ("samedi 11 octobre" isn't title case in French) */
  h3::first-letter {
    text-transform: uppercase;
  }
  .entries {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 6px;
  }
  .entries > li {
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--raised);
    scroll-margin-top: 12px;
  }
  .entries > li.off {
    opacity: 0.75;
  }
  .fold {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    padding-left: 2px;
  }
  .title {
    font-weight: 700;
    font-size: 14px;
  }
  .peek {
    margin: 0 0 2px 22px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .spacer {
    flex: 1;
  }
  .ver,
  .tag,
  .hint,
  .ids {
    color: var(--muted);
  }
  .hint,
  .ids {
    margin: 4px 0;
  }
  .risk,
  .why {
    color: var(--danger);
  }
  .risk {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    font-weight: 700;
  }
  .notes {
    list-style: none;
    padding: 0;
    margin: 8px 0 4px;
    display: grid;
    gap: 8px;
  }
  .notes p,
  .note {
    margin: 2px 0 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .note.rich {
    white-space: normal;
  }
  .rich :global(ul) {
    margin: 2px 0;
    padding-left: 18px;
  }
  .rich :global(h5) {
    margin: 4px 0 2px;
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0 0;
  }
  .fine {
    margin-top: 20px;
    color: var(--muted);
    font-size: 12px;
  }
</style>
