<script module lang="ts">
  // kept at module level: Home is drawn afresh each time it's opened, and what it asked the network
  // stays a while, so switching tabs never asks again
  import { keptServers } from "./order";
  import { api, fromSteam, patient, type Feed, type Finding, type GameServer, type HostView, type LauncherView, type News, type Save, type ServerMods, type View } from "./api";

  const MINUTES = 60_000;

  let feedAsked: { at: number; p: Promise<Feed> } | null = null;
  /** After updates or a launch: the next glance asks again. */
  export const forgetGlance = () => (feedAsked = null);
  /** Your Workshop mods updated since you last played (without their notes), asked at most every five
   * minutes: What's new's count comes from here too. */
  export function glance(): Promise<Feed> {
    if (!feedAsked || Date.now() - feedAsked.at > 5 * MINUTES) {
      const p = patient(() => api.feed(false));
      feedAsked = { at: Date.now(), p };
      p.catch(() => (feedAsked = null));
    }
    return feedAsked.p;
  }

  /** Servers' answers (A2S, five minutes) and their mods worked out (null: that failed), by address. */
  const live = $state<Record<string, { at: number; s: GameServer; mods?: ServerMods | null }>>({});
  /** Servers whose mods are being worked out: a join waits for that answer rather than asking again. */
  const asking = new Map<string, Promise<ServerMods | null>>();
  /** Favourites and history, and the servers hosted here: drawn at once when Home opens again, then asked afresh. */
  let launcher = $state<LauncherView | null>(null);
  let host = $state<HostView | null>(null);
  /** Check setup's answer for a view of the profile (a new one after every change), asked again after five minutes. */
  let checked: { view: View; at: number; findings: Finding[] } | null = null;
  type Popular = { at: number; steam: boolean; list: GameServer[]; error: string };
  /** Versioned: a list kept in another shape isn't read as this one. */
  const POPULAR_KEY = "pzmm.popular.v2";
  /** The busiest modded servers that answered, from Steam's list or else the game's own. Kept between runs:
   * without a key, asking Steam shows the player in-game there for a moment, so only a refresh asks again. */
  let popular = $state<Popular | null>(stored());
  /** The list kept, if it's one this version wrote (anything else is dropped, never drawn). */
  function stored(): Popular | null {
    try {
      const k = keptServers(localStorage.getItem(POPULAR_KEY));
      return k && { ...k, error: "" };
    } catch {
      return null; // storage blocked
    }
  }
  let news = $state<{ at: number; news: News | null; error: string } | null>(null);
  type Pick = { kind: "home" } | { kind: "server"; s: GameServer } | { kind: "hosted"; name: string };
  /** What the page shows: Home (the active profile), a server, or a server hosted here. */
  let sel = $state<Pick>({ kind: "home" });
  type LibTab = "popular" | "favourites" | "recent" | "hosted";
</script>

