<script module lang="ts">
  import type { HostView, LauncherView } from "./api";
  import { ask as askNow } from "./ask.svelte";
  import { t } from "./i18n.svelte";
  // kept at module level: the tab is drawn afresh when the profile changes (joining a server switches
  // profile), and what was open stays open
  /** What the main area shows: overview, a server list (internet, favourites, history, friends), direct, new, or server. */
  let sel = $state("overview");
  /** The server open when `sel` is "server". */
  let selected = $state<string | null>(null);
  let tab = $state("console");
  /** What the server browser looks up: a pzmm:// link, or a server hosted here to join (with its config's password). */
  let browserLink = $state<{ url: string; password?: string } | null>(null);
  /** The last pzmm:// link taken (a redraw doesn't take it again). */
  let seenLink: object | null = null;
  /** The servers hosted here, and favourites/history (Friends needs the key): drawn as last seen when the tab opens again, then asked afresh. */
  let host = $state<HostView | null>(null);
  let launcher = $state<LauncherView | null>(null);
  /** The open server's section has changes not saved yet (Mods, Settings, Automation); here so Home's links ask too. */
  let dirty = $state(false);
  /** Before the open section goes: changes not saved are dropped only when the player says so. */
  async function leave() {
    if (sel === "server" && dirty && (await askNow(t("hosting.leaveTitle"), [t("hosting.leaveLine")], [{ label: t("hosting.leave"), icon: "undo" }])) !== 0) return false;
    dirty = false;
    return true;
  }
  /** Open on a list ("internet", "favourites"...), or on a server hosted here by name (Home's links). */
  export async function showInServers(list: string, server = "") {
    if (!(sel === "server" && selected === server) && !(await leave())) return;
    sel = server ? "server" : list;
    if (server) selected = server;
  }
  /** The new server wizard is open (the main page doesn't redraw the tab under it on a profile switch). */
  export const inServerWizard = () => sel === "new";
  /** Join a server hosted here (Home's Join): Direct connect looks up its address, its password used as is. */
  export async function joinInServers(own: { url: string; password: string }) {
    if (!(await leave())) return;
    browserLink = own;
    sel = "direct";
  }
</script>

