<script module lang="ts">
  // kept at module level: the Servers tab is drawn afresh when the profile changes (a server's
  // set-up switches profile), and the list, the filters and the open server stay as they were
  import type { GameServer as Server, LauncherView as Lists, ServerListing } from "./api";
  /** The list on show: internet, favourites, history, friends, or direct (the address form). */
  let view = $state("internet");
  let lists = $state<Lists | null>(null);
  /** Steam's list, filled in by A2S as answers come. */
  let all = $state<Server[]>([]);
  let listed = $state(false);
  let loading = $state(false);
  let listError = $state("");
  let sources = $state<ServerListing["sources"]>([]);
  /** How many of Browse's list (`internet`) and of the other list on show (`saved`) were asked. */
  let asked = $state({ internet: 0, saved: 0 });
  /** Favourites and history, as they answer now. */
  let saved = $state<Server[]>([]);
  let selected = $state<Server | null>(null);
  let linkPassword = $state<string | null>(null);
  /** A server hosted here: its config's join password, used without asking. */
  let ownPassword = $state<string | null>(null);
  let address = $state("");
  let addressError = $state("");
  let looking = $state(false);

  // filters
  let search = $state("");
  let build = $state("any");
  let modsFilter = $state("any");
  let maxDownload = $state(5);
  let map = $state("");
  let playersFilter = $state("any");
  let passwordFilter = $state("any");
  let pvpFilter = $state("any");
  let sort = $state("players");
  let countryFilter = $state("any");
  /** Country codes by IP, when countries are on. */
  let countries = $state<Record<string, string>>({});
  const SHOWN = 200;
  /** A newer probe run stops an older one of the same kind: Browse's goes on while another list is shown. */
  const runs = { internet: 0, saved: 0 };
  /** The last pzmm:// link shown (a redraw doesn't look it up again). */
  let shownLink: object | null = null;
  /** A pzmm:// link from outside waits in Direct connect: nothing is asked of its host until Look up. */
  let linkWaiting = $state(false);
</script>

