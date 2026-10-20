<script module lang="ts">
  /** What the last join or restore said, by server: it outlives the redraw a profile switch causes. */
  const said = $state<Record<string, { text: string; error: boolean }>>({});
</script>

<script lang="ts">
  import { track } from "./activity.svelte";
  import { onMount, untrack } from "svelte";
  import { api, connectOf, defaultSkip, joinPlan, knownItems, type GameServer, type MapCompare, type ServerJoin, type ServerMods, type ServerPlayer, type SetupItem } from "./api";
  import { fmtAgo, fmtDate, fmtDateTime, fmtNum, fmtSize } from "./format";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import { modNeed } from "./order";
  import { arrowPick, trap } from "./ui";
  import Icon from "./Icon.svelte";

  let { server, autoJoin = false, busy, lock, installed, password, ownPassword = null, joins, favourite, onFavourite, onClose, onUpdate, onSetup, onCheck, onJoined }: {
    server: GameServer;
    busy: boolean;
    lock: string;
    installed: Set<string>;
    /** From a pzmm:// link: null when it said nothing about a password. */
    password: string | null;
    /** A server hosted here: its config's join password, used without asking. */
    ownPassword?: string | null;
    joins: ServerJoin[];
    favourite: boolean;
    onFavourite: () => void;
    onClose: () => void;
    onUpdate: (s: GameServer) => void;
    onSetup: (server: GameServer, items: string[], mods: string[], maps: string[], known?: SetupItem[], unlisted?: number) => Promise<boolean>;
    /** The check Launch runs, on the profile you join with. */
    onCheck: () => Promise<boolean>;
    onJoined: () => void;
    /** Opened by a list row's Join: join at once (set up first when it runs mods). */
    autoJoin?: boolean;
  } = $props();

  const TABS = $derived(["mods", "rules", "players", ...(joins.length ? ["history"] : [])]);
  let tab = $state("mods");
  let mods = $state<ServerMods | null>(null);
  /** Its maps against the map mods PZMM knows: whose mod isn't in its list, and which overlap. */
  let maps = $state<MapCompare | null>(null);
  let resolving = $state(false);
  let players = $state<ServerPlayer[] | null>(null);
  let playersError = $state("");
  let passwordSaved = $state(false);
  let error = $state("");
  const outcome = $derived(said[server.addr]);
  const tell = (text: string, error = false) => (said[server.addr] = { text, error });
  let working = $state(false);
  let asking = $state<{ value: string; remember: boolean; done: (ok: boolean) => void } | null>(null);
  let now = $state(Date.now() / 1000);
  /** Mods ticked off: not downloaded, not put in the profile. */
  let skip = $state<Record<string, boolean>>({});
  const skipped = $derived(Object.values(skip).filter(Boolean).length);

  const connect = $derived(connectOf(server));
  /** Mods it runs but doesn't list (counted as Home and the server list count them). */
  const unlisted = $derived(modNeed(server, installed, mods).unlisted);
  /** Workshop items to download and the profile's mod order (see joinPlan); the setup popup says which are best guesses. */
  const plan = $derived(joinPlan(server, mods, skip, true));
  const downloads = $derived(plan.downloads);
  const size = $derived(downloads.reduce((n, m) => n + m.size, 0));
  const missing = $derived((mods?.mods ?? []).filter((m) => m.status === "unknown"));
  /** A PZMM list's items updated on the Workshop since the list was made: the server still runs the old version. */
  const moved = $derived((mods?.items ?? []).filter((i) => i.pinned && i.updated > i.pinned));
  const major = $derived(server.build.startsWith("41") ? 41 : server.build.startsWith("42") ? 42 : null);
  const mismatch = $derived(!!mods && major !== null && major !== mods.build);
  /** The newest Workshop update among the server's resolved mods. */
  const newest = $derived((mods?.mods ?? []).filter((m) => m.updated).sort((a, b) => b.updated - a.updated)[0] ?? null);
  /** The server has been up at least this long: someone has been on that long. */
  /** Maps whose map mod (PZMM knows it) isn't among the mods it lists. */
  const mapNeeds = $derived((maps?.only_server ?? []).filter((s) => s.id && !mods?.mods.some((m) => m.id === s.id || (!!s.item && m.item === s.item))));
  const list = (xs: string[]) => new Intl.ListFormat(intlLang(), { type: "conjunction" }).format(xs);
  const upAtLeast = $derived(players?.length ? Math.max(...players.map((p) => p.duration)) : 0);

  /** The installed mods the statuses were worked out with. */
  let resolvedWith: Set<string> | null = null;
  async function resolve() {
    resolvedWith = installed;
    resolving = true;
    error = "";
    try {
      mods = await api.serverMods(server);
      const m = mods;
      api.mapServerCompare("", [], { mods: m.mods.map((x) => x.id), workshop: m.mods.map((x) => x.item).filter(Boolean), maps: m.maps }, m.build).then((c) => (maps = c)).catch(() => {});
      // the server says players may go without these: off until ticked
      for (const id of Object.keys(defaultSkip(mods))) skip[id] ??= true;
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      resolving = false;
    }
  }

  // installs since (here or anywhere else) leave rows saying "to download": work them out again
  $effect(() => {
    const now = installed;
    if (mods && !resolving && now !== resolvedWith && mods.mods.some((m) => m.status !== "installed" && now.has(m.id))) untrack(resolve);
  });

  async function refresh() {
    error = "";
    try {
      const [s] = await api.serversProbe([server], true);
      onUpdate(s);
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function showPlayers() {
    playersError = "";
    try {
      players = await api.serverPlayers(server.addr);
      now = Date.now() / 1000;
    } catch (e) {
      playersError = t(...keyedError(e));
    }
  }

  function askPassword(): Promise<{ value: string; remember: boolean } | null> {
    return new Promise((done) => {
      // read back through `asking`: the state proxy holds what was typed, not this object
      asking = { value: password ?? "", remember: false, done: (ok: boolean) => {
        const typed = asking;
        asking = null;
        done(ok && typed ? { value: typed.value, remember: typed.remember } : null);
      } };
    });
  }

  /** Set up (download, make and use the server's profile) unless `asIs`, then start the game joining it. */
  async function join(asIs: boolean) {
    error = "";
    delete said[server.addr];
    let pw = { value: "", remember: false };
    if (ownPassword) pw.value = ownPassword;
    else if (server.password && !passwordSaved) {
      const a = await askPassword();
      if (!a) return;
      pw = a;
    }
    working = true;
    try {
      if (!asIs && !(await onSetup(server, downloads.map((m) => m.item), plan.order, mods?.maps ?? [], mods ? knownItems(mods) : [], unlisted))) return;
      if (!(await onCheck())) return;
      await api.serverJoin(server, pw.value, pw.remember);
      tell(t("launcher.started"));
      if (pw.remember) passwordSaved = true;
      onJoined();
    } catch (e) {
      tell(t(...keyedError(e)), true);
    } finally {
      working = false;
    }
  }

  async function restore(j: ServerJoin) {
    error = "";
    delete said[server.addr];
    working = true;
    try {
      // the items installed already aren't downloaded again
      const have = new Set([...(mods?.items.filter((i) => i.installed).map((i) => i.item) ?? []), ...(mods?.mods.filter((m) => m.status === "installed").map((m) => m.item) ?? [])]);
      if (await onSetup(server, Object.keys(j.items).filter((i) => !have.has(i)), j.mods, j.maps)) tell(t("launcher.restored", { date: fmtDateTime(j.time) }));
    } finally {
      working = false;
    }
  }

  async function forgetPassword() {
    await api.serverPasswordForget(connect).catch((e) => (error = t(...keyedError(e))));
    passwordSaved = false;
  }

  function duration(s: number) {
    const m = Math.floor(s / 60);
    return m < 60 ? t("launcher.minutes", { count: m }) : t("launcher.hours", { h: Math.floor(m / 60), m: m % 60 });
  }
  const gap = (s: number) => (s < 86400 ? duration(s) : t("launcher.days", { count: Math.floor(s / 86400) }));

  const status = (s: string) => t(`launcher.status.${s}`);
  const RULES = ["open", "public", "pvp", "version", "modCount", "lastWipe"];

  onMount(() => {
    const ready = server.probed ? resolve() : Promise.resolve();
    if (autoJoin)
      ready.then(() => {
        if (!server.mod_count) join(true);
        else if (mods && !lock) join(false);
      });
    if (server.password) api.serverPasswordSaved(connect).then((v) => (passwordSaved = v)).catch(() => {});
  });

  // the activity bar moves while this tab works
  $effect(() => {
    if (working) return track();
  });
</script>

<section class="details" aria-label={server.name}>
  <div class="top">
    <div class="title">
      <h3>{server.name}</h3>
      <span class="hint">
        <span class="mono">{connect}</span>
        {#if server.probed}
          · {t("launcher.facts", { players: fmtNum(server.players), max: fmtNum(server.max_players), ping: fmtNum(server.ping ?? 0) })}
          {#if server.build}· <span class="mono">{server.build}</span>{/if}
          {#if server.pvp}· {t("launcher.pvpTag")}{/if}
          {#if server.password}· {t("launcher.locked")}{/if}
        {/if}
      </span>
      {#if server.map}<span class="hint">{t("launcher.mapLine", { map: server.map })}</span>{/if}
    </div>
    <span class="btns">
      <button onclick={onFavourite} aria-pressed={favourite}><Icon name="star" size={14} fill={favourite} />{t(favourite ? "launcher.unfavourite" : "launcher.favourite")}</button>
      <button onclick={refresh}><Icon name="refresh" size={14} />{t("common.refresh")}</button>
      <!-- BattleMetrics finds a server by its address -->
      <button onclick={() => api.openUrl(`https://www.battlemetrics.com/servers/zomboid?q=${encodeURIComponent(connect.replace(/:\d+$/, ""))}`)} title={t("launcher.battlemetricsTip")}><Icon name="external" size={14} />{t("launcher.battlemetrics")}</button>
      <button class="quiet" onclick={onClose} aria-label={t("common.close")} title={t("common.close")}><Icon name="x" size={14} /></button>
    </span>
  </div>

  {#if !server.probed}
    <p class="warn" role="status"><Icon name="alert" size={14} />{t("launcher.noAnswer", { error: server.error || "—" })}</p>
  {/if}
  {#if mismatch}
    <p class="warn" role="alert"><Icon name="alert" size={14} />{t("launcher.buildMismatch", { server: server.build, mine: mods?.build ?? "" })}</p>
  {/if}

  {#if server.description}<p class="desc">{server.description}</p>{/if}

  <div class="seg" role="tablist" aria-label={t("launcher.detailsLabel")} tabindex="-1" onkeydown={(e) => arrowPick(e, TABS, tab, (v) => (tab = v))}>
    {#each TABS as id}
      <button role="tab" aria-selected={tab === id} tabindex={tab === id ? 0 : -1} class:active={tab === id} onclick={() => { tab = id; if (id === "players" && !players) showPlayers(); }}>{t(`launcher.tabs.${id}`)}</button>
    {/each}
  </div>

  <div class="pane">
    {#if tab === "mods"}
      {#if !server.probed}
        <p class="hint">{t("launcher.modsUnknown")}</p>
      {:else if !server.mod_count}
        <p class="hint">{t("launcher.noMods")}</p>
      {:else}
        {#if resolving}<p class="hint" role="status">{t("launcher.resolving")}</p>{/if}
        {#if mods?.list}
          <p class="exact" role="note"><Icon name="check" size={14} />{t(mods.list === "full" ? "launcher.exactList" : "launcher.exactItems", { count: mods.items.length })}</p>
        {:else if server.tag_error}
          <p class="hint" role="note">{t(`launcher.tagError.${server.tag_error}`)}</p>
        {/if}
        <ul class="mods">
          {#each mods?.mods ?? server.mods.map((id) => ({ id, status: "", item: "", title: "", size: 0, updated: 0 })) as m}
            <li>
              {#if mods}<input type="checkbox" checked={!skip[m.id]} onchange={(e) => (skip[m.id] = !e.currentTarget.checked)} aria-label={t("launcher.useMod", { id: m.id })} title={t("launcher.useMod", { id: m.id })} />{/if}
              <span class="mono id">{m.id}</span>
              {#if mods?.server_only.includes(m.id)}<span class="side" title={t("launcher.serverOnlyTip")}>{t("launcher.serverOnly")}</span>
              {:else if mods?.optional.includes(m.id)}<span class="side" title={t("launcher.optionalTip")}>{t("launcher.optional")}</span>{/if}
              {#if m.status}
                <span class="st {m.status}">{status(m.status)}</span>
                <span class="hint">{new Intl.ListFormat(intlLang(), { type: "unit" }).format([m.title, m.status !== "installed" && m.size ? fmtSize(m.size) : "", m.updated ? t("launcher.updated", { ago: fmtAgo(m.updated) }) : ""].filter(Boolean))}</span>
              {/if}
            </li>
          {/each}
        </ul>
        {#if unlisted}<p class="hint">{t("launcher.andMore", { count: unlisted, total: fmtNum(server.mod_count) })}</p>{/if}
        {#if skipped}<p class="warn" role="note"><Icon name="alert" size={14} />{t("launcher.skipped", { count: skipped })}</p>{/if}
        {#if mapNeeds.length}
          {@const ids = [...new Set(mapNeeds.map((s) => s.id ?? ""))]}
          <p class="warn" role="note"><Icon name="alert" size={14} />{t("launcher.mapsUnlisted", { count: ids.length, mods: list(ids), maps: list(mapNeeds.map((s) => s.folder)) })}</p>
        {/if}
        {#each maps?.overlap ?? [] as [a, b]}<p class="hint" role="note">{t("launcher.mapOverlap", { a, b })}</p>{/each}
      {/if}
    {:else if tab === "rules"}
      <dl class="rules">
        {#each RULES.filter((k) => k in server.rules) as k}
          <dt>{t(`launcher.rule.${k}`)}</dt>
          {@const v = server.rules[k]}
          <dd>{k === "lastWipe" ? fmtDate(Number(v), "long") : ["open", "public", "pvp"].includes(k) && (v === "1" || v === "0") ? t(v === "1" ? "launcher.yes" : "launcher.no") : v}</dd>
        {/each}
        {#if server.keywords}<dt>{t("launcher.keywords")}</dt><dd class="mono">{server.keywords}</dd>{/if}
      </dl>
      {#if server.rules.lastWipe}<p class="hint">{t("launcher.wipeHint")}</p>{/if}
    {:else if tab === "players"}
      {#if playersError}<p class="error">{playersError}</p>
      {:else if !players}<p class="hint">{t("launcher.asking1")}</p>
      {:else if !players.length}<p class="hint">{t("launcher.nobody")}</p>
      {:else}
        <ul class="players">
          {#each players as p}<li><span>{p.name}</span><span class="hint">{duration(p.duration)}</span></li>{/each}
        </ul>
      {/if}
    {:else}
      <p class="hint">{t("launcher.historyHint")}</p>
      <ul class="joins">
        {#each joins as j (j.time)}
          <li>
            <span>{fmtDateTime(j.time)} <span class="hint">· {t("launcher.joinMods", { count: j.mods.length, items: fmtNum(Object.keys(j.items).length) })}</span></span>
            <button disabled={busy || working || !!lock} title={lock} onclick={() => restore(j)}><Icon name="rollback" size={14} />{t("launcher.useSetup", { date: fmtDate(j.time, "short") })}</button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#if moved.length}
    <ul class="moved" role="alert">
      {#each moved as i (i.item)}
        <li class="warn"><Icon name="alert" size={14} />{t("launcher.movedOn", { mod: i.title || i.item, gap: gap(i.updated - i.pinned) })}</li>
      {/each}
    </ul>
  {:else if newest && !mods?.list}
    <p class={upAtLeast && newest.updated > now - upAtLeast ? "warn" : "hint"} role="note">
      {#if upAtLeast && newest.updated > now - upAtLeast}
        <Icon name="alert" size={14} />{t("launcher.preflightSure", { up: duration(upAtLeast), mod: newest.title || newest.id, ago: fmtAgo(newest.updated) })}
      {:else}
        {t("launcher.preflight", { time: fmtDateTime(newest.updated), mod: newest.title || newest.id })}
      {/if}
    </p>
  {/if}

  <div class="plan">
    {#if server.mod_count && mods}
      <p class="hint">
        {downloads.length ? t("launcher.planDownload", { count: downloads.length, size: fmtSize(size) }) : t("launcher.planNothing")}
        · {t("launcher.planProfile")}
        {#if missing.length}· {t("launcher.planMissing", { count: missing.length })}{/if}
      </p>
    {/if}
    {#if server.password && !passwordSaved && password === null && !ownPassword}<p class="hint" role="note">{t("launcher.passwordFirst")}</p>{/if}
    <div class="btns">
      {#if server.mod_count}
        <button class="primary" disabled={busy || working || resolving || !!lock || !mods} title={lock} onclick={() => join(false)}><Icon name="download" size={14} />{t("launcher.setUpJoin")}</button>
      {/if}
      <button class={server.mod_count ? "" : "primary"} disabled={busy || working} onclick={() => join(true)}><Icon name="play" size={14} />{t(server.mod_count ? "launcher.joinAsIs" : "launcher.join")}</button>
      {#if passwordSaved}<button class="quiet" onclick={forgetPassword}>{t("launcher.forgetPassword")}</button>{/if}
    </div>
    {#if outcome}<p class={outcome.error ? "error" : "hint"} role="status">{outcome.text}</p>{/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>

  {#if asking}
    <div class="dim" onclick={() => asking?.done(false)} aria-hidden="true"></div>
    <div class="strip ask" role="dialog" aria-modal="true" tabindex="-1" aria-label={t("launcher.passwordTitle", { name: server.name })} use:trap onkeydown={(e) => e.key === "Escape" && asking?.done(false)}>
    <form onsubmit={(e) => { e.preventDefault(); asking?.done(true); }}>
      <p><strong>{t("launcher.passwordTitle", { name: server.name })}</strong></p>
      <!-- a link's password in clear, to read before it's sent -->
      <input type={password ? "text" : "password"} bind:value={asking.value} autocomplete="off" spellcheck="false" aria-label={t("launcher.password")} />
      {#if password}<p class="hint">{t("launcher.passwordFromLink")}</p>{/if}
      <label class="check"><input type="checkbox" bind:checked={asking.remember} />{t("launcher.remember")}</label>
      <div class="row">
        <button class="primary" type="submit" disabled={!asking.value}><Icon name="play" size={14} />{t("launcher.join")}</button>
        <button class="quiet" type="button" onclick={() => asking?.done(false)}><Icon name="x" size={14} />{t("common.cancel")}</button>
      </div>
    </form>
    </div>
  {/if}
</section>

<style>
  /* docked on the server list's right, like the Workshop's item panel; over the list in narrow windows */
  .details {
    flex: none;
    width: 460px;
    overflow-y: auto;
    padding: 14px 16px;
    background: var(--panel);
    border-left: 1px solid var(--line);
  }
  @media (max-width: 1200px) {
    .details {
      position: absolute;
      z-index: 2;
      top: 0;
      right: 0;
      bottom: 0;
      width: min(460px, 100%);
      box-shadow: var(--shadow);
    }
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
  }
  .title {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .title .hint {
    display: block;
  }
  h3 {
    margin: 0 0 2px;
    font-size: 17px;
  }
  .btns {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .warn {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    color: var(--danger);
    margin: 8px 0;
  }
  .desc {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 180px;
    overflow: auto;
    margin: 10px 0;
    padding: 8px 10px;
    border-left: 3px solid var(--line);
  }
  .exact {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 0 0 6px;
    color: var(--ok);
  }
  .moved {
    list-style: none;
    padding: 0;
    margin: 8px 0;
  }
  .moved .warn {
    margin: 4px 0;
  }
  .seg {
    display: inline-flex;
    margin: 6px 0 8px;
  }
  .pane {
    min-height: 60px;
  }
  .mods,
  .players,
  .joins {
    list-style: none;
    padding: 0;
    margin: 0;
    max-height: 260px;
    overflow: auto;
  }
  .mods li,
  .players li,
  .joins li {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 3px 0;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
    overflow-wrap: anywhere;
  }
  .joins li {
    justify-content: space-between;
    align-items: center;
  }
  .id {
    min-width: 0;
  }
  .side {
    flex: none;
    font-size: 12px;
    color: var(--muted);
  }
  .st {
    flex: none;
    font-size: 12px;
    padding: 0 6px;
    border-radius: 4px;
    border: 1px solid var(--line);
  }
  .st.installed {
    color: var(--ok);
  }
  .st.found {
    color: var(--text);
  }
  .st.likely {
    color: var(--warn);
  }
  .st.unknown {
    color: var(--danger);
  }
  .rules {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 4px 14px;
    margin: 0;
  }
  .rules dt {
    color: var(--muted);
  }
  .rules dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0;
  }
  .plan {
    margin-top: 12px;
  }
  .plan p {
    margin: 0 0 8px;
  }
  /* a box in the middle over the dimmed window, like every question */
  .dim {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: rgb(0 0 0 / 0.45);
  }
  .ask {
    position: fixed;
    z-index: 31;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    overflow: auto;
    margin: 0;
    padding: 20px 22px;
    border-radius: 10px;
    background: var(--panel);
    box-shadow: var(--shadow);
  }
  .ask p {
    margin: 0 0 8px;
  }
  .ask input:not([type="checkbox"]) {
    width: min(100%, 320px);
  }
  .row {
    display: flex;
    gap: 8px;
  }
</style>