<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { connectOf, joinPlan, knownItems, ownJoin, WINDOWS, type ServerCard, type ServerEvent, type SetupItem } from "./api";
  import { modNeed } from "./order";
  import { fmtAgo, fmtDate, fmtNum } from "./format";
  import { counted } from "./findings";
  import { keyedError, t } from "./i18n.svelte";
  import { thumb } from "./thumb";
  import Icon, { type IconName } from "./Icon.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import Scrap from "./Scrap.svelte";
  import { arrowPick } from "./ui";
  import { joinInServers, showInServers } from "./Servers.svelte";
  import { showInSettings } from "./Settings.svelte";

  let { view, profiles, enabled, problems, busy, lock = "", installedIds, zbNeed = null, save = null, onSetup, onCheck, onLaunch, onUseProfile, onGo, onUpdate, onOpenLink, onCheckSetup, ask }: {
    view: View | null;
    /** The profiles of the game being played. */
    profiles: string[];
    /** Enabled mods in the active profile, and the problems its list shows. */
    enabled: number;
    problems: number;
    busy: boolean;
    /** Why switching profile is blocked now ("" when it isn't): setting up a server waits. */
    lock?: string;
    /** Installed mod ids: a server's mods to get, before they're worked out. */
    installedIds: string[];
    /** Enabled mods need ZombieBuddy and it isn't set up (`partial`: only some of it, as after a game update). */
    zbNeed?: { count: number; partial: boolean } | null;
    /** The latest save, when it was played with the active profile's mods. */
    save?: Save | null;
    /** Download a server's mods and switch to its own profile (the Servers tab's). */
    onSetup: (server: GameServer, items: string[], mods: string[], maps: string[], known?: SetupItem[], unlisted?: number) => Promise<boolean>;
    /** The check before the game starts: false when the player stopped there. */
    onCheck: () => Promise<boolean>;
    onLaunch: () => void;
    onUseProfile: (name: string) => Promise<boolean>;
    /** Another tab: "mods", "feed", "servers"... (`filter`: My mods' status filter). */
    onGo: (tab: string, filter?: string) => void;
    /** Update these mods (What's new's Update). */
    onUpdate: (ids: string[]) => Promise<unknown>;
    /** A server's details in Servers (where it asks for a password). */
    onOpenLink: (url: string) => void;
    /** My mods with Check setup open. */
    onCheckSetup: () => void;
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
  } = $props();

  const ART = "https://cdn.cloudflare.steamstatic.com/steam/apps/108600/library_hero.jpg";
  const LOGO = "https://cdn.cloudflare.steamstatic.com/steam/apps/108600/logo.png";
  let feed = $state<Feed | null>(null);
  let feedError = $state(false);
  let findings = $state<Finding[] | null>(null);
  /** The check `findings` should answer for: an older one finishing late doesn't count. */
  let checking = "";
  let popularLoading = $state(false);
  let search = $state("");
  /** The library tab: Popular each time Home opens (drawn from the last list kept; see `popular`). */
  let libTab = $state<LibTab>("popular");
  let artFailed = $state(false);
  let logoFailed = $state(false);
  let featFailed = $state(false);
  /** Check setup couldn't run. */
  let checkFailed = $state(false);
  let menuAt = $state<{ x: number; y: number } | null>(null);
  /** The server being joined or started, and what the last action said. */
  let working = $state("");
  let said = $state<{ text: string; error: boolean } | null>(null);

  const ready = patient(() => api.launcherView()).then(
    (l) => (launcher = l),
    () => (launcher ??= { favourites: [], history: [], joins: {}, key_saved: false, countries: false }),
  );

  const installed = $derived(new Set(installedIds));
  const liveOf = (s: GameServer) => live[s.addr]?.s ?? s;
  const q = $derived(search.trim().toLowerCase());
  const match = (name: string) => !q || name.toLowerCase().includes(q);
  const favourites = $derived((launcher?.favourites ?? []).map(liveOf));
  const recent = $derived((launcher?.history ?? []).slice(0, 10).map(liveOf));
  const hosted = $derived((host?.servers ?? []).filter((c) => c.managed));
  /** What Check setup finds wrong with what Play starts (a server config's findings are that server's). */
  const issues = $derived(findings ? counted(findings).length : 0);
  /** When you last played (0: no sign of play yet). */
  const played = $derived(feed?.basis ? feed.since : 0);
  const posts = $derived(news?.news?.posts ?? []);
  /** The updated mods PZMM installed and hasn't updated yet (the rest Steam keeps up to date). */
  const pending = $derived(feed?.entries.filter((e) => e.pending).length ?? 0);

  const server = $derived(sel.kind === "server" ? liveOf(sel.s) : null);
  const card = $derived(sel.kind === "hosted" ? (hosted.find((c) => c.name === (sel as { name: string }).name) ?? null) : null);
  const isFavourite = $derived(!!server && favourites.some((f) => f.addr === server.addr));

  /** Ask the servers not asked in the last five minutes (the backend asks 32 at a time and caches too). */
  async function probe(servers: GameServer[]) {
    const stale = servers.filter((s) => !live[s.addr] || Date.now() - live[s.addr].at > 5 * MINUTES);
    if (!stale.length) return;
    const got = await api.serversProbe(stale, false).catch(() => stale.map((s) => ({ ...s, probed: false })));
    for (const s of got) live[s.addr] = { at: Date.now(), s, mods: live[s.addr]?.mods };
  }

  /** Work out a server's mods once (installed, the indexes, then the Workshop); while that's asked, its answer. */
  function resolve(s: GameServer): Promise<ServerMods | null | undefined> {
    const l = live[s.addr];
    if (!l || l.mods !== undefined || !l.s.probed || !l.s.mod_count) return Promise.resolve(asking.get(s.addr));
    l.mods = null;
    const p = patient(() => api.serverMods(l.s)).catch(() => null).then((mods) => {
      asking.delete(s.addr);
      if (live[s.addr]) live[s.addr].mods = mods;
      return mods;
    });
    asking.set(s.addr, p);
    return p;
  }
  /** A server row hovered a moment: its mods are worked out before it's picked, so Join doesn't wait. */
  let warming = 0;
  const warm = (s: GameServer) => {
    clearTimeout(warming);
    warming = window.setTimeout(() => probe([s]).then(() => resolve(s)), 250);
  };

  /** Its mods you don't have, and the ones it doesn't list (the Servers tab counts the same). */
  const need = (s: GameServer) => modNeed(s, installed, live[s.addr]?.mods);
  // a server's page: ask it now, work out its mods (only a new pick clears what was said)
  $effect(() => {
    const s = sel.kind === "server" ? sel.s : null;
    untrack(() => {
      said = null;
      if (s) probe([s]).then(() => resolve(s));
    });
  });

  /** Set up (its mods, in a profile of its own) unless `asIs`, then start the game joining it. A password
   * not remembered is asked for in the server's details. */
  async function joinServer(s: GameServer, asIs = false) {
    said = null;
    // its details in Servers ask for the password before joining (and say so)
    if (s.password && !(await api.serverPasswordSaved(connectOf(s)).catch(() => false))) return onOpenLink(connectOf(s));
    working = s.addr;
    try {
      if (!asIs && s.mod_count) {
        if (!live[s.addr]?.mods) said = { text: t("home.findingMods"), error: false };
        const mods = live[s.addr]?.mods ?? (await resolve(s)) ?? (await patient(() => api.serverMods(s)));
        said = null;
        // a mod not found on the Workshop: the server's page in Servers says which, and how to get it
        if (mods.mods.some((m) => m.status === "unknown")) return onOpenLink(connectOf(s));
        // best matches count: the setup popup lists them before anything downloads
        const plan = joinPlan(s, mods, undefined, true);
        if (!(await onSetup(s, plan.downloads.map((d) => d.item), plan.order, mods.maps, knownItems(mods), modNeed(s, installed, mods).unlisted))) return;
      }
      if (!(await onCheck())) return;
      await api.serverJoin(s, "", false);
      said = { text: t("launcher.started"), error: false };
      launcher = await patient(() => api.launcherView());
    } catch (e) {
      said = { text: t(...keyedError(e)), error: true };
    } finally {
      done();
    }
  }
  /** The setup popup closed while Join was disabled, so focus fell to the page: back on Join once it's enabled. */
  function done() {
    working = "";
    if (document.activeElement === document.body) tick().then(() => document.querySelector<HTMLElement>(".playbar .big")?.focus());
  }

  /** Download a server's mods into its own profile, without joining. */
  async function getMods(s: GameServer) {
    said = live[s.addr]?.mods ? null : { text: t("home.findingMods"), error: false };
    working = s.addr;
    try {
      const mods = live[s.addr]?.mods ?? (await resolve(s)) ?? (await patient(() => api.serverMods(s)));
      said = null;
      if (mods.mods.some((m) => m.status === "unknown")) return onOpenLink(connectOf(s));
      const plan = joinPlan(s, mods, undefined, true);
      if (await onSetup(s, plan.downloads.map((d) => d.item), plan.order, mods.maps, knownItems(mods), modNeed(s, installed, mods).unlisted)) {
        // worked out again, now with them installed
        if (live[s.addr]) live[s.addr].mods = undefined;
        resolve(s);
      }
    } catch (e) {
      said = { text: t(...keyedError(e)), error: true };
    } finally {
      done();
    }
  }

  /** Start a server hosted here: what Check setup finds wrong with its config first, as Servers does. */
  async function start(c: ServerCard) {
    said = null;
    const wrong = (await api.hostPreflight(c.name).catch(() => [] as Finding[])).filter((f) => f.level === "error").map((f) => t(`doctor.${f.key}`, f.vars));
    if (wrong.length && (await ask(t("hosting.startCheckTitle", { name: c.name }), wrong, [{ label: t("hosting.startAnyway"), icon: "play" }])) !== 0) return;
    working = c.name;
    try {
      await api.hostStart(c.name);
    } catch (e) {
      said = { text: t(...keyedError(e)), error: true };
    } finally {
      working = "";
      refreshHost();
    }
  }
  const refreshHost = () => patient(() => api.hostView()).then((h) => (host = h)).catch(() => {});

  /** Play a profile: switch to it first when it isn't the active one. */
  async function play(name: string) {
    if (name !== view?.profile && !(await onUseProfile(name))) return;
    onLaunch();
  }

  async function favourite(s: GameServer) {
    try {
      const st = await api.serverFavourite(s, !isFavourite);
      if (launcher) launcher = { ...launcher, ...st };
    } catch (e) {
      said = { text: t(...keyedError(e)), error: true };
    }
  }

  async function copy(text: string) {
    await navigator.clipboard.writeText(text).catch(() => {});
    said = { text: t("home.copied"), error: false };
  }

  function servers(where: string, name = "") {
    showInServers(where, name);
    onGo("servers");
  }

  const pick = (p: Pick) => (sel = p);
  const shownTab = $derived<LibTab>(libTab === "hosted" && !hosted.length ? "popular" : libTab);

  /** The updated mods' Workshop preview images, by item: "" while asked or when there's none. */
  let previews = $state<Record<string, string>>({});
  $effect(() => {
    const ids = (feed?.entries.slice(0, 5) ?? []).map((e) => e.item).filter((id) => !(id in previews));
    if (!ids.length) return;
    for (const id of ids) previews[id] = "";
    api.workshopItems(ids).then((items) => items.forEach((i) => (previews[i.id] = i.preview_url)), () => {});
  });

  async function loadPopular(fresh = false) {
    if (popularLoading) return;
    await ready;
    // Steam's list is asked on its own, kept ten minutes (without a Web API key, Steam shows you as playing for a moment)
    if (!fresh && popular && Date.now() - popular.at < 10 * MINUTES) return void probe(popular.list);
    popularLoading = true;
    let steam = true;
    try {
      const listing = await patient(() => api.serversList(fresh));
      steam = fromSteam(listing.sources);
      // the 64 busiest by the list's count (two batches of A2S), then the modded ones that answered
      const top = listing.servers.sort((a, b) => b.players - a.players).slice(0, 64);
      await probe(top);
      const got = top.map(liveOf).filter((s) => s.probed && s.mod_count > 0);
      popular = { at: Date.now(), steam, list: got.sort((a, b) => b.players - a.players).slice(0, 30), error: "" };
      try {
        // kept unasked (its numbers are old by the next start); the game's own list isn't kept, so the next start offers Steam's
        if (steam) localStorage.setItem(POPULAR_KEY, JSON.stringify({ ...popular, list: popular.list.map((s) => ({ ...s, probed: false })) }));
      } catch {
        // not kept: the next start asks again
      }
    } catch (e) {
      // the list shown stays, the error beside it
      popular = { ...(popular ?? { steam, list: [] }), at: Date.now(), error: t(...keyedError(e)) };
    } finally {
      popularLoading = false;
    }
  }
  $effect(() => {
    if (shownTab === "popular") loadPopular();
  });

  async function loadNews() {
    if (news && Date.now() - news.at < 60 * MINUTES) return;
    try {
      news = { at: Date.now(), news: await patient(() => api.news()), error: "" };
    } catch (e) {
      news = { at: Date.now(), news: null, error: t(...keyedError(e)) };
    }
  }

  // the setup check follows the profile (any change to it, an install or an update reloads it); reopening
  // Home shows the last answer, asked again after a while
  $effect(() => {
    if (!view) return;
    const v = view;
    const key = JSON.stringify([v.profile, v.order]);
    checking = key;
    if (checked?.view === v) {
      findings = checked.findings;
      if (Date.now() - checked.at < 5 * MINUTES) return;
    }
    patient(() => api.doctor()).then(
      (f) => {
        checked = { view: v, at: Date.now(), findings: f };
        if (checking === key) (findings = f), (checkFailed = false);
      },
      () => checking === key && (checkFailed = true),
    );
  });

  onMount(() => {
    refreshHost();
    glance().then((f) => (feed = f), () => (feedError = true));
    loadNews();
    ready.then((l) => probe([...l.history.slice(0, 10), ...l.favourites]));
    // a hosted server's status changed, or a player joined or left (its console lines, as core's player_event
    // reads them): the count asked again at most every two seconds
    let soon = 0;
    const unlisten = listen<ServerEvent>("server", ({ payload: e }) => {
      if (host?.servers.find((c) => c.name === e.name)?.status !== e.status) refreshHost();
      else if (!soon && /" (fully connected|disconnected player)/.test(e.line?.text ?? "")) soon = window.setTimeout(() => ((soon = 0), refreshHost()), 2000);
    });
    return () => (clearTimeout(soon), unlisten.then((f) => f()));
  });

  /** A big update, a patch (a third number) or other news, from the build a post names. */
  const tagOf = (build: string) => (!build ? "news" : build.split(".").length > 2 ? "patch" : "major");
  /** The first Discord invite and the first other web page in a server's description. */
  const linksOf = (s: GameServer) => {
    const discord = s.description.match(/discord(?:\.gg|(?:app)?\.com\/invite)\/([\w-]+)/i)?.[1];
    const site = s.description.match(/https:\/\/(?!discord)[^\s<>"']+/i)?.[0];
    return { discord: discord ? `https://discord.gg/${discord}` : "", site: site ?? "" };
  };

  /** Project Zomboid and modding places; each checked to load. */
  const LINKS: { key: string; url: string; icon: IconName | "discord" | "pzmm" }[] = [
    { key: "website", url: "https://projectzomboid.com/blog/", icon: "external" },
    { key: "forums", url: "https://theindiestone.com/forums/", icon: "message" },
    { key: "discord", url: "https://discord.gg/theindiestone", icon: "discord" },
    { key: "wiki", url: "https://pzwiki.net/wiki/Project_Zomboid_Wiki", icon: "layers" },
    { key: "wikiModding", url: "https://pzwiki.net/wiki/Modding", icon: "wand" },
    { key: "moddingDiscord", url: "https://discord.gg/ghRTHUTc94", icon: "discord" },
    { key: "reddit", url: "https://www.reddit.com/r/projectzomboid/", icon: "share" },
    { key: "workshop", url: "https://steamcommunity.com/app/108600/workshop/", icon: "steam" },
    { key: "pzmm", url: "https://discord.com/invite/6URUy6mNzu", icon: "pzmm" },
  ];

  /** The play button's menu: a server's extras, switching profile, then options. */
  const menuItems = $derived.by((): MenuItem[] => {
    return [
      ...(server ? serverItems(server) : []),
      ...(card ? [
        { label: t("home.manage"), icon: "settings" as IconName, run: () => card && servers("", card.name) },
        ...(card.address ? [{ label: t("home.copyAddress"), icon: "copy" as IconName, run: () => card?.address && copy(card.address) }] : []),
        null,
      ] : []),
      { heading: t("home.switchProfile") },
      ...profiles.map((p) => ({ label: p, checked: sel.kind === "home" && p === view?.profile, disabled: p !== view?.profile && (busy || !!lock),
        run: () => (p === view?.profile ? pick({ kind: "home" }) : onUseProfile(p).then((ok) => ok && pick({ kind: "home" }))) })),
      null,
      { label: t("tabs.settings"), icon: "settings" as IconName, run: () => (showInSettings("launch"), onGo("settings")) },
    ];
  });
  function serverItems(s: GameServer): MenuItem[] {
    const l = linksOf(s);
    return [
      { label: t("launcher.joinAsIs"), icon: "play", run: () => joinServer(s, true), disabled: busy || !!working },
      { label: t("home.details"), icon: "settings", run: () => onOpenLink(connectOf(s)) },
      { label: t("launcher.favourite"), icon: "star", checked: isFavourite, run: () => favourite(s) },
      { label: t("home.copyAddress"), icon: "copy", run: () => copy(connectOf(s)) },
      { label: t("launcher.battlemetrics"), icon: "external", run: () => api.openUrl(`https://www.battlemetrics.com/servers/zomboid?q=${encodeURIComponent(connectOf(s).replace(/:\d+$/, ""))}`) },
      ...(l.discord ? [{ label: t("home.serverDiscord"), icon: "external" as IconName, run: () => api.openUrl(l.discord) }] : []),
      ...(l.site ? [{ label: t("home.serverSite"), icon: "external" as IconName, run: () => api.openUrl(l.site) }] : []),
      { label: t("home.copyLink"), icon: "copy", run: () => copy(`pzmm://join/${connectOf(s)}`) },
      null,
    ];
  }
  const openMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menuAt = menuAt ? null : { x: r.left, y: r.bottom + 4 };
  };
</script>

{#snippet dot(on: "up" | "down" | "asking" | "busy")}<span class="dot {on}" aria-hidden="true"></span>{/snippet}

{#snippet serverRow(s: GameServer)}
  {@const get = s.probed ? need(s).get : 0}
  <li>
    <button class="row" class:active={server?.addr === s.addr} class:dim={live[s.addr] && !s.probed} aria-pressed={server?.addr === s.addr} onmouseenter={() => warm(s)} onmouseleave={() => clearTimeout(warming)} onclick={() => pick(server?.addr === s.addr ? { kind: "home" } : { kind: "server", s })} title={server?.addr === s.addr ? t("home.deselect") : s.name}>
      {@render dot(!live[s.addr] ? "asking" : s.probed ? "up" : "down")}
      {#if s.password}<span class="rlock" role="img" title={t("launcher.locked")} aria-label={t("launcher.locked")}><Icon name="lock" size={12} /></span>{/if}
      <span class="rname">{s.name}</span>
      {#if get}<span class="rnote">{t("home.toGetShort", { count: get })}</span>{/if}
    </button>
  </li>
{/snippet}

{#snippet libtab(id: LibTab, label: string, count: number | null)}
  <button role="tab" class:active={shownTab === id} aria-selected={shownTab === id} tabindex={shownTab === id ? 0 : -1} onclick={() => (libTab = id)}>{label}{#if count !== null}<span class="count">{fmtNum(count)}</span>{/if}</button>
{/snippet}

{#snippet official()}<span class="tag ok"><Icon name="check" size={11} />{t("home.official")}</span>{/snippet}

{#snippet pill(tone: "ok" | "bad" | "warn" | "idle", icon: IconName, text: string, act: string, run: () => void, disabled = false)}
  <button class="health {tone}" onclick={run} {disabled}>
    <Icon name={icon} size={14} />
    <span>{text}</span>
    <span class="act">{act}</span>
  </button>
{/snippet}

{#snippet stat(label: string, value: string, tone: "" | "bad" | "warn" = "", tip = value, go?: () => void)}
  <div class="stat"><span class="slabel">{label}</span>{#if go}<button class="svalue lastplayed {tone}" onclick={go} title={tip}>{value}</button>{:else}<span class="svalue {tone}" title={tip}>{value}</span>{/if}</div>
{/snippet}

<svelte:window onkeydown={(e) => e.key === "Escape" && sel.kind !== "home" && !menuAt && !(e.target as HTMLElement).closest?.("input, textarea, [role=dialog], [role=alertdialog], [role=menu]") && pick({ kind: "home" })} />

<section class="home" aria-label={t("tabs.home")}>
  <div class="page blueprint">
    <!-- the hero: Project Zomboid's library art, its logo (or the server's name) over it -->
    <header class="hero" class:bare={artFailed}>
      {#if !artFailed}<img class="art" src={ART} alt="" onerror={() => (artFailed = true)} />{/if}
      <div class="mark">
        {#if view?.mode === "viewpoint" && !server && !card}
          <!-- i18n-ignore: the game's name -->
          <img class="logo glyph-white" src="/brand/project-viewpoint.png" alt="Project Viewpoint" />
        {:else if logoFailed}
          <!-- i18n-ignore: the game's name -->
          <span class="logo-text">Project Zomboid</span>
        {:else}
          <!-- i18n-ignore: the game's name -->
          <img class="logo" src={LOGO} alt="Project Zomboid" onerror={() => (logoFailed = true)} />
        {/if}
        <!-- what Play starts: the picked server, else the profile's own game -->
        <h2 class="title" title={server?.name ?? card?.name}>{server?.name ?? card?.name ?? t("home.singleplayer")}</h2>
      </div>
      <!-- the community's places, on a scrap of paper taped to the art -->
      <ul class="note" aria-label={t("home.community")}>
        <Scrap id="community-78">
          <h3 class="ntitle">{t("home.community")}</h3>
          <ul class="nlinks">
            {#each LINKS as l (l.key)}
              <li>
                <button class="nlink" onclick={() => api.openUrl(l.url)} title={l.url}>
                  {#if l.icon === "discord"}<img src="/brand/discord.png" alt="" width="15" height="15" class="inked" />
                  {:else if l.icon === "pzmm"}<img src="/brand/pzmm.png" alt="" width="15" height="15" class="inked" />
                  {:else}<Icon name={l.icon} size={15} />{/if}
                  <span>{t(`home.links.${l.key}`)}</span>
                </button>
              </li>
            {/each}
          </ul>
        </Scrap>
      </ul>
    </header>

    <!-- the play bar: the main button, then what matters at a glance -->
    <div class="playbar">
      {#if server}
        <div class="split">
          <button class="primary big" onclick={() => server && joinServer(server)} disabled={busy || !!working || (!!server.mod_count && !!lock)} title={lock || [server.password ? t("home.passwordTip") : "", server.mod_count ? t("home.joinTip") : ""].filter(Boolean).join("\n")}>
            <Icon name={server.password ? "lock" : "play"} size={18} />{t("launcher.join")}</button>
          <button class="primary caret" aria-haspopup="menu" aria-expanded={!!menuAt} aria-label={t("home.more")} onpointerdown={(e) => menuAt && e.stopPropagation()} onclick={openMenu}><Icon name="chevron-down" size={14} /></button>
        </div>
        <div class="stat"><span class="slabel">{t("home.stat.players")}</span>
          <span class="svalue">{#if server.probed}{server.players}/{server.max_players}<span class="bar" aria-hidden="true"><span style="width: {server.max_players ? Math.min(100, (server.players / server.max_players) * 100) : 0}%"></span></span>{:else}—{/if}</span></div>
        <!-- the server's state: ping, map and build, then whether you have its mods -->
        {@const n = need(server)}
        <div class="end">
          <div class="stats">
            {@render stat(t("home.stat.ping"), server.probed && server.ping !== null ? t("launcher.ms", { n: server.ping }) : live[server.addr] ? "—" : "…")}
            {@render stat(t("home.stat.map"), server.map.split(";")[0] || "—", "", server.map.replaceAll(";", ", "))}
            {@render stat(t("home.stat.build"), server.build || "—")}
          </div>
          {#if !live[server.addr]}<span class="line w30" aria-hidden="true"></span>
          {:else if !server.probed}{@render pill("bad", "alert", t("home.notAnswering"), t("home.details"), () => server && onOpenLink(connectOf(server)))}
          {:else if n.get}{@render pill("warn", "download", n.unlisted ? t("home.server.toGetUnlisted", { count: n.get, unlisted: n.unlisted }) : t("home.server.toGet", { count: n.get }), t("home.server.get"), () => server && getMods(server), busy || !!working || !!lock)}
          <!-- the mods it doesn't list are unknown until it's set up: not called ready -->
          {:else if n.unlisted}{@render pill("idle", "check", t("home.server.readyListed", { count: n.total - n.unlisted, unlisted: n.unlisted }), t("home.details"), () => server && onOpenLink(connectOf(server)))}
          {:else}{@render pill("ok", "check", n.total ? t("home.server.ready", { count: n.total }) : t("launcher.vanilla"), t("home.details"), () => server && onOpenLink(connectOf(server)))}{/if}
        </div>
      {:else if card}
        <div class="split">
          {#if card.remote && !(card.status === "running" && card.address)}
            <!-- run elsewhere and reached over RCON: where players join is unknown unless its address is set -->
            <button class="primary big" onclick={() => card && servers("", card.name)}><Icon name="settings" size={18} />{t("home.manage")}</button>
          {:else if card.status === "running"}
            <button class="primary big" onclick={async () => card && (card.remote ? onOpenLink(card.address) : (joinInServers(await ownJoin(card)), onGo("servers")))} title={t("hosting.joinTip")}><Icon name="play" size={18} />{t("hosting.join")}</button>
          {:else}
            <button class="primary big" onclick={() => card && start(card)} disabled={!!working || card.status !== "stopped" && card.status !== "crashed" || !!card.remote || !!card.external}><Icon name="play" size={18} />{t("hosting.start")}</button>
          {/if}
          <button class="primary caret" aria-haspopup="menu" aria-expanded={!!menuAt} aria-label={t("home.more")} onpointerdown={(e) => menuAt && e.stopPropagation()} onclick={openMenu}><Icon name="chevron-down" size={14} /></button>
        </div>
        <div class="stat"><span class="slabel">{t("home.stat.players")}</span><span class="svalue">{card.status === "running" && card.players !== null ? String(card.players) : "—"}</span></div>
        <!-- the hosted server's state: port and build, then whether it's running -->
        <div class="end">
          <div class="stats">
            {@render stat(t("home.stat.port"), card.remote ?? String(card.game_port))}
            {@render stat(t("home.stat.build"), card.build === "legacy41" ? "41" : "42")}
          </div>
          {@render pill(card.status === "running" ? "ok" : card.status === "crashed" ? "bad" : "idle", card.status === "running" ? "check" : card.status === "crashed" ? "alert" : "pulse",
            t(`hosting.status.${card.status}`), t("home.manage"), () => card && servers("", card.name))}
        </div>
      {:else}
        <div class="split">
          <button class="primary big" onclick={onLaunch} disabled={busy}><Icon name="play" size={18} />{t("home.playButton")}</button>
          <button class="primary caret" aria-haspopup="menu" aria-expanded={!!menuAt} aria-label={t("home.more")} onpointerdown={(e) => menuAt && e.stopPropagation()} onclick={openMenu}><Icon name="chevron-down" size={14} /></button>
        </div>
        <!-- the server you last played, a click away -->
        {@const last = recent[0]}
        {@const when = last && launcher?.joins[last.addr]?.[0]?.time}
        <!-- or the save played since: the game can't be told to open it, so Continue starts it as Play does -->
        {#if save && (!last || !when || save.modified > when)}
          <div class="stat">
            <span class="slabel">{t("home.stat.playedGame")}</span>
            <button class="svalue lastplayed" onclick={onLaunch} disabled={busy} title={t("home.continueTip", { save: save.name, ago: fmtAgo(save.modified) })}>{t("home.continue", { save: save.name })}</button>
          </div>
        {:else}
          <div class="stat">
            <span class="slabel">{t("home.stat.played")}</span>
            {#if last}
              <button class="svalue lastplayed" onclick={() => pick({ kind: "server", s: last })} title={when ? `${last.name} · ${fmtAgo(when)}` : last.name}>{last.name}</button>
            {:else if launcher}<button class="svalue lastplayed findsrv" onclick={() => servers("internet")}><Icon name="search" size={14} />{t("home.findServer")}</button>
            {:else}<span class="svalue">…</span>{/if}
          </div>
        {/if}
        <!-- the profile's state: its mods, then ZombieBuddy or the setup check -->
        <div class="end">
          <div class="stats">
            {@render stat(t("home.stat.mods"), view ? String(enabled) : "…", "", undefined, () => onGo("mods", "on"))}
            {@render stat(t("home.stat.problems"), view ? String(problems) : "…", problems > 0 ? "bad" : "", undefined, () => onGo("mods", "problems"))}
            {@render stat(t("home.stat.updates"), feed ? String(pending) : feedError ? "—" : "…", pending ? "warn" : "", undefined, () => onGo("feed"))}
          </div>
          {#if zbNeed}{@render pill("bad", "alert", zbNeed.partial ? t("home.zb.partial") : t("home.zb.missing", { count: zbNeed.count }), t("home.health.fix"), () => onGo("zb"))}
          {:else if checkFailed}{@render pill("bad", "alert", t("home.health.failed"), t("home.health.run"), onCheckSetup)}
          {:else if findings === null}{@render pill("idle", "refresh", t("app.checking"), "", onCheckSetup, true)}
          {:else}
            <!-- the setup check: what it found, and its action -->
            {@render pill(issues ? "bad" : "ok", issues ? "alert" : "check", issues ? t("home.health.problems", { count: issues }) : t("home.health.ok"), issues ? t("home.health.fix") : t("home.health.run"), onCheckSetup)}
          {/if}
        </div>
      {/if}
      {#if said}<span class="said" class:error={said.error} role="status">{said.text}</span>{/if}
    </div>
    {#if menuAt}<Menu x={menuAt.x} y={menuAt.y} label={t("home.more")} items={menuItems} onClose={() => (menuAt = null)} />{/if}

    <div class="content">
      <div class="col">
        <!-- featured: The Indie Stone's latest post -->
        <section aria-labelledby="home-featured">
          <h3 class="label from" id="home-featured">{t("home.featured")}{@render official()}</h3>
          {#if !news}
            <div class="card feature sk" aria-hidden="true"><span class="fimg"></span><span class="ftext"><span class="line w30"></span><span class="line w60 tall"></span><span class="line w60"></span></span></div>
          {:else if !posts.length}
            <p class="none card">{news.error ? t("home.newsFailed") : t("home.noNews")}</p>
          {:else}
            {@const p = posts[0]}
            <button class="card feature" onclick={() => api.openUrl(p.url)}>
              <!-- offline: the card's own grey -->
              <span class="fimg">{#if !featFailed}<img src={p.image || ART} alt="" loading="lazy" onerror={() => (featFailed = true)} />{/if}</span>
              <span class="ftext">
                <span class="fmeta"><span class="tag">{t(`home.tag.${tagOf(p.build)}`)}</span><span>{fmtDate(p.date, "long")}</span>
                  {#if p.build && played && p.date > played}<span class="tag warn" title={t("home.mayAffectTip")}>{t("home.mayAffect")}</span>{/if}</span>
                <span class="ftitle">{p.title}</span>
                <span class="fex">{p.excerpt}</span>
              </span>
            </button>
          {/if}
        </section>

        {#if !server && !card && posts.length > 1}
          <section class="card block" aria-labelledby="home-news">
            <h3 class="label from" id="home-news">{t("home.news")}{@render official()}</h3>
            <ul class="updates">
              {#each posts.slice(1, 5) as p (p.url)}
                <li><button class="upd" onclick={() => api.openUrl(p.url)} title={p.excerpt}><span class="lname">{p.title}</span><span class="muted when">{fmtDate(p.date, "short")}</span></button></li>
              {/each}
            </ul>
          </section>
        {/if}

        <!-- activity: your mods updated since you last played -->
        <section class="card block" aria-labelledby="home-activity">
          <header class="bhead">
            <h3 class="label" id="home-activity">{t("home.activity")}</h3>
            <button class="quiet" onclick={() => onGo("feed")}>{t("tabs.feed")}<Icon name="chevron-right" size={14} /></button>
          </header>
          {#if feedError}<p class="none">{t("home.updatesFailed")}</p>
          {:else if !feed}<span class="line w60" aria-hidden="true"></span><span class="line w30" aria-hidden="true"></span>
          {:else if !feed.entries.length}<p class="none">{t("home.noneUpdated")}</p>
          {:else}
            <!-- each mod with its Workshop image, its new version and when; a click reads its notes in What's new -->
            <ul class="updates">
              {#each feed.entries.slice(0, 5) as e (e.item)}
                {@const name = e.title || e.item}
                <li>
                  <button class="upd" onclick={() => onGo("feed")} title={name}>
                    {#if previews[e.item]}<img use:thumb={t("common.imageFailed")} class="thumb" src={previews[e.item]} alt="" loading="lazy" />
                    {:else}<span class="thumb blank" aria-hidden="true">{name.charAt(0)}</span>{/if}
                    <span class="utext">
                      <span class="uname">{name}</span>
                      <span class="muted usub">{#if e.from}<span class="mono">{e.from} → {e.to}</span>{" · "}{/if}{fmtAgo(e.updated)}</span>
                    </span>
                    {#if e.risks.length}<span class="tag danger" title={e.risks.map((r) => t(`feed.risk.${r}`)).join("\n")}>{t("feed.mayBreak")}</span>{/if}
                  </button>
                  {#if e.pending}<button class="primary" onclick={() => onUpdate(e.mods).then(() => glance().then((f) => (feed = f)))} disabled={busy}><Icon name="refresh" size={13} />{t("feed.update")}</button>{/if}
                </li>
              {/each}
            </ul>
            {#if feed.entries.length > 5}<button class="link more" onclick={() => onGo("feed")}>{t("home.andMore", { count: feed.entries.length - 5 })}</button>{/if}
          {/if}
        </section>
      </div>

      <div class="col">
        <!-- the library: the servers you know and the busiest; its heading level with Featured's -->
        <section class="libwrap" aria-labelledby="home-pick">
        <h3 class="label" id="home-pick">{t("home.pickServer")}</h3>
        <nav class="card lib" aria-label={t("home.library")}>
          <div class="find">
            <Icon name="search" size={14} />
            <input type="search" bind:value={search} placeholder={t("home.search")} aria-label={t("home.search")} />
          </div>

          <div class="ltabs" role="tablist" aria-label={t("home.library")} tabindex="-1"
            onkeydown={(e) => arrowPick(e, ["popular", "favourites", "recent", ...(hosted.length ? ["hosted"] : [])], shownTab, (v) => (libTab = v as LibTab))}>
            {@render libtab("popular", t("home.groups.popular"), popular ? popular.list.filter((s) => match(s.name)).length : null)}
            {@render libtab("favourites", t("home.groups.favourites"), launcher ? favourites.filter((s) => match(s.name)).length : null)}
            {@render libtab("recent", t("home.groups.recent"), launcher ? recent.filter((s) => match(s.name)).length : null)}
            {#if hosted.length}{@render libtab("hosted", t("home.groups.hosted"), hosted.filter((c) => match(c.name)).length)}{/if}
          </div>
          <!-- the tab's list fills the card, down to where the page's left column ends -->
          <ul class="llist">
            {#if popularLoading && shownTab === "popular"}
              <li class="none fetching" role="status">{t("launcher.listing")}<span class="bar wide" aria-hidden="true"><span></span></span></li>
            {/if}
            {#if shownTab === "favourites"}
              {#each favourites.filter((s) => match(s.name)) as s (s.addr)}{@render serverRow(s)}{/each}
              {#if launcher && !favourites.length}<li class="none">{t("home.none.favourites")}</li>{/if}
            {:else if shownTab === "recent"}
              {#each recent.filter((s) => match(s.name)) as s (s.addr)}{@render serverRow(s)}{/each}
              {#if launcher && !recent.length}<li class="none">{t("launcher.empty.history")}</li>{/if}
            {:else if shownTab === "hosted"}
              {#each hosted.filter((c) => match(c.name)) as c (c.name)}
                <li>
                  <button class="row" class:active={card?.name === c.name} class:dim={c.status !== "running"} aria-pressed={card?.name === c.name} onclick={() => pick(card?.name === c.name ? { kind: "home" } : { kind: "hosted", name: c.name })} title={card?.name === c.name ? t("home.deselect") : c.name}>
                    {@render dot(c.status === "running" ? "up" : c.status === "starting" || c.status === "stopping" ? "busy" : "down")}<span class="rname">{c.name}</span>
                    {#if c.status === "running" && c.players}<span class="rcount">{c.players}</span>{/if}
                  </button>
                </li>
              {/each}
            {:else if !popular}
              {#each [0, 1, 2] as i (i)}<li class="row sk" aria-hidden="true"><span class="line w60"></span></li>{/each}
            {:else}
              {#each popular.list.map(liveOf).filter((s) => match(s.name)) as s (s.addr)}{@render serverRow(s)}{/each}
              {#if popular.error}<li class="none" title={popular.error}>{t("home.popularFailed")} <button class="link" onclick={() => loadPopular(true)} disabled={popularLoading}>{t("common.tryAgain")}</button></li>
              {:else if !popular.list.length}<li class="none">{t(popular.steam ? "home.none.popular" : "home.none.game")}</li>{/if}
              {#if popular.error}<!-- its Try again is the refresh -->
              {:else if !popular.steam && !WINDOWS}
                <!-- elsewhere only the Steam Web API key lists Steam's servers -->
                <li class="none">{t("home.gameListKey")} <button class="link" onclick={() => (showInSettings("keys"), onGo("settings"))}>{t("launcher.addKey")}</button></li>
              {:else if !popular.steam}<li class="none">{t("home.gameList")} <button class="link" onclick={() => loadPopular(true)} disabled={popularLoading}>{t("common.tryAgain")}</button></li>{/if}
            {/if}
          </ul>

          <div class="side-foot">
            <button class="quiet" onclick={() => servers("internet")}><Icon name="search" size={14} />{t("servers.rail.browse")}</button>
          </div>
        </nav>
        </section>
        {#if server || card}
          <!-- who's on, its mods and its settings: the server's page in Servers -->
          <button class="card info" onclick={() => (server ? onOpenLink(connectOf(server)) : card && servers("", card.name))}>
            <Icon name="server" size={18} />
            <span class="itext"><span class="iname">{t("home.serverInfo")}</span><span class="muted">{t("home.serverInfoSub")}</span></span>
            <Icon name="chevron-right" size={16} />
          </button>
        {:else}
          {#if view && !enabled}<p class="none card">{t("home.firstMods")} <button class="link" onclick={() => onGo("workshop")}>{t("tabs.workshop")}</button></p>{/if}
        {/if}

      </div>
    </div>
  </div>
</section>

<style>
  /* the tab edge to edge, the hero full width */
  .home {
    display: flex;
    min-height: 0;
    background: var(--bg);
  }
  .libwrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .lib {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 6px 0 0;
  }
  /* no height of its own: the list grows into what the left column leaves, and scrolls */
  .lib .llist {
    flex: 1 1 auto;
    height: 0;
    min-height: 240px;
    overflow-y: auto;
    /* reaches 6px past the card's left edge: the chevron on that edge isn't cut off by the scrolling */
    margin: 0 0 0 -6px;
    padding: 6px 0 6px 6px;
    list-style: none;
  }
  .lib ul {
    list-style: none;
    margin: 0 0 6px;
    padding: 0;
  }
  /* inset from the card's sides, so the highlight is rounded like the profile menu's */
  .row {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 8px;
    width: calc(100% - 16px);
    margin: 0 8px;
    padding: 5px 10px 5px 14px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    text-align: left;
  }
  .row:not(.sk):hover:not(:disabled),
  .row.active {
    color: var(--on-selected);
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  /* the hovered or selected row: My mods' chevron, centred on the card's left edge (the row starts 8px in) */
  .row:not(.sk):hover:not(:disabled)::before,
  .row.active::before {
    content: "";
    position: absolute;
    left: -13px;
    top: 50%;
    width: 10px;
    height: 14px;
    margin-top: -7px;
    background: var(--on-selected);
    clip-path: polygon(0 0, 45% 0, 100% 50%, 45% 100%, 0 100%, 55% 50%);
  }
  .row.dim .rname {
    opacity: 0.55;
  }
  .rname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rlock {
    flex: none;
    display: flex;
    color: var(--muted);
  }
  .rnote {
    flex: none;
    font-size: 12px;
    color: var(--warn);
  }
  .rcount {
    padding: 0 6px;
    border-radius: 8px;
    background: var(--raised);
    font-size: 11px;
    font-weight: 700;
  }
  .find {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px 14px 8px;
    color: var(--muted);
  }
  .find input {
    flex: 1;
  }
  .ltabs {
    display: flex;
    flex-wrap: wrap;
    gap: 0 2px;
    margin: 0 14px;
    border-bottom: 1px solid var(--line);
  }
  /* the app's tabs (the bar along the top), smaller */
  .ltabs button {
    gap: 6px;
    padding: 7px 10px 6px;
    border: 0;
    border-bottom: 2px solid transparent;
    border-radius: 4px 4px 0 0; /* the highlight rounded on top, square on the underline */
    margin-bottom: -1px;
    background: transparent;
    color: var(--muted);
  }
  .ltabs button:hover:not(:disabled),
  .ltabs button.active {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-bottom-color: var(--on-selected);
    box-shadow: inset 0 1px 0 var(--hover-line), inset 1px 0 0 var(--hover-line), inset -1px 0 0 var(--hover-line);
  }
  .count {
    opacity: 0.7;
  }
  .lib .none {
    padding: 2px 14px 6px 22px;
    font-size: 12px;
    color: var(--muted);
  }
  .side-foot {
    margin-top: auto;
    padding: 8px 6px;
    border-top: 1px solid var(--line);
  }
  .side-foot button {
    width: 100%;
    justify-content: flex-start;
  }

  .page {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    container-type: inline-size;
  }

  /* the hero: the game's library art, fading into the play bar */
  .hero {
    position: relative;
    /* tall enough for the community note; in a short window, short enough that Play shows */
    height: min(clamp(350px, 27vw, 400px), max(180px, 100vh - 340px));
    overflow: hidden;
    background: var(--bg);
  }
  .hero::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, transparent 45%, rgb(0 0 0 / 0.75));
    pointer-events: none;
  }
  .art {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 30%;
  }
  .mark {
    position: absolute;
    z-index: 1;
    left: 32px;
    right: 376px;
    bottom: 22px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }
  .logo {
    display: block;
    width: min(380px, 50%);
    height: auto;
    filter: drop-shadow(0 2px 8px rgb(0 0 0 / 0.6));
  }
  .glyph-white {
    filter: drop-shadow(0 2px 8px rgb(0 0 0 / 0.6));
  }
  .logo-text,
  .title {
    margin: 0;
    max-width: 100%;
    font-family: var(--font-marker);
    font-weight: 400;
    font-size: clamp(26px, 3.2vw, 40px);
    line-height: 1.15;
    color: #fff;
    text-shadow: 0 2px 10px rgb(0 0 0 / 0.8);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note {
    position: absolute;
    z-index: 1;
    top: 28px;
    right: 40px;
    width: 304px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  /* a hero cut short: the note scrolls within it */
  @media (max-height: 700px) {
    .note {
      top: 12px;
      bottom: 12px;
      overflow-y: auto;
    }
  }
  /* too short for all its links: the paper fades out at the bottom, so it reads as scrolling (the space
     under the last link lets it scroll clear of the fade) */
  @media (max-height: 660px) {
    .note {
      padding-bottom: 28px;
      mask-image: linear-gradient(to bottom, #000 calc(100% - 32px), transparent);
    }
  }
  .ntitle {
    position: relative;
    margin: 0 0 2px;
    font-family: var(--font-marker);
    font-weight: 400;
    font-size: 22px;
    color: var(--ink);
  }
  .nlinks {
    position: relative;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .nlink {
    all: unset;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    box-sizing: border-box;
    padding: 2px 0;
    font-size: 13px;
    color: var(--ink);
    cursor: pointer;
  }
  .nlink:hover,
  .nlink:focus-visible {
    color: var(--blood);
    text-decoration: underline;
  }
  .nlink:focus-visible {
    outline: 2px dashed var(--blood);
    outline-offset: 2px;
  }
  /* the white logos in the paper's ink */
  .inked {
    filter: brightness(0) opacity(0.85);
  }
  /* offline: the blueprint with the name on it */
  .hero.bare {
    background-color: #121212;
    background-image:
      linear-gradient(rgb(255 255 255 / 0.09) 1px, transparent 1px),
      linear-gradient(90deg, rgb(255 255 255 / 0.09) 1px, transparent 1px),
      linear-gradient(rgb(255 255 255 / 0.035) 1px, transparent 1px),
      linear-gradient(90deg, rgb(255 255 255 / 0.035) 1px, transparent 1px);
    background-size: 96px 96px, 96px 96px, 24px 24px, 24px 24px;
  }

  /* the play bar: the main button, stats in small caps, icon buttons at the end */
  .playbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 14px 28px;
    padding: 16px 32px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .split {
    display: flex;
  }
  .big {
    padding: 11px 30px;
    font-size: 17px;
    letter-spacing: 0.02em;
  }
  .split .big {
    border-radius: 6px 0 0 6px;
  }
  .caret {
    padding: 0 10px;
    border-radius: 0 6px 6px 0;
    /* a gap in the bar's own colour, not a light line */
    border-left: 1px solid var(--panel);
  }
  .caret:hover:not(:disabled) {
    border-left-color: var(--panel);
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 28px;
    min-width: 0;
  }
  .stat {
    display: flex;
    flex-direction: column;
    min-width: 0;
    max-width: 220px;
  }
  .slabel,
  .label {
    margin: 0;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .svalue {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    flex: none;
    width: 64px;
    height: 5px;
    border-radius: 3px;
    background: color-mix(in oklab, var(--text) 14%, transparent);
    overflow: hidden;
  }
  .bar > span {
    display: block;
    height: 100%;
    background: var(--accent-text);
  }
  /* the server list being fetched: no count to show, so a segment sweeps across */
  .bar.wide {
    display: block;
    width: 100%;
    margin-top: 6px;
  }
  .bar.wide > span {
    width: 35%;
    animation: sweep 1.2s ease-in-out infinite;
  }
  @keyframes sweep {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(290%);
    }
  }

  .said {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }

  /* the content: two thirds and one third, their tops in line */
  .content {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
    gap: 20px;
    padding: 24px 32px 40px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 20px;
    min-width: 0;
  }
  .col > section > .label {
    margin-bottom: 8px;
  }
  .card {
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
    min-width: 0;
  }
  .block {
    padding: 14px 18px 16px;
  }
  .block > .label {
    margin-bottom: 8px;
  }
  .bhead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: -4px 0 4px;
  }
  .feature {
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 6fr);
    width: 100%;
    padding: 0;
    overflow: hidden;
    border: 0;
    text-align: left;
    white-space: normal;
    align-items: stretch;
  }
  .feature:hover:not(:disabled) {
    background: var(--panel);
    border: 0;
    color: inherit;
    outline: 1px solid var(--hover-line);
    outline-offset: -1px;
  }
  .fimg {
    position: relative;
    min-height: 190px;
    background: var(--raised);
  }
  .fimg img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .ftext {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 18px 22px;
    min-width: 0;
  }
  .fmeta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    font-size: 12px;
    color: var(--muted);
  }
  .tag {
    padding: 0 6px;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    line-height: 18px;
    color: var(--on-accent);
    background: var(--accent);
  }
  /* the posts are The Indie Stone's own */
  .from {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tag.ok {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--ok);
    background: transparent;
    border: 1px solid color-mix(in oklab, var(--ok) 50%, transparent);
  }
  .tag.warn {
    color: var(--warn);
    background: transparent;
    border: 1px solid color-mix(in oklab, var(--warn) 50%, transparent);
  }
  .tag.danger {
    color: var(--danger);
    background: transparent;
    border: 1px solid color-mix(in oklab, var(--danger) 45%, transparent);
  }
  .ftitle {
    font-size: 19px;
    font-weight: 700;
    line-height: 1.25;
  }
  .fex {
    color: var(--muted);
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .lname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* the updated mods: a Workshop image, the name over its version and time */
  .updates {
    list-style: none;
    margin: 0 -10px;
    padding: 0;
  }
  .updates li:has(> .primary) {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .upd {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 6px 10px;
    border-radius: 4px;
    cursor: pointer;
  }
  .upd:hover,
  .upd:focus-visible {
    background: var(--hover);
    color: var(--on-selected);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .upd:hover .muted,
  .upd:focus-visible .muted {
    color: color-mix(in oklab, var(--on-selected) 72%, transparent);
  }
  .upd:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .thumb {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 6px;
    object-fit: cover;
    background: var(--raised);
  }
  .thumb.blank {
    display: grid;
    place-items: center;
    font-family: var(--font-marker);
    font-size: 20px;
    color: var(--muted);
    border: 1px solid var(--line);
    box-sizing: border-box;
  }
  .utext {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .uname,
  .usub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .uname {
    font-weight: 600;
  }
  .usub {
    font-size: 12px;
  }
  .when {
    flex: none;
    font-size: 12px;
  }
  .more {
    margin-top: 8px;
  }
  .end {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px 28px;
    margin-left: auto;
  }
  .health {
    --tone: var(--ok);
    gap: 8px;
    padding: 0 0 0 12px;
    min-height: 32px;
    border: 1px solid color-mix(in oklab, var(--tone) 45%, transparent);
    border-radius: 16px;
    background: color-mix(in oklab, var(--tone) 8%, transparent);
    color: var(--tone);
    overflow: hidden;
  }
  .health.bad {
    --tone: var(--danger);
  }
  .health.warn {
    --tone: var(--warn);
  }
  .health.idle {
    --tone: var(--muted);
  }
  /* no action (Checking…): the text keeps its own right edge */
  .health .act:empty {
    display: none;
  }
  .health:has(.act:empty) {
    padding-right: 14px;
  }
  .health:hover:not(:disabled) {
    border-color: var(--tone);
    background: color-mix(in oklab, var(--tone) 16%, transparent);
    color: var(--tone);
  }
  .act {
    align-self: stretch;
    display: flex;
    align-items: center;
    padding: 0 14px 0 10px;
    border-left: 1px solid color-mix(in oklab, var(--tone) 45%, transparent);
    font-weight: 700;
  }
  .health:disabled {
    opacity: 0.6;
  }
  /* the panel's colour on the filled action: dark ink on Charcoal's orange and red, light on Daylight's */
  .health.bad .act,
  .health.warn .act {
    color: var(--panel);
    background: var(--tone);
  }
  .lastplayed {
    all: unset;
    display: block;
    max-width: 220px;
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .lastplayed:hover,
  .lastplayed:focus-visible {
    text-decoration: underline;
  }
  .lastplayed:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 2px;
  }
  /* nothing played yet: the way to find a server */
  .findsrv {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--accent-text);
  }
  /* the way to the server's full page */
  .info {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 14px;
    width: 100%;
    padding: 14px 18px;
    border: 0;
    text-align: left;
    white-space: normal;
  }
  .info:hover:not(:disabled) {
    background: var(--panel);
    border: 0;
    color: inherit;
    box-shadow: var(--card-shadow), inset 0 0 0 1px var(--hover-line);
  }
  .itext {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .iname {
    font-weight: 700;
  }
  .none {
    margin: 0;
    color: var(--muted);
  }
  p.card.none {
    padding: 14px 18px;
  }
  .muted {
    color: var(--muted);
  }
  .svalue.bad {
    color: var(--danger);
  }
  .svalue.warn {
    color: var(--warn);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
  }
  .dot.up {
    background: var(--ok);
  }
  .dot.busy {
    background: var(--warn);
  }
  .dot.down {
    background: transparent;
    border: 1px solid var(--muted);
  }
  .dot.asking {
    animation: blink 1.2s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }

  /* loading: grey bars where the text will be */
  .line {
    display: block;
    height: 10px;
    margin: 6px 0;
    border-radius: 5px;
    background: color-mix(in oklab, var(--text) 9%, transparent);
  }
  .line.tall {
    height: 18px;
  }
  .w60 {
    width: 60%;
  }
  .w30 {
    width: 30%;
  }
  .sk.row {
    padding: 6px 14px 6px 22px;
  }

  @container (max-width: 820px) {
    .lib .llist {
      height: auto;
      max-height: 420px;
    }
    .content {
      grid-template-columns: minmax(0, 1fr);
      padding: 20px 16px 32px;
    }
    .playbar {
      padding: 14px 16px;
    }
  }
  @container (max-width: 560px) {
    .feature {
      grid-template-columns: minmax(0, 1fr);
    }
    .fimg {
      min-height: 0;
      aspect-ratio: 16 / 7;
    }
  }
</style>