<script lang="ts">
  import { track } from "./activity.svelte";
  import { onMount, untrack } from "svelte";
  import { api, fromSteam, parseServer, patient, WINDOWS, type GameServer, type SetupItem } from "./api";
  import { modNeed } from "./order";
  import { fmtNum } from "./format";
  import { intlLang, keyedError, lang, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import ServerJoin from "./ServerJoin.svelte";
  import { showInSettings } from "./Settings.svelte";

  let { show, hidden = false, busy, lock = "", installedIds, link = null, onSetup, onCheck, onSettings }: {
    /** The list the Servers rail picked (or "direct"). */
    show: string;
    /** The Servers tab is behind another one. */
    hidden?: boolean;
    busy: boolean;
    /** Why switching profile is blocked now ("" when it isn't). */
    lock?: string;
    installedIds: string[];
    /** A pzmm:// link to show, or a server hosted here with its password (a new object each time one arrives). */
    link?: { url: string; password?: string } | null;
    onSetup: (server: GameServer, items: string[], mods: string[], maps: string[], known?: SetupItem[], unlisted?: number) => Promise<boolean>;
    onCheck: () => Promise<boolean>;
    onSettings: () => void;
  } = $props();


  const installed = $derived(new Set(installedIds));
  /** A row's Join: the server it was for, and a count that opens its details afresh each time. */
  let joinFor = $state("");
  let joins = $state(0);
  /** For the filters: listed mods not installed, plus every mod the server doesn't list (they may be missing too). */
  const toGet = (s: GameServer) => {
    const n = modNeed(s, installed);
    return n.get + n.unlisted;
  };

  const ipOf = (s: GameServer) => s.addr.slice(0, s.addr.lastIndexOf(":"));
  const regions = $derived(new Intl.DisplayNames([lang()], { type: "region" }));
  function countryName(code: string) {
    try {
      return regions.of(code) ?? code;
    } catch {
      return code;
    }
  }
  const countryOf = (s: GameServer) => countries[ipOf(s)] ?? "";

  /** Countries for the servers in `list` not looked up yet (only when they're on). */
  async function lookupCountries(list: GameServer[]) {
    if (!lists?.countries) return;
    const ips = [...new Set(list.map(ipOf))].filter((ip) => !(ip in countries));
    if (!ips.length) return;
    try {
      const got = await api.serverCountries(ips);
      // asked once per list: an address without an answer stays blank
      countries = { ...countries, ...Object.fromEntries(ips.map((ip) => [ip, got[ip] ?? ""])) };
    } catch (e) {
      listError = t(...keyedError(e));
    }
  }

  async function setCountries(on: boolean) {
    if (!lists) return;
    try {
      await api.serverCountriesSet(on);
      lists = { ...lists, countries: on };
    } catch (e) {
      listError = t(...keyedError(e));
      return;
    }
    if (on) lookupCountries(source);
    else countryFilter = "any";
  }

  function keep(s: GameServer) {
    if (countryFilter !== "any" && countryOf(s) !== countryFilter) return false;
    if (search.trim() && !s.name.toLowerCase().includes(search.trim().toLowerCase())) return false;
    if (map.trim() && !s.map.toLowerCase().includes(map.trim().toLowerCase())) return false;
    if (build !== "any" && !s.build.startsWith(build)) return false;
    if (playersFilter === "notEmpty" && !s.players) return false;
    if (playersFilter === "notFull" && s.max_players && s.players >= s.max_players) return false;
    if (/^\d+$/.test(playersFilter) && s.players < +playersFilter) return false;
    if (pvpFilter !== "any" && s.pvp !== (pvpFilter === "pvp")) return false;
    // password and mods are known once it answered
    if ((passwordFilter !== "any" || modsFilter !== "any") && !s.probed) return false;
    if (passwordFilter !== "any" && s.password !== (passwordFilter === "locked")) return false;
    if (modsFilter === "mine" && toGet(s)) return false;
    if (modsFilter === "missing" && !toGet(s)) return false;
    if (modsFilter === "few" && toGet(s) > maxDownload) return false;
    return true;
  }
  const order = (a: GameServer, b: GameServer) =>
    sort === "ping" ? (a.ping ?? 1e9) - (b.ping ?? 1e9) : sort === "name" ? a.name.localeCompare(b.name) : b.players - a.players;
  const source = $derived(view === "internet" ? all : saved);
  const steamListed = $derived(fromSteam(sources));
  /** The sources that answered, with how many servers each gave; why the others didn't, as a tooltip. */
  const answered = $derived(new Intl.ListFormat(intlLang(), { type: "conjunction" }).format(sources.filter((a) => !a.error).map((a) => `${t(`launcher.sources.${a.source}`)} (${fmtNum(a.count)})`)));
  const failed = $derived(sources.map((a) => a.error).filter(Boolean).join("\n"));
  // history stays newest first
  const matching = $derived(view === "history" ? source.filter(keep) : source.filter(keep).sort(order));
  const shownCountries = $derived([...new Set(source.map(countryOf).filter(Boolean))].sort((a, b) => countryName(a).localeCompare(countryName(b))));

  async function refreshState() {
    try {
      // during an install: asked again once it's done, so the lists aren't left blank
      lists = await patient(() => api.launcherView());
    } catch (e) {
      listError = t(...keyedError(e));
    }
  }

  /** Ask servers in batches (the backend asks 32 at a time); a newer run stops an older one. */
  async function probe(kind: "internet" | "saved", list: GameServer[], fresh: boolean, into: (i: number, got: GameServer[]) => void) {
    const me = ++runs[kind];
    asked[kind] = 0;
    for (let i = 0; i < list.length; i += 64) {
      const got = await api.serversProbe(list.slice(i, i + 64), fresh).catch(() => null);
      if (me !== runs[kind]) return;
      if (got) into(i, got);
      asked[kind] = Math.min(list.length, i + 64);
    }
  }

  /** Steam's list (through the Steam client, or the key) merged with the game's own. */
  async function loadList(fresh = false) {
    if (!lists) return;
    loading = true;
    listError = "";
    try {
      const got = await patient(() => api.serversList(fresh));
      sources = got.sources;
      const list = got.servers.sort((a, b) => b.players - a.players);
      all = list;
      listed = true;
      loading = false;
      lookupCountries(list);
      await probe("internet", list, fresh, (i, got) => (all = [...all.slice(0, i), ...got, ...all.slice(i + got.length)]));
    } catch (e) {
      listError = t(...keyedError(e));
    } finally {
      loading = false;
    }
  }

  async function showSaved() {
    const list = (view === "favourites" ? lists?.favourites : lists?.history) ?? [];
    saved = list;
    lookupCountries(list);
    await probe("saved", list, false, (i, got) => (saved = [...saved.slice(0, i), ...got, ...saved.slice(i + got.length)]));
  }

  async function showFriends() {
    saved = [];
    loading = true;
    listError = "";
    try {
      saved = await api.serversFriends();
      lookupCountries(saved);
    } catch (e) {
      listError = t(...keyedError(e));
    } finally {
      loading = false;
    }
    await probe("saved", saved, false, (i, got) => (saved = [...saved.slice(0, i), ...got, ...saved.slice(i + got.length)]));
  }

  function pick(v: string) {
    view = v;
    if (v === "internet") {
      if (!listed) loadList();
    } else if (v === "friends") showFriends();
    else if (v !== "direct") showSaved();
  }

  async function lookup(text: string, own: string | null = null) {
    addressError = "";
    linkWaiting = false;
    const p = parseServer(text);
    if (!p) {
      addressError = t("launcher.badAddress");
      return;
    }
    looking = true;
    try {
      const [s] = await api.serversProbe([p.server], true);
      linkPassword = p.password;
      ownPassword = own;
      selected = s;
    } catch (e) {
      addressError = t(...keyedError(e));
    } finally {
      looking = false;
    }
  }

  $effect(() => {
    if (link && link !== shownLink) {
      shownLink = link;
      address = link.url;
      // Home's and the Servers tab's own servers are looked up at once; a link from a web page or a chat waits
      if (/^pzmm:/i.test(link.url)) (linkWaiting = true), (selected = null);
      else lookup(link.url, link.password ?? null);
    }
  });

  const isFavourite = (s: GameServer) => !!lists?.favourites.some((f) => f.addr === s.addr);
  async function toggleFavourite(s: GameServer) {
    try {
      const st = await api.serverFavourite(s, !isFavourite(s));
      if (lists) lists = { ...lists, ...st };
      if (view === "favourites") saved = saved.filter((x) => st.favourites.some((f) => f.addr === x.addr));
    } catch (e) {
      listError = t(...keyedError(e));
    }
  }

  /** Its mods as Home and its details count them, on one line: the ones it doesn't list never called here, said in the tooltip. */
  function modsCell(s: GameServer) {
    if (!s.probed) return { text: "", tip: "" };
    if (!s.mod_count) return { text: t("launcher.vanilla"), tip: "" };
    const n = modNeed(s, installed);
    const shown = n.total - n.unlisted;
    const part = n.get ? t("launcher.modsToGet", { count: n.get }) : !n.unlisted ? t("launcher.modsHave") : shown ? t("launcher.modsShownHave", { count: shown }) : "";
    const list = (xs: string[]) => new Intl.ListFormat(intlLang(), { type: "unit" }).format(xs);
    return { text: list([fmtNum(n.total), part].filter(Boolean)), tip: list([t("launcher.modsCount", { count: n.total }), ...(n.unlisted ? [t("launcher.notShown", { count: n.unlisted })] : [])]) };
  }

  let ready = $state(false);
  // the rail picks the list: loaded the first time it's shown, kept as it was across redraws
  $effect(() => {
    const v = show;
    if (ready) untrack(() => (v !== view || (v === "internet" && !listed)) && pick(v));
  });

  // shown again (Settings may have turned countries on or off): the setting read afresh, and the countries looked up
  let away = false;
  $effect(() => {
    if (hidden) away = true;
    else if (away) (away = false), untrack(() => refreshState().then(() => lookupCountries(source)));
  });

  onMount(async () => {
    const first = !lists;
    // seen before: drawn as it was at once, the lists asked again behind
    ready = !first;
    await refreshState();
    if (first) api.modIndexRefresh().catch(() => {});
    ready = true;
  });

  // the activity bar moves while this tab works
  $effect(() => {
    if (loading) return track();
  });
</script>

<div class="browser">
  <div class="lists">
  {#if show === "direct"}
    <h2>{t("launcher.title")}</h2>
    <p class="lede">{t("launcher.lede")}</p>
    <form class="connect" onsubmit={(e) => { e.preventDefault(); lookup(address); }}>
      <label for="srv-addr">{t("launcher.direct")}</label>
      <!-- i18n-ignore: an address example -->
      <input id="srv-addr" type="text" bind:value={address} spellcheck="false" placeholder="203.0.113.5:16261" />
      <button type="submit" disabled={looking || !address.trim()}><Icon name="search" size={14} />{t("launcher.lookUp")}</button>
    </form>
    {#if linkWaiting}<p class="hint" role="note">{t("launcher.linkWaiting")}</p>{/if}
    <p class="hint">{t("launcher.directHint")}</p>
    {#if addressError}<p class="error">{addressError}</p>{/if}
  {:else}
  {#if view === "internet" && listed && !steamListed}
    <div class="empty" role="note" title={failed}>
      <!-- outside Windows only the key gets Steam's list -->
      <p>{t("launcher.gameList")} {t(WINDOWS ? "launcher.startSteam" : "launcher.needKey")}</p>
      <button onclick={() => (showInSettings("keys"), onSettings())}><Icon name="settings" size={14} />{t("launcher.addKey")}</button>
    </div>
  {:else if view === "friends"}
    <p class="hint">{t("launcher.friendsHint")}</p>
  {/if}
  {#if lists}
    <div class="filters" role="group" aria-label={t("launcher.filtersLabel")}>
      <input id="servers-search" type="search" bind:value={search} placeholder={t("launcher.search")} aria-label={t("launcher.search")} />
      <select bind:value={build} aria-label={t("launcher.build")}>
        <option value="any">{t("launcher.anyBuild")}</option>
        <option value="42">{t("launcher.b42")}</option>
        <option value="41">{t("launcher.b41")}</option>
      </select>
      <select bind:value={modsFilter} aria-label={t("launcher.mods")}>
        <option value="any">{t("launcher.anyMods")}</option>
        <option value="mine">{t("launcher.onlyMine")}</option>
        <option value="missing">{t("launcher.missingSome")}</option>
        <option value="few">{t("launcher.atMost")}</option>
      </select>
      {#if modsFilter === "few"}
        <input type="number" min="0" max="999" bind:value={maxDownload} aria-label={t("launcher.atMostCount")} class="num" />
      {/if}
      <input type="search" bind:value={map} placeholder={t("launcher.map")} aria-label={t("launcher.map")} class="map" />
      <select bind:value={playersFilter} aria-label={t("launcher.players")}>
        <option value="any">{t("launcher.anyPlayers")}</option>
        <option value="notEmpty">{t("launcher.notEmpty")}</option>
        <option value="notFull">{t("launcher.notFull")}</option>
        {#each [5, 10, 20] as n}<option value={String(n)}>{fmtNum(n)}+</option>{/each}
      </select>
      <select bind:value={passwordFilter} aria-label={t("launcher.password")}>
        <option value="any">{t("launcher.anyPassword")}</option>
        <option value="open">{t("launcher.noPassword")}</option>
        <option value="locked">{t("launcher.withPassword")}</option>
      </select>
      <select bind:value={pvpFilter} aria-label={t("launcher.pvp")}>
        <option value="any">{t("launcher.anyPvp")}</option>
        <option value="pvp">{t("launcher.pvpOn")}</option>
        <option value="pve">{t("launcher.pvpOff")}</option>
      </select>
      {#if lists.countries}
        <select bind:value={countryFilter} aria-label={t("launcher.country")}>
          <option value="any">{t("launcher.anyCountry")}</option>
          {#each shownCountries as c}<option value={c}>{countryName(c)}</option>{/each}
        </select>
      {/if}
      <label class="check" title={t("launcher.countriesTip")}><input type="checkbox" checked={lists.countries} onchange={(e) => setCountries(e.currentTarget.checked)} />{t("launcher.countries")}</label>
      <select bind:value={sort} aria-label={t("launcher.sort")} hidden={view === "history"}>
        <option value="players">{t("launcher.byPlayers")}</option>
        <option value="ping">{t("launcher.byPing")}</option>
        <option value="name">{t("launcher.byName")}</option>
      </select>
      {#if view === "internet" || view === "friends"}
        <button class="refresh" onclick={() => (view === "friends" ? showFriends() : loadList(true))} disabled={loading}><Icon name="refresh" size={14} />{t("common.refresh")}</button>
      {/if}
    </div>
    {#if modsFilter !== "any"}<p class="hint">{t("launcher.modsFilterHint")}</p>{/if}
    {#if lists.countries}<p class="hint">{t("launcher.countriesHint")}</p>{/if}

    <p class="hint status" role="status">
      {#if loading}{t("launcher.listing")}
      {:else if source.length}
        {@const done = asked[view === "internet" ? "internet" : "saved"]}
        {t("launcher.shown", { count: matching.length, total: fmtNum(source.length) })}{#if view === "internet" && answered.length}{" · "}<span title={failed}>{t("launcher.from", { sources: answered })}</span>{/if}{#if done < source.length}{" · "}{t("launcher.asking", { done: fmtNum(done), total: fmtNum(source.length) })}{/if}
      {/if}
    </p>
    {#if listError}<p class="error">{listError}</p>{/if}

    {#if !loading && !source.length}
      <p class="hint empty">{t(view === "internet" && !steamListed ? (WINDOWS ? "launcher.empty.game" : "launcher.empty.gameKey") : `launcher.empty.${view}`)}</p>
    {:else if matching.length}
      <div class="table">
        <table>
          <thead>
            <tr>
              <th>{t("launcher.col.name")}</th>
              {#if lists.countries}<th>{t("launcher.col.country")}</th>{/if}
              <th class="mapcol">{t("launcher.col.map")}</th>
              <th class="n">{t("launcher.col.players")}</th>
              <th class="n">{t("launcher.col.ping")}</th>
              <th>{t("launcher.col.build")}</th>
              <th>{t("launcher.col.mods")}</th>
              <th><span class="visually-hidden">{t("launcher.favourite")}</span></th>
            </tr>
          </thead>
          <tbody>
            {#each matching.slice(0, SHOWN) as s (s.addr)}
              {@const mc = modsCell(s)}
              <tr class:selected={selected?.addr === s.addr} class:silent={!s.probed && !!s.error}>
                <td class="name">
                  <button class="pick" onclick={() => { linkPassword = ownPassword = null; joinFor = ""; selected = s; }} aria-pressed={selected?.addr === s.addr}>
                    {#if s.password}<span title={t("launcher.locked")}><Icon name="lock" size={12} /></span>{/if}
                    <span>{s.name}</span>
                    {#if s.pvp}<span class="tag">{t("launcher.pvpTag")}</span>{/if}
                    {#if s.friends?.length}<span class="hint">{t("launcher.friendsOn", { names: new Intl.ListFormat(intlLang(), { type: "conjunction" }).format(s.friends) })}</span>{/if}
                  </button>
                </td>
                {#if lists.countries}<td class="mono" title={countryOf(s) ? countryName(countryOf(s)) : ""}>{countryOf(s)}</td>{/if}
                <td class="mapcol">{s.map}</td>
                <td class="n">{s.players}/{s.max_players}</td>
                <td class="n">{s.ping === null ? (s.error ? "—" : "…") : t("launcher.ms", { n: s.ping })}</td>
                <td class="mono">{s.build}</td>
                <td class="modscol" title={mc.tip}>{mc.text}</td>
                <td class="acts">
                  {#if s.probed && !s.error}<button class="primary" onclick={() => { linkPassword = ownPassword = null; joinFor = s.addr; joins++; selected = s; }} disabled={busy}>{t("launcher.join")}</button>{/if}
                  <button class="star" class:on={isFavourite(s)} onclick={() => toggleFavourite(s)} aria-pressed={isFavourite(s)} title={t("launcher.favourite")} aria-label={t("launcher.favourite")}>
                    <Icon name="star" size={14} fill={isFavourite(s)} />
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if matching.length > SHOWN}<p class="hint">{t("launcher.more", { count: SHOWN })}</p>{/if}
    {:else if source.length}
      <p class="hint empty">{t("launcher.noMatch")}</p>
    {/if}
  {/if}
  {/if}
  </div>

  {#if selected}
    {#key `${selected.addr} ${joins}`}
      <ServerJoin server={selected} autoJoin={joinFor === selected.addr} {busy} {lock} {installed} password={linkPassword} {ownPassword} joins={lists?.joins[selected.addr] ?? []} favourite={isFavourite(selected)}
        onFavourite={() => selected && toggleFavourite(selected)} onClose={() => (selected = null)}
        onUpdate={(s) => (selected = s)} {onSetup} {onCheck} onJoined={refreshState} />
    {/key}
  {/if}
</div>

<style>
  /* the list fills the Servers workspace; a server's details dock on its right (over it in narrow windows) */
  .browser {
    position: relative;
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .lists {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 12px 16px 0;
  }
  .lists > * {
    flex: none;
  }
  h2 {
    margin: 12px 0 6px;
    font-size: 22px;
  }
  .connect,
  .filters {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .connect {
    margin: 4px 0 12px;
  }
  .connect label {
    color: var(--muted);
  }
  .connect input {
    width: 260px;
  }
  .refresh {
    margin-left: auto;
  }
  .filters input[type="search"] {
    width: 180px;
  }
  .filters .map {
    width: 120px;
  }
  .filters .num {
    width: 70px;
  }
  .filters .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .status {
    margin: 8px 0 4px;
    min-height: 1.45em;
  }
  .empty {
    margin: 12px 0;
  }
  .empty p {
    margin: 0 0 8px;
  }
  /* reaches 6px past the list's left edge: the picked row's chevron on that edge isn't cut off */
  .lists > .table {
    container-type: inline-size;
    flex: 1 1 auto;
    min-height: 160px;
    overflow: auto;
    margin-left: -6px;
    padding-left: 6px;
    border-top: 1px solid var(--line);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--panel);
    text-align: left;
    font-weight: 400;
    color: var(--muted);
    padding: 6px 8px;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
    z-index: 2;
  }
  td {
    padding: 2px 8px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
    vertical-align: middle;
  }
  .n {
    text-align: right;
    white-space: nowrap;
  }
  .modscol,
  .acts {
    white-space: nowrap;
  }
  /* the design system's hover: the red fill, its text colour and a 1px edge; the picked row's chevron on the table's edge */
  tr:hover td,
  tr.selected td {
    color: var(--on-selected);
    background: var(--hover);
    box-shadow: inset 0 1px 0 var(--hover-line), inset 0 -1px 0 var(--hover-line);
  }
  tr:hover td:first-child,
  tr.selected td:first-child {
    position: relative;
    box-shadow: inset 0 1px 0 var(--hover-line), inset 0 -1px 0 var(--hover-line), inset 1px 0 0 var(--hover-line);
  }
  tr:hover td:last-child,
  tr.selected td:last-child {
    box-shadow: inset 0 1px 0 var(--hover-line), inset 0 -1px 0 var(--hover-line), inset -1px 0 0 var(--hover-line);
  }
  tr:hover :is(.hint, .tag, .star),
  tr.selected :is(.hint, .tag, .star:not(.on)) {
    color: inherit;
  }
  tr:hover td:first-child::before,
  tr.selected td:first-child::before {
    content: "";
    z-index: 1;
    position: absolute;
    left: -5px;
    top: 50%;
    width: 10px;
    height: 14px;
    margin-top: -7px;
    background: var(--on-selected);
    clip-path: polygon(0 0, 45% 0, 100% 50%, 45% 100%, 0 100%, 55% 50%);
  }
  tr.silent td {
    color: var(--muted);
  }
  .name {
    max-width: 320px;
  }
  .pick {
    justify-content: flex-start;
    padding: 4px 2px;
    white-space: normal;
    text-align: left;
    gap: 6px;
  }
  .mapcol {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* squeezed (beside a server's details): the map gives way, the server's page says it */
  @container (max-width: 640px) {
    .mapcol {
      display: none;
    }
  }
  .tag {
    font-size: 11px;
    padding: 0 5px;
    border-radius: 4px;
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .star {
    padding: 4px;
    color: var(--muted);
  }
  .star.on {
    color: var(--accent-text);
  }
</style>