<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, connectOf, ownJoin, type Finding, type FoundServer, type GameServer, type ItemState, type RemoteServer, type Row, type ServerCard, type ServerEvent, type SetupItem } from "./api";
  import { fmtDateTime, fmtNum, fmtSize } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { intlLang, keyedError } from "./i18n.svelte";
  import { arrowPick, trap } from "./ui";
  import Icon from "./Icon.svelte";
  import ServerBrowser from "./ServerBrowser.svelte";
  import { q } from "./ServerCommands.svelte";
  import ServerConfigs from "./ServerConfigs.svelte";
  import ServerAuto from "./ServerAuto.svelte";
  import ServerBackups from "./ServerBackups.svelte";
  import ServerConsole from "./ServerConsole.svelte";
  import ServerFiles from "./ServerFiles.svelte";
  import ServerMods from "./ServerMods.svelte";
  import ServerPlayers from "./ServerPlayers.svelte";
  import ServerSettings from "./ServerSettings.svelte";
  import ServerWizard from "./ServerWizard.svelte";
  import ServerWorld from "./ServerWorld.svelte";

  let { busy, lock = "", hidden = false, profiles, profile, rows, installedIds, link = null, onSetup, onCheck, onSettings, onImport, ask }: {
    busy: boolean;
    /** Kept mounted behind another tab: no polling meanwhile (status events still come). */
    hidden?: boolean;
    /** Why switching profile is blocked now ("" when it isn't): joining and Use as profile wait. */
    lock?: string;
    /** The library (installed mods and their load rules), for a server's mod list. */
    rows: Row[];
    profiles: string[];
    profile: string;
    /** Installed mod ids (the server browser compares servers' mods with them). */
    installedIds: string[];
    /** A pzmm:// link to show (a new object each time one arrives). */
    link?: { url: string } | null;
    /** Download a server's mods and switch to its own profile. */
    onSetup: (server: GameServer, items: string[], mods: string[], maps: string[], known?: SetupItem[]) => Promise<boolean>;
    /** The check before the game starts (Launch's): false when the player stopped there. */
    onCheck: () => Promise<boolean>;
    onSettings: () => void;
    onImport: (name: string) => Promise<boolean>;
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
  } = $props();

  /** The rail's server lists, then Direct connect. */
  const LISTS = ["internet", "favourites", "history", "friends", "direct"];
  const LIST_ICONS: Record<string, IconName> = { internet: "search", favourites: "star", history: "rollback", friends: "share", direct: "play" };
  const listLabel = (v: string) => t(v === "internet" ? "servers.rail.browse" : v === "direct" ? "launcher.direct" : `launcher.views.${v}`);

  const lists = $derived(launcher?.key_saved ? LISTS : LISTS.filter((v) => v !== "friends"));
  /** Narrow windows: the rail is a drawer the bar above the main area opens. */
  let railOpen = $state(false);
  let rail = $state<HTMLElement>();
  let railToggle = $state<HTMLButtonElement>();
  /** Open the drawer on its first item, or close it with focus back on the bar's button. */
  async function toggleRail(open = !railOpen) {
    railOpen = open;
    await tick();
    (open ? rail?.querySelector("button") : railToggle)?.focus();
  }
  let error = $state("");
  /** What the open server's page last did that its panel can't show (a world copied opens the copy). */
  let notice = $state("");
  /** Servers with a start/stop in flight. */
  let acting = $state<Record<string, boolean>>({});
  /** Servers LinuxGSM or Docker runs, found in a picked folder (null: not looked). */
  let found = $state<FoundServer[] | null>(null);
  let now = $state(Date.now() / 1000);
  /** A server run elsewhere being added (null: the form is closed). */
  let remote = $state<RemoteServer | null>(null);
  let remotePassword = $state("");
  let adding = $state(false);

  const card = $derived(sel === "server" ? (host?.servers.find((s) => s.name === selected) ?? null) : null);
  const current = $derived(card?.name ?? (sel === "new" ? t("hosting.new") : LISTS.includes(sel) ? listLabel(sel) : t("servers.rail.overview")));

  async function go(v: string) {
    if (!(await leave())) return;
    sel = v;
    if (railOpen) toggleRail(false);
    // what was typed, failed or checked for the server left behind isn't this one's
    address = null;
    items = null;
    error = notice = "";
  }
  async function openServer(name: string) {
    if (sel === "server" && selected === name) return railOpen && toggleRail(false);
    if (!(await leave())) return;
    selected = name;
    go("server");
  }
  async function pickTab(v: string) {
    if (v === shown || !(await leave())) return;
    tab = v;
    notice = "";
  }
  /** Join a server at this address through the server browser (its mods, profile and join). */
  async function join(to: { url: string; password?: string }, list = "direct") {
    if (!(await leave())) return;
    browserLink = to;
    go(list);
  }
  $effect(() => {
    if (link && link !== seenLink) {
      seenLink = link;
      join({ url: link.url });
    }
  });
  // automation is for servers PZMM runs; one run elsewhere is reached over RCON, its files over FTP
  const tabs = $derived(card?.remote ? ["console", "players", "files", ...(card.ftp?.pulled ? ["settings"] : [])] : ["console", "players", "mods", "settings", "backups", ...(card?.managed && !card.external ? ["world", "auto"] : [])]);
  let copied = $state(false);
  /** The upkeep command just copied. */
  let copiedCmd = $state("");
  /** The address players join at, being changed (null: not). */
  let address = $state<string | null>(null);
  let addressCopied = $state(false);

  async function saveAddress(s: ServerCard) {
    error = "";
    try {
      await api.hostAddressSet(s.name, address ?? "");
      address = null;
      refresh();
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function copyAddress(a: string) {
    await navigator.clipboard.writeText(a).catch(() => {});
    addressCopied = true;
    setTimeout(() => (addressCopied = false), 1500);
  }
  /** The Workshop check for a server run elsewhere. */
  let items = $state<ItemState[] | null>(null);
  const shown = $derived(tabs.includes(tab) ? tab : "console");
  const up = (s: ServerCard) => s.status === "running" || s.status === "starting";

  const refresh = () =>
    api
      .hostView()
      .then((h) => {
        host = h;
        if (sel === "server" && !h.servers.some((s) => s.name === selected)) sel = "overview";
      })
      .catch((e) => (error = t(...keyedError(e))));

  async function act(s: ServerCard, what: "start" | "stop" | "restart") {
    // what Check setup finds wrong with its config, before it starts (the same check)
    const problems = what === "stop" ? [] : (await api.hostPreflight(s.name).catch(() => [] as Finding[])).filter((f) => f.level === "error").map((f) => t(`doctor.${f.key}`, f.vars));
    if (what === "start" && problems.length) {
      if ((await ask(t("hosting.startCheckTitle", { name: s.name }), problems, [{ label: t("hosting.startAnyway"), icon: "play" }])) !== 0) return;
    }
    if (what !== "start") {
      const lines = [t("hosting.stopLine", { name: s.name }), ...(s.attached ? [] : [t("hosting.stopRcon")]), ...problems];
      const label = t(what === "stop" ? "hosting.stop" : "hosting.restart");
      // players on (or not known): they can be told a minute ahead
      const warn = s.status === "running" && s.players !== 0 ? [{ label: t(what === "stop" ? "hosting.warnStop" : "hosting.warnRestart"), icon: "message" as IconName }] : [];
      const i = await ask(t(what === "stop" ? "hosting.stopTitle" : "hosting.restartTitle", { name: s.name }), lines, [{ label, icon: what === "stop" ? "x" : "refresh" }, ...warn]);
      if (i < 0) return;
      if (i === 1) return warnFirst(s, what);
    }
    run(s, what);
  }

  /** Stops and restarts waiting out the minute players were warned of: their timers, by server. */
  let warned = $state<Record<string, number>>({});

  async function warnFirst(s: ServerCard, what: "stop" | "restart") {
    error = "";
    try {
      await api.hostSend(s.name, `servermsg ${q(t(what === "stop" ? "hosting.warnStopMsg" : "hosting.warnRestartMsg"))}`);
      warned[s.name] = window.setTimeout(() => {
        delete warned[s.name];
        run(s, what);
      }, 60000);
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  function callOff(s: ServerCard) {
    clearTimeout(warned[s.name]);
    delete warned[s.name];
    api.hostSend(s.name, `servermsg ${q(t("hosting.warnOffMsg"))}`).catch(() => {});
  }

  async function run(s: ServerCard, what: "start" | "stop" | "restart") {
    error = "";
    acting[s.name] = true;
    try {
      await (what === "start" ? api.hostStart(s.name) : what === "stop" ? api.hostStop(s.name) : api.hostRestart(s.name));
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      acting[s.name] = false;
      refresh();
    }
  }

  const buildName = (b: string) => t(b === "legacy41" ? "hosting.update.b41" : "hosting.update.b42");

  /** Update the server's files, or move them to the other build's branch. */
  async function updateServer(s: ServerCard) {
    const other = s.build === "legacy41" ? "b42" : "legacy41";
    const choices = [{ label: t("hosting.update.same", { build: buildName(s.build) }), icon: "update" as IconName }, { label: t("hosting.update.switch", { build: buildName(other) }), icon: "swap" as IconName }];
    const i = await ask(t("hosting.update.title", { name: s.name }), [t("hosting.update.line", { dir: s.install_dir }), t("hosting.update.switchLine")], choices);
    if (i < 0) return;
    error = "";
    acting[s.name] = true;
    try {
      await api.hostUpdate(s.name, i === 0 ? s.build : other);
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      acting[s.name] = false;
      refresh();
    }
  }

  async function abort(s: ServerCard) {
    try {
      await api.hostAbortRestart(s.name);
    } catch (e) {
      error = t(...keyedError(e));
    }
    refresh();
  }

  async function copyDiagnostics(s: ServerCard) {
    try {
      await navigator.clipboard.writeText(await api.hostDiagnostics(s.name));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function checkWorkshop(s: ServerCard) {
    error = "";
    try {
      items = await api.hostWorkshopStatus(s.name);
    } catch (e) {
      error = t(...keyedError(e));
    }
  }
  const outdated = (list: ItemState[]) => list.filter((i) => i.downloaded !== null && i.updated > i.downloaded);

  /** A sparkline's points for `values` scaled to `max`, 64 by 16. */
  const line = (values: number[], max: number) => values.map((v, i) => `${((i / Math.max(1, values.length - 1)) * 64).toFixed(1)},${(16 - (Math.min(v, max) / (max || 1)) * 15).toFixed(1)}`).join(" ");
  /** In-game date and time as the player's locale writes them. */
  const gameDate = ([y, mo, d, h, mi]: number[]) => fmtDateTime(new Date(y, mo - 1, d, h, mi).getTime() / 1000);

  async function adopt(s: ServerCard) {
    const dir = host?.server_dir ?? (await api.pickFolder(""));
    if (!dir) return;
    if ((await ask(t("hosting.adoptTitle", { name: s.name }), [t("hosting.adoptLine", { dir })], [{ label: t("hosting.adopt"), icon: "plus" }])) !== 0) return;
    try {
      await api.hostAdopt(s.name, dir);
      refresh();
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function forget(s: ServerCard) {
    const line = s.remote ? t("hosting.remote.forgetLine", { addr: s.remote }) : t("hosting.forgetLine", { dir: s.ini });
    if ((await ask(t("hosting.forgetTitle", { name: s.name }), [line], [{ label: t("hosting.forget"), icon: "trash" }])) !== 0) return;
    try {
      await (s.remote ? api.hostRemoteForget(s.name) : api.hostForget(s.name));
      refresh();
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function findExisting() {
    const dir = await api.pickFolder("");
    if (!dir) return;
    error = "";
    try {
      found = await api.hostDetect(dir);
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function addFound(f: FoundServer) {
    try {
      await api.hostAddExternal(f);
      const rest = found?.filter((x) => x !== f) ?? [];
      found = rest.length ? rest : null;
      openServer(f.name);
      refresh();
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  const kind = (k: string) => t(`hosting.external.${k}`);
  /** Names as a list in the player's language ("A, B and C"). */
  const list = (xs: string[]) => new Intl.ListFormat(intlLang(), { type: "conjunction" }).format(xs);

  /** Add a server run elsewhere: PZMM logs in to its RCON first. */
  async function addRemote(e: Event) {
    e.preventDefault();
    if (!remote) return;
    error = "";
    adding = true;
    try {
      await api.hostRemoteAdd({ ...remote, name: remote.name.trim(), host: remote.host.trim() }, remotePassword);
      openServer(remote.name.trim());
      remote = null;
      remotePassword = "";
      refresh();
    } catch (err) {
      error = t(...keyedError(err));
    } finally {
      adding = false;
    }
  }

  /** Status, uptime, players and memory, as far as they're known. */
  const facts = (s: ServerCard) =>
    s.remote ? t("hosting.remote.at", { addr: s.remote }) : s.external ? t("hosting.external.runBy", { kind: kind(s.external.kind) }) : [
      t(`hosting.status.${s.status}`),
      up(s) && s.started && uptime(s),
      s.status === "running" && s.players !== null && t("hosting.playerCount", { count: s.players }),
      s.memory_used && t("hosting.memory", { used: fmtSize(s.memory_used), max: fmtSize(s.memory_mb * 1048576) }),
      s.status === "running" && s.samples.length && t("hosting.cpu", { pct: Math.round(s.samples[s.samples.length - 1][0]) }),
      s.game_time && t("hosting.inGame", { date: gameDate(s.game_time) }),
      s.next_restart && !s.restart_at && t("hosting.nextRestart", { time: fmtDateTime(s.next_restart) }),
    ]
      .filter(Boolean)
      .join(" · ");

  function uptime(s: ServerCard) {
    const m = Math.max(0, Math.floor((now - s.started) / 60));
    return m < 60 ? t("hosting.minutes", { count: m }) : t("hosting.hours", { h: Math.floor(m / 60), m: m % 60 });
  }

  // shown (again): a look straight away
  $effect(() => {
    if (!hidden) untrack(refresh);
  });
  onMount(() => {
    api.launcherView().then((l) => (launcher = l)).catch(() => {});
    // status changes arrive as events; memory and uptime need a look now and then, while the tab shows
    const timer = setInterval(() => {
      if (hidden) return;
      now = Date.now() / 1000;
      if (host?.servers.some((s) => up(s) && !s.remote)) refresh();
    }, 5000);
    const unlisten = listen<ServerEvent>("server", ({ payload: e }) => {
      const s = host?.servers.find((s) => s.name === e.name);
      if (!s || (s.status === e.status && !e.reason)) return;
      if (s.status !== e.status) refresh();
      s.status = e.status;
      s.reason = e.reason;
    });
    return () => {
      clearInterval(timer);
      unlisten.then((f) => f());
    };
  });

</script>

{#snippet dot(s: ServerCard)}<span class="dot {s.managed ? s.status : 'unmanaged'}" aria-hidden="true"></span>{/snippet}

{#snippet spark(s: ServerCard)}
  {#if s.samples.length > 1 && s.status === "running"}
    <svg class="spark" viewBox="0 0 64 16" role="img" aria-label={t("hosting.sparkLabel")}>
      <title>{t("hosting.sparkLabel")}</title>
      <polyline class="mem" points={line(s.samples.map((x) => x[1]), s.memory_mb * 1048576)} />
      <polyline class="cpu" points={line(s.samples.map((x) => x[0]), 100)} />
    </svg>
  {/if}
{/snippet}

{#snippet actions(s: ServerCard)}
  <span class="btns">
    {#if s.remote}
      <!-- run elsewhere: reached over RCON only -->
    {:else if !s.managed}
      <button onclick={() => adopt(s)} title={t("hosting.adoptTip")}><Icon name="plus" size={14} />{t("hosting.adopt")}</button>
    {:else if s.external}
      <!-- run by LinuxGSM or Docker: started and stopped there -->
    {:else if up(s) || s.status === "stopping"}
      {#if s.status === "running"}<button onclick={async () => join(await ownJoin(s))} title={t("hosting.joinTip")}><Icon name="play" size={14} />{t("hosting.join")}</button>{/if}
      {#if warned[s.name]}
        <button onclick={() => callOff(s)} title={t("hosting.warnedTip")}><Icon name="x" size={14} />{t("hosting.abort")}</button>
      {:else}
        <button onclick={() => act(s, "restart")} disabled={acting[s.name] || s.status !== "running"}><Icon name="refresh" size={14} />{t("hosting.restart")}</button>
        <button onclick={() => act(s, "stop")} disabled={acting[s.name] || s.status === "stopping"}><Icon name="x" size={14} />{t("hosting.stop")}</button>
      {/if}
    {:else}
      <button onclick={() => updateServer(s)} disabled={acting[s.name]} title={t("hosting.update.tip")}><Icon name="update" size={14} />{t("hosting.update.button")}</button>
      <button onclick={() => act(s, "start")} disabled={acting[s.name]}><Icon name="play" size={14} />{t("hosting.start")}</button>
    {/if}
  </span>
{/snippet}

<!-- updates waiting and a restart counting down; why it crashed -->
{#snippet notes(s: ServerCard)}
  {#if (s.updates.length || s.restart_at) && s.managed && !s.external && !s.remote}
    <div class="pending" role="status">
      <span class="mark"><Icon name="update" size={14} /></span>
      <span>
        {#if s.updates.length}{t("hosting.updatesPending", { count: s.updates.length })}{/if}
        {#if s.restart_at}{s.updates.length ? "· " : ""}{t("hosting.restartIn", { count: Math.max(1, Math.ceil((s.restart_at - now) / 60)) })}{/if}
        <span class="hint">{list(s.updates)}</span>
      </span>
      {#if s.restart_at}
        <button onclick={() => abort(s)} title={t("hosting.abortTip")}><Icon name="x" size={14} />{t("hosting.abort")}</button>
      {/if}
      {#if s.status === "running"}
        <button onclick={() => act(s, "restart")} disabled={acting[s.name]}><Icon name="refresh" size={14} />{t("hosting.restartNow")}</button>
      {/if}
    </div>
  {/if}
  {#if s.reason && s.status === "crashed"}
    <p class="reason" role="alert"><Icon name="alert" size={14} />{t(`hosting.note.${s.reason.key}`, s.reason.vars)}</p>
  {/if}
{/snippet}

<!-- the narrow drawer closes on a click outside it, and on Esc (focus back on its button) -->
<svelte:window onpointerdown={(e) => railOpen && !rail?.contains(e.target as Node) && !railToggle?.contains(e.target as Node) && (railOpen = false)}
  onkeydown={(e) => e.key === "Escape" && railOpen && toggleRail(false)} />

<section class="servers" aria-label={t("servers.label")}>
  <nav class="rail" class:open={railOpen} aria-label={t("servers.rail.label")} bind:this={rail}>
    <h3>{t("servers.rail.play")}</h3>
    {#each lists as v}
      <button class:active={sel === v} aria-current={sel === v ? "page" : undefined} onclick={() => go(v)}><Icon name={LIST_ICONS[v]} size={14} />{listLabel(v)}</button>
    {/each}
    <h3>{t("hosting.title")}</h3>
    <button class:active={sel === "overview"} aria-current={sel === "overview" ? "page" : undefined} onclick={() => go("overview")}><Icon name="pulse" size={14} />{t("servers.rail.overview")}</button>
    {#each host?.servers ?? [] as s (s.name)}
      <button class="srv" class:active={card?.name === s.name} aria-current={card?.name === s.name ? "page" : undefined} onclick={() => openServer(s.name)}
        title={s.managed ? t(`hosting.status.${s.status}`) : t("hosting.unmanaged")}>
        {@render dot(s)}<span class="name">{s.name}</span>
        {#if s.status === "running" && s.players !== null}<span class="count" aria-label={t("hosting.playerCount", { count: s.players })}>{fmtNum(s.players)}</span>{/if}
      </button>
    {/each}
    <button class="new" class:active={sel === "new"} aria-current={sel === "new" ? "page" : undefined} onclick={() => go("new")} disabled={busy}><Icon name="plus" size={14} />{t("hosting.new")}</button>
  </nav>

  <div class="main">
    <div class="mbar">
      <button bind:this={railToggle} onclick={() => toggleRail()} aria-expanded={railOpen}><Icon name="server" size={14} />{current}<Icon name={railOpen ? "chevron-up" : "chevron-down"} size={14} /></button>
    </div>

    {#if LISTS.includes(sel)}
      <ServerBrowser show={sel} {hidden} {busy} {lock} {installedIds} link={browserLink} {onSetup} {onCheck} {onSettings} />
    {:else if sel === "new"}
      <div class="scroll">
        {#if host}
          <ServerWizard {host} {profiles} {profile} taken={host.servers.map((s) => s.name)}
            onDone={(name) => { openServer(name); refresh(); }} onCancel={() => go("overview")} />
        {/if}
      </div>
    {:else if card}
      <section class="page" aria-label={card.name}>
        <header class="page-head">
          {@render dot(card)}
          <div class="title">
            <h2>{card.name}</h2>
            <span class="hint">{card.managed ? facts(card) : t("hosting.unmanaged")}</span>
          </div>
          {@render spark(card)}
          {@render actions(card)}
        </header>
        <div class="bars">
          {@render notes(card)}
          <div class="subbar">
            <div class="subtabs" role="tablist" aria-label={t("hosting.pageLabel")} tabindex="-1" onkeydown={(e) => arrowPick(e, tabs, shown, pickTab)}>
              {#each tabs as id}
                <button role="tab" id="srv-tab-{id}" aria-controls="srv-panel" aria-selected={shown === id} tabindex={shown === id ? 0 : -1} class:active={shown === id} onclick={() => pickTab(id)}>{t(`hosting.tabs.${id}`)}</button>
              {/each}
            </div>
            <span class="hint mono where" title={card.remote ?? card.ini}>{card.remote ?? card.ini}</span>
            {#if !card.remote}
              <button class="quiet" onclick={() => card && copyDiagnostics(card)} title={t("hosting.diagnosticsTip")}><Icon name="copy" size={14} />{copied ? t("hosting.console.copied") : t("hosting.diagnostics")}</button>
            {/if}
            {#if card.managed && (!up(card) || card.remote)}
              <button class="quiet danger" onclick={() => card && forget(card)}><Icon name="trash" size={14} />{t("hosting.forget")}</button>
            {/if}
          </div>
          {#if card.external}
            <p class="hint run">
              {t("hosting.external.how", { kind: kind(card.external.kind), folder: card.external.folder })}
              {#each card.external.run as r}<code class="mono">{r}</code>{/each}
            </p>
            <div class="upkeep">
              {#each card.upkeep as [what, cmd]}
                <div class="row">
                  <span class="hint">{t(`hosting.external.upkeep.${what}`)}</span>
                  <code class="mono">{cmd}</code>
                  <button class="quiet" onclick={() => navigator.clipboard.writeText(cmd).then(() => ((copiedCmd = cmd), setTimeout(() => (copiedCmd = ""), 1500)), () => {})} title={t("hosting.external.copyCmd")} aria-label={t("hosting.external.copyCmd")}><Icon name={copiedCmd === cmd ? "check" : "copy"} size={14} /></button>
                </div>
              {/each}
              <div class="row">
                <button onclick={() => card && checkWorkshop(card)}><Icon name="refresh" size={14} />{t("hosting.external.checkWorkshop")}</button>
                {#if items}
                  {@const old = outdated(items)}
                  <span class="hint" role="status">
                    {#if items.every((i) => i.downloaded === null)}{t("hosting.external.wsUnknown", { count: items.length })}
                    {:else if old.length}{t("hosting.external.wsOutdated", { count: old.length, titles: list(old.map((i) => i.title)) })}
                    {:else}{t("hosting.external.wsCurrent", { count: items.length })}{/if}
                  </span>
                {/if}
              </div>
            </div>
          {/if}
          {#if card.managed && !card.remote}
            <div class="row address">
              {#if address !== null}
                <form class="row" onsubmit={(e) => { e.preventDefault(); if (card) saveAddress(card); }}>
                  <!-- i18n-ignore: an address example -->
                  <input type="text" bind:value={address} spellcheck="false" placeholder="example.gl.joinmc.link:12345" aria-label={t("hosting.address.label")} />
                  <button type="submit"><Icon name="check" size={14} />{t("hosting.address.save")}</button>
                  <button class="quiet" type="button" onclick={() => (address = null)}>{t("common.cancel")}</button>
                </form>
              {:else if card.address}
                <span>{t("hosting.address.at", { address: card.address })}</span>
                <button class="quiet" onclick={() => card && copyAddress(card.address)}><Icon name="copy" size={14} />{addressCopied ? t("hosting.console.copied") : t("hosting.address.copy")}</button>
                <button class="quiet" onclick={() => (address = card?.address ?? "")}>{t("hosting.address.change")}</button>
              {:else}
                <button class="quiet" onclick={() => (address = "")} title={t("hosting.address.tip")}><Icon name="share" size={14} />{t("hosting.address.set")}</button>
              {/if}
            </div>
          {/if}
          {#if notice}<p class="hint" role="status">{notice}</p>{/if}
          {#if error}<p class="error">{error}</p>{/if}
        </div>
        <div class="content" class:fill={shown === "console"} id="srv-panel" role="tabpanel" aria-labelledby="srv-tab-{shown}">
          {#key card.name}
            {#if shown === "console"}
              <ServerConsole server={card} {hidden} />
            {:else if shown === "players"}
              <ServerPlayers server={card} {ask} {hidden} />
            {:else if shown === "files"}
              <ServerFiles server={card} onChanged={refresh} />
            {:else if shown === "mods"}
              <ServerMods server={card} {profiles} {profile} {rows} {ask} bind:dirty />
            {:else if shown === "settings"}
              <ServerSettings server={card} bind:dirty />
            {:else if shown === "backups"}
              <ServerBackups server={card} {ask} />
            {:else if shown === "world"}
              <ServerWorld server={card} {ask} onChanged={(name) => { if (name) [selected, notice] = [name, t("hosting.world.copied", { name })]; refresh(); }} />
            {:else}
              <ServerAuto server={card} onSaved={refresh} bind:dirty />
            {/if}
          {/key}
        </div>
      </section>
    {:else}
      <div class="scroll">
        <div class="head">
          <div>
            <h2>{t("hosting.title")}</h2>
            <p class="lede">{t("hosting.lede")}</p>
          </div>
          <span class="btns">
            <button onclick={() => (remote = { name: "", host: "", port: 27015, build: "b42" })} disabled={busy} title={t("hosting.remote.addTip")}><Icon name="server" size={14} />{t("hosting.remote.add")}</button>
            <button onclick={findExisting} disabled={busy} title={t("hosting.external.findTip")}><Icon name="search" size={14} />{t("hosting.external.find")}</button>
            <!-- one primary: the remote form's Connect while it's open -->
            <button class:primary={!remote} onclick={() => go("new")} disabled={busy}><Icon name="plus" size={14} />{t("hosting.new")}</button>
          </span>
        </div>
        {#if found}
          <div class="found" role="region" aria-label={t("hosting.external.foundLabel")}>
            {#if !found.length}
              <p class="hint">{t("hosting.external.none")}</p>
            {/if}
            {#each found as f}
              <div class="row">
                <span><strong>{f.name}</strong> <span class="hint">{t("hosting.external.runBy", { kind: kind(f.external.kind) })} · <span class="mono">{f.data}</span></span></span>
                <button onclick={() => addFound(f)}><Icon name="plus" size={14} />{t("hosting.external.add")}</button>
              </div>
            {/each}
            <button class="quiet" onclick={() => (found = null)}>{t("common.close")}</button>
          </div>
        {/if}

        {#if remote}
          <form class="found remote" onsubmit={addRemote} use:trap aria-label={t("hosting.remote.add")}>
            <p class="hint">{t("hosting.remote.intro")}</p>
            <div class="fields">
              <label>{t("hosting.remote.name")}<input type="text" bind:value={remote.name} required maxlength="60" /></label>
              <label>{t("hosting.remote.host")}<input type="text" bind:value={remote.host} required spellcheck="false" placeholder="203.0.113.7" /></label>
              <label>{t("hosting.remote.port")}<input type="number" bind:value={remote.port} min="1" max="65535" required /></label>
              <label>{t("hosting.remote.password")}<input type="password" bind:value={remotePassword} required autocomplete="off" /></label>
              <label>{t("hosting.remote.build")}
                <select bind:value={remote.build}>
                  <option value="b42">{t("hosting.remote.b42")}</option>
                  <option value="legacy41">{t("hosting.remote.b41")}</option>
                </select>
              </label>
            </div>
            <p class="hint">{t("hosting.remote.plain")}</p>
            <div class="btns">
              <button class="primary" type="submit" disabled={adding}><Icon name="plus" size={14} />{adding ? t("hosting.remote.connecting") : t("hosting.remote.connect")}</button>
              <button class="quiet" type="button" onclick={() => (remote = null)}>{t("common.cancel")}</button>
            </div>
          </form>
        {/if}
        {#if error}<p class="error">{error}</p>{/if}

        {#if host && !host.servers.length}
          <p class="hint">{t("hosting.none")}</p>
        {/if}
        {#if host?.servers.length}
          <ul class="cards" aria-label={t("hosting.listLabel")}>
            {#each host.servers as s (s.name)}
              <li class="card">
                <div class="card-head">
                  <button class="pick" onclick={() => openServer(s.name)}>{@render dot(s)}<strong>{s.name}</strong></button>
                  {@render spark(s)}
                </div>
                <span class="hint">{s.managed ? facts(s) : t("hosting.unmanaged")}</span>
                {@render notes(s)}
                {@render actions(s)}
              </li>
            {/each}
          </ul>
        {/if}

        {#if launcher?.history.length}
          <h3>{t("servers.recent")}</h3>
          <ul class="recent">
            {#each launcher.history.slice(0, 5) as s (s.addr)}
              <li>
                <span class="name"><strong>{s.name}</strong> <span class="hint mono">{connectOf(s)}</span></span>
                <button onclick={() => join({ url: connectOf(s) }, "history")}><Icon name="play" size={14} />{t("hosting.join")}</button>
              </li>
            {/each}
          </ul>
        {/if}

        <div class="card configs">
          <ServerConfigs {busy} {lock} {profile} {onImport} {ask} />
        </div>
      </div>
    {/if}
  </div>
</section>

<style>
  /* the tab edge to edge: a rail of lists and servers down the left, the open one filling the rest; only
     the panes scroll. In narrow windows the rail is a drawer the bar above the main area opens */
  .servers {
    position: relative;
    display: flex;
    min-height: 0;
    background: var(--bg);
  }
  .rail {
    flex: none;
    width: 240px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: 4px 0 12px;
    background: var(--panel);
    border-right: 1px solid var(--line);
  }
  .rail h3 {
    margin: 14px 16px 4px;
    font-size: 12px;
    color: var(--muted);
  }
  .rail button {
    flex: none;
    justify-content: flex-start;
    gap: 10px;
    padding: 7px 14px;
    border: 0;
    border-radius: 0;
    border-left: 2px solid transparent;
    background: transparent;
    text-align: left;
  }
  .rail button:hover:not(:disabled),
  .rail button.active {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-left-color: var(--on-selected);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .rail .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rail .count {
    padding: 0 6px;
    border-radius: 8px;
    background: var(--raised);
    font-size: 11px;
    font-weight: 700;
  }
  .rail .new {
    color: var(--muted);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .mbar {
    display: none;
  }
  @media (max-width: 900px) {
    .mbar {
      flex: none;
      display: flex;
      align-items: center;
      height: 40px;
      padding: 0 12px;
      background: var(--panel);
      border-bottom: 1px solid var(--line);
    }
    .rail {
      position: absolute;
      z-index: 3;
      top: 40px;
      left: 0;
      bottom: 0;
      box-shadow: var(--shadow);
    }
    .rail:not(.open) {
      display: none;
    }
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 20px 32px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
    flex-wrap: wrap;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
  }
  h3 {
    margin: 24px 0 6px;
    font-size: 15px;
  }
  .lede {
    max-width: 70ch;
  }
  .card {
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
    padding: 14px 16px;
    min-width: 0;
  }
  .cards {
    list-style: none;
    padding: 0;
    margin: 12px 0 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 12px;
  }
  .cards .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cards .card > .btns {
    margin-top: auto;
  }
  .card-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .card-head .pick {
    flex: 1;
    min-width: 0;
    justify-content: flex-start;
    gap: 10px;
    padding: 2px 0;
    border: 0;
    background: none;
    font-size: 16px;
    text-align: left;
    overflow-wrap: anywhere;
  }
  .card-head .pick {
    margin-left: -6px;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .card-head .pick:hover:not(:disabled) {
    color: var(--on-selected);
    background: var(--hover);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  /* the cards, recent servers and configs one width: the main area's */
  .configs {
    margin-top: 24px;
  }
  .recent {
    list-style: none;
    padding: 0;
    margin: 0;
    border-top: 1px solid var(--line);
  }
  .recent li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .recent .name {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .pending {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }
  .pending > span:not(.mark) {
    flex: 1;
    min-width: 0;
  }
  .mark {
    display: flex;
    color: var(--warn);
  }
  .reason {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--danger);
  }
  /* a server: its header, then the open section filling the rest (the console at full height) */
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .page-head {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 12px 20px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .page-head .title {
    flex: 1;
    min-width: 200px;
    overflow-wrap: anywhere;
  }
  .page-head h2 {
    margin: 0;
    font-size: 20px;
  }
  .bars {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 20px 0;
  }
  .subbar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 20px 20px;
  }
  .content.fill {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .content.fill > :global(.console) {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .content.fill :global(.console > *) {
    flex: none;
  }
  .content.fill :global(.console > .log) {
    flex: 1 1 0;
    min-height: 160px;
    height: auto;
  }
  /* a short window: the server's page scrolls as one, its header and bars no longer squeezing the section to nothing */
  @media (max-height: 700px) {
    .page {
      overflow-y: auto;
    }
    .content,
    .content.fill {
      flex: none;
      overflow: visible;
    }
    .content.fill :global(.console > .log) {
      flex: none;
      height: 280px;
    }
  }
  .found {
    margin: 10px 0;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--panel);
    box-shadow: var(--card-shadow);
  }
  .found .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 0;
    overflow-wrap: anywhere;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 8px 12px;
    margin: 8px 0;
  }
  .fields label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
  }
  .run {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0;
  }
  .run code {
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--panel);
  }
  .btns {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--muted);
  }
  .dot.running {
    background: var(--ok);
  }
  .dot.starting,
  .dot.stopping {
    background: var(--warn);
  }
  .dot.crashed {
    background: var(--danger);
  }
  .dot.unmanaged {
    background: transparent;
    border: 1px solid var(--muted);
  }
  .where {
    flex: 1 1 200px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spark {
    flex: none;
    width: 64px;
    height: 16px;
    fill: none;
    stroke-width: 1.5;
  }
  .spark .cpu {
    stroke: var(--accent-text);
  }
  .spark .mem {
    stroke: var(--muted);
  }
  .address,
  .address form,
  .upkeep .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .upkeep code {
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--panel);
  }
</style>
