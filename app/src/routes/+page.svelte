<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getVersion } from "@tauri-apps/api/app";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { decodeShare, describeSource, encodeShare, isOurs, removable, WINDOWS, type HistoryRecord, type ModDetails, type Snapshot } from "$lib/api";
  import { arrowPick, isTyping, trap } from "$lib/ui";
  import { api, modeOf, VIEWPOINT, zbReady, type Mode, type ModRow, type NotifyKind, type Row, type View, type ZbStatus } from "$lib/api";
  import Details from "$lib/Details.svelte";
  import Workshop from "$lib/Workshop.svelte";
  import ZombieBuddy from "$lib/ZombieBuddy.svelte";
  import Saves from "$lib/Saves.svelte";
  import Servers, { inServerWizard } from "$lib/Servers.svelte";
  import Maps from "$lib/Maps.svelte";
  import Links from "$lib/Links.svelte";
  import Icon from "$lib/Icon.svelte";
  import ActivityLog from "$lib/ActivityLog.svelte";
  import Feed, { forgetFeed } from "$lib/Feed.svelte";
  import Home, { forgetGlance, glance } from "$lib/Home.svelte";
  import Authoring from "$lib/Authoring.svelte";
  import Menu, { type MenuItem } from "$lib/Menu.svelte";
  import type { IconName } from "$lib/Icon.svelte";
  import { answer, ask, askChecked, asking, confirmed, type Choice } from "$lib/ask.svelte";
  import { background, classify, type LogEntry } from "$lib/activity.svelte";
  import WhatsNew, { NOTES_FOR } from "$lib/WhatsNew.svelte";
  import ActivityModal from "$lib/ActivityModal.svelte";
  import DownloadHistory from "$lib/DownloadHistory.svelte";
  import { fmtDateTime, fmtNum, fmtTime, modChanges, workshopUrl } from "$lib/format";
  import Findings from "$lib/Findings.svelte";
  import CheckSetup from "$lib/CheckSetup.svelte";
  import GameLog from "$lib/GameLog.svelte";
  import { groups, isServer, remedies, severity, type Fixes, type Group, type Remedy } from "$lib/findings";
  import CollectionImport from "$lib/CollectionImport.svelte";
  import Settings from "$lib/Settings.svelte";
  import { applyPrefs, prefs } from "$lib/prefs.svelte";
  import ModTable from "$lib/ModTable.svelte";
  import { autosort, bisectRound, dependants, findProblems, groupByCategory, loadList, moveSection, neededBy, orderOf, orphanedRequirements, parsePreset, placeInOrder, placeInSections, sortByName, sortLoadOrder, syncOrder, tiers } from "$lib/order";
  import LoadOrder from "$lib/LoadOrder.svelte";
  import WindowControls from "$lib/WindowControls.svelte";
  import ProfilePicker from "$lib/ProfilePicker.svelte";
  import ServerSetup from "$lib/ServerSetup.svelte";
  import type { Categories, FeedEntry, Finding, GameRequest, GameServer, LogReport, Marks, Overlap, Pack, Preset, Save, SettingsView, SetupItem, Stats } from "$lib/api";
  import { intlLang, keyedError, t, th, type Vars } from "$lib/i18n.svelte";
  /** Names in a sentence, separated the way the player's language does ("A, B, C"; "A、B、C"). */
  const listed = (items: string[]) => new Intl.ListFormat(intlLang(), { type: "unit" }).format(items);

  let tab = $state<"home" | "mods" | "feed" | "workshop" | "maps" | "servers" | "saves" | "zb" | "authoring" | "settings">("home");
  /** Workshop mods updated since you last played (the What's new tab's count). */
  let feedCount = $state(0);
  /** What's new's count: updates not seen in the tab yet (an entry seen counts again once its mod updates again),
   * here or in the game's What's new (`seen`: the newest update read there). */
  const FEED_SEEN = "pzmm.feedSeen";
  const feedKey = (e: FeedEntry) => `${e.item}:${e.updated}`;
  function feedUnseen({ entries, seen: inGame }: { entries: FeedEntry[]; seen: number }) {
    let seen: string[] = [];
    try {
      seen = JSON.parse(localStorage.getItem(FEED_SEEN) ?? "[]");
    } catch {}
    return entries.filter((e) => e.updated > inGame && !seen.includes(feedKey(e))).length;
  }
  function feedSeen(entries: FeedEntry[]) {
    feedCount = 0;
    if (entries.length) api.feedSeen(Math.max(...entries.map((e) => e.updated))).catch(() => {});
    try {
      localStorage.setItem(FEED_SEEN, JSON.stringify(entries.map(feedKey)));
    } catch {}
  }
  /** A pzmm://join link to show in the Servers tab's details (it asks before joining). */
  let joinLink = $state<{ url: string } | null>(null);
  const openLink = (url: string) => {
    // pzmm://game: the game started PZMM for its requests (they say what to open)
    if (url.startsWith("pzmm://game")) return void gameAsked();
    tab = "servers";
    joinLink = { url };
  };
  /** The game asked something (PZMM came to the front): act on it once nothing else is running. */
  function gameAsked() {
    if (busy) setTimeout(gameAsked, 2000);
    else fromGame();
  }
  let updates = $state<Set<string>>(new Set());
  /** settings.json, folders and the rest of the Settings tab (null until read). */
  let cfg = $state<SettingsView | null>(null);
  $effect(applyPrefs);
  // the in-game manager matches the app's accent and theme (app.txt)
  let systemDark = $state(typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  $effect(() => {
    const q = window.matchMedia("(prefers-color-scheme: dark)");
    const follow = () => (systemDark = q.matches);
    q.addEventListener("change", follow);
    return () => q.removeEventListener("change", follow);
  });
  $effect(() => {
    const dark = prefs.theme === "dark" || (prefs.theme === "system" && systemDark);
    api.gameLook(prefs.accent, dark ? "dark" : "light").catch(() => {});
  });

  // shared files between enabled mods: who overrides whom (refreshed after every change)
  let overlaps = $state<Record<string, Overlap>>({});
  /** The load order the overlaps were asked for, and the latest ask (an older reply that lands later is dropped). */
  let overlapsFor = "";
  let overlapsAsk = 0;
  /** Ask again when the load order changed, or `files` may have (installs, updates, removals). */
  function refreshOverlaps(files = false) {
    const key = order.join("\n");
    if (!files && key === overlapsFor) return;
    overlapsFor = key;
    const n = ++overlapsAsk;
    api.conflictSummary().then((o) => n === overlapsAsk && (overlaps = o), () => n === overlapsAsk && (overlapsFor = ""));
  }

  // --- PZMM's own updates (signed releases on GitHub) ---
  let appVersion = $state("");
  let appUpdate = $state.raw<Update | null>(null);
  /** The version just updated to, while its patch notes show. */
  let whatsNew = $state("");
  /** After an update (not a first install), show what changed once. */
  async function whatsNewCheck(firstRun: boolean) {
    const key = "pzmm.lastVersion";
    const now = await getVersion();
    let last: string | null = null;
    try {
      last = localStorage.getItem(key);
      localStorage.setItem(key, now);
    } catch {}
    // no version noted: a first install, or an update from before PZMM noted it (it had made a profile by then);
    // and only with notes written for this version (a release that didn't update them shows none, not old ones)
    if ((last ? last !== now : !firstRun) && now === NOTES_FOR) whatsNew = now;
  }
  let appProgress = $state<number | null>(null); // 0..100 while downloading

  async function checkAppUpdate(manual = false) {
    try {
      appUpdate = await check();
      if (!manual) return;
      appUpdateLater = ""; // asked for: show it again even after Later
      if (appUpdate) say("log.appUpdateFound", { version: appUpdate.version });
      else say("log.appLatest", { version: appVersion });
    } catch (e) {
      if (manual) say("log.appCheckFailed", { error: t(...keyedError(e)) }, true);
    }
  }

  async function installAppUpdate() {
    if (!appUpdate) return;
    let total = 0;
    let got = 0;
    appProgress = 0;
    try {
      await appUpdate.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          appProgress = total ? Math.round((got / total) * 100) : null;
        } else appProgress = 100;
      });
      await relaunch();
    } catch (e) {
      say("log.appUpdateFailed", { error: t(...keyedError(e)) }, true);
      appProgress = null;
    }
  }
  /** The version Later was picked for: a newer one shows again. */
  let appUpdateLater = $state("");
  // asked again every six hours too, for a PZMM left open for days
  $effect(() => {
    if (cfg?.settings.app_updates === false) return;
    const timer = setInterval(checkAppUpdate, 6 * 3_600_000);
    return () => clearInterval(timer);
  });
  /** The profile the Servers tab is drawn for: a switch draws it afresh, but not under the new server wizard
   * (what's filled in, or a server install running, would go); leaving the wizard catches up. */
  let serversFor = $state("");
  $effect(() => {
    const p = view?.profile ?? "";
    if (!inServerWizard()) serversFor = p;
  });
  let workshopSeen = $state(false);
  let serversSeen = $state(false);
  $effect(() => {
    if (tab === "workshop") workshopSeen = true;
    if (tab === "servers") serversSeen = true;
  });
  let zb = $state<ZbStatus | null>(null);
  /** The Workshop tab is waiting on Steam: the activity bar says so. */
  let workshopLoading = $state(false);
  /** A folder set by hand (or by an environment variable) that isn't there or isn't right. */
  const folderTrouble = $derived(!!cfg?.folders.some((f) => (f.custom || f.overridden) && f.status !== "found"));
  const zbNeeded = $derived(!!zb && zb.required_by.length > 0 && !zbReady(zb));
  const refreshZb = () => api.zbStatus().then((s) => (zb = s)).catch(() => {});

  let view = $state<View | null>(null);
  /** The library (left list): its order and sections are the player's organisation only. */
  let rows = $state<Row[]>([]);
  /** The load order: enabled mod ids, as the game loads them (the side panel's list). */
  let order = $state<string[]>([]);
  /** Workshop categories, once fetched: Enable uses them to place mods like Autosort would. */
  let cats = $state<Categories | null>(null);

  // --- Project Zomboid / Project Viewpoint ---
  let modePlan = $state<{ target: Mode; profile?: string; lines: string[]; error: string } | null>(null);
  const modeProfiles = $derived(
    (view?.profiles ?? []).filter((p) => p === view?.profile || modeOf(view?.kinds[p]) === (view?.mode ?? "zomboid")),
  );

  /** Show what switching game does; `profile` is the one picked in the profile menu, if any. */
  async function planMode(target: Mode, profile?: string) {
    if (!view || target === view.mode) return;
    modePlan = { target, profile, lines: [], error: "" };
    try {
      const lines = await api.modePreview(target);
      if (modePlan?.target === target) modePlan.lines = lines;
    } catch (e) {
      if (modePlan?.target === target) modePlan.error = t(...keyedError(e));
      else say(...keyedError(e), true);
    }
  }

  async function switchMode() {
    if (!modePlan) return;
    const { target, profile } = modePlan;
    modePlan = null;
    const label = t(target === "viewpoint" ? "busy.viewpointSetup" : "busy.toZomboid");
    if (!(await run(label, () => api.modeSwitch(target), { showLog: true }))) return;
    // the switch lands on that game's last profile; go on to the one that was picked
    if (profile && view && profile !== view.profile && view.profiles.includes(profile)) await run(t("busy.switchingTo", { name: profile }), () => api.profileUse(profile));
  }
  let selected = $state<string | null>(null);
  /** Runs in flight: busy until every one has finished. */
  let pending = $state(0);
  const busy = $derived(pending > 0);
  let busyLabel = $state("");
  /** `time`: when it was said (ms); `url`: a page it links to (an update's change notes). */
  let log = $state<LogEntry[]>([]);
  /** Workshop items a download has finished, of how many, while it runs (Cancel stops it). */
  let progress = $state<[number, number] | null>(null);
  let logOpen = $state(false);
  /** The activity's large views: every line with what it's about, or the download history. */
  let activityView = $state<"" | "expand" | "history">("");
  // search, filters and sort start as they were left (kept in prefs)
  let search = $state(prefs.modsSearch);
  type Filter = "all" | "on" | "off" | "fav" | "hidden" | "problems";
  let filter = $state<Filter>((["all", "on", "off", "fav", "hidden", "problems"].includes(prefs.modsFilter) ? prefs.modsFilter : "all") as Filter);
  /** Only mods of this kind (framework, patch, map, ui, translation; "" any). */
  let kind = $state(prefs.modsKind);
  /** Only mods with this tag of the player's ("" any). */
  let tag = $state(prefs.modsTag);
  /** Only mods with texts in the game's language. */
  let inLanguage = $state(prefs.modsLang);
  /** Presets, favourites and hidden mods the game's mod menu and the Workshop mod managers keep (shared with them),
   * and the player's own tags (PZMM's). */
  let marks = $state<Marks>({ presets: [], favourites: [], hidden: [], tags: {} });
  const favourites = $derived(new Set(marks.favourites));
  const hidden = $derived(new Set(marks.hidden));
  const allTags = $derived([...new Set(Object.values(marks.tags).flat())].sort((a, b) => a.localeCompare(b)));
  const refreshLists = () => api.lists().then((l) => (marks = l)).catch(() => {});
  let installText = $state("");
  /** The profile name being typed: for a new profile, or to rename or copy the active one. */
  /** The profile name being asked for; `from`: the profile renamed or duplicated. */
  let nameFor = $state<{ kind: "new" | "rename" | "duplicate"; value: string; from?: string } | null>(null);
  let editingSep = $state<number | null>(null);
  let dragFrom = $state<number | null>(null);

  // --- derived ---
  const mods = $derived(new Map(rows.filter((r): r is ModRow => r.kind === "mod").map((r) => [r.id, r])));
  /** Workshop items installed (PZMM's copies and Steam's): Browse Workshop marks them. */
  const installedItems = $derived(new Set([...mods.values()].flatMap((m) => {
    const item = !m.missing && m.source.match(/^(?:workshop|steam):(\d+)$/)?.[1];
    return item ? [item] : [];
  })));
  const enabledCount = $derived([...mods.values()].filter((m) => m.on).length);
  /** Every mod, enabled ones first in load order: load rules are checked and sorted on this. */
  const list = $derived(loadList(rows, order));
  const problems = $derived(findProblems(list));
  const fixable = $derived([...problems.values()].filter((p) => p.fixable).length);
  const missingReqs = $derived.by(() => {
    const out = new Set<string>();
    for (const m of mods.values()) {
      if (m.on && m.missing) out.add(m.id); // enabled, but not installed
      else if (m.on) for (const r of m.requires) if (!mods.has(r)) out.add(r);
    }
    return out;
  });
  const problemCount = $derived(problems.size + [...mods.values()].filter((m) => m.on && m.missing).length);
  /** For each mod, the enabled mods that require it. */
  const needed = $derived(neededBy(rows));
  // --- columns: size and last change, loaded after the list (they walk every file) ---
  let stats = $state<Record<string, Stats>>({});
  let statsAsk = 0;
  function refreshStats() {
    const n = ++statsAsk;
    api.modStats().then((s) => n === statsAsk && (stats = s)).catch(() => {});
  }
  const SORTS = ["order", "name", "enabled", "installed", "size", "updated", "status"] as const;
  let sortBy = $state<(typeof SORTS)[number]>(SORTS.find((s) => s === prefs.modsSort) ?? "order");
  $effect(() => {
    Object.assign(prefs, { modsSearch: search, modsFilter: filter, modsKind: kind, modsTag: tag, modsLang: inLanguage, modsSort: sortBy });
  });
  const language = $derived(view?.language ?? "");

  const filtering = $derived(search.trim() !== "" || filter !== "all" || kind !== "" || tag !== "" || (inLanguage && !!language) || sortBy !== "order");
  // --- sections: separators can be collapsed (remembered per profile on this computer) ---
  let collapsed = $state<Set<string>>(new Set());
  const collapsedKey = $derived(`pzmm.collapsed.${view?.profile ?? ""}`);
  $effect(() => {
    try {
      collapsed = new Set(JSON.parse(localStorage.getItem(collapsedKey) ?? "[]"));
    } catch {
      collapsed = new Set();
    }
  });
  function toggleSection(name: string) {
    const next = new Set(collapsed);
    next.has(name) ? next.delete(name) : next.add(name);
    collapsed = next;
    try {
      localStorage.setItem(collapsedKey, JSON.stringify([...next]));
    } catch {}
  }
  /** For each row, the separator it sits under ("" above the first one). */
  const sectionOf = $derived.by(() => {
    let current = "";
    return rows.map((r) => (r.kind === "sep" ? (current = r.name) : current));
  });

  const place = $derived(new Map(order.map((id, i) => [id, i])));
  const visible = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return rows
      .map((r, i) => ({ r, i }))
      .filter(({ r, i }) => {
        if (r.kind === "sep") return !filtering && r.name !== "";
        if (!filtering && sectionOf[i] && collapsed.has(sectionOf[i])) return false;
        if (filter === "on" && !r.on) return false;
        if (filter === "off" && r.on) return false;
        if (filter === "problems" && !problems.has(r.id) && !r.missing) return false;
        if (filter === "fav" && !favourites.has(r.id)) return false;
        // hidden mods only show under Hidden, or when searched for
        if (filter === "hidden" ? !hidden.has(r.id) : hidden.has(r.id) && !q && filter !== "problems") return false; // Problems counts them too
        if (kind && !cats?.kinds[r.id]?.includes(kind)) return false;
        if (tag && !marks.tags[r.id]?.includes(tag)) return false;
        if (inLanguage && language && !r.languages?.includes(language)) return false;
        return !q || r.id.toLowerCase().includes(q) || r.name.toLowerCase().includes(q) || r.author.toLowerCase().includes(q);
      })
      .sort((a, b) => {
        if (a.r.kind !== "mod" || b.r.kind !== "mod") return 0;
        const [x, y] = [a.r, b.r];
        // favourites first (a setting) wherever the list isn't the library's own order
        if (prefs.favFirst && filtering && favourites.has(x.id) !== favourites.has(y.id)) return favourites.has(x.id) ? -1 : 1;
        if (sortBy === "order") return 0;
        const name = (m: ModRow) => (m.name || m.id).toLowerCase();
        if (sortBy === "name") return name(x).localeCompare(name(y));
        if (sortBy === "enabled") return Number(y.on) - Number(x.on) || (x.on ? (place.get(x.id) ?? -1) - (place.get(y.id) ?? -1) : name(x).localeCompare(name(y)));
        if (sortBy === "installed") return (stats[y.id]?.created ?? 0) - (stats[x.id]?.created ?? 0);
        if (sortBy === "size") return (stats[y.id]?.size ?? 0) - (stats[x.id]?.size ?? 0);
        if (sortBy === "updated") return (stats[y.id]?.modified ?? 0) - (stats[x.id]?.modified ?? 0);
        const rank = (m: ModRow) => (m.missing || problems.has(m.id) ? 0 : updates.has(m.id) ? 1 : 2);
        return rank(x) - rank(y) || name(x).localeCompare(name(y));
      });
  });

  /** Back from the game: reload, so changes made in its mod menu show before anything is saved over them. */
  let lastFocusLoad = 0;
  /** When the window last lost focus: a quick look at another window (under 10 s) reloads nothing. */
  let blurredAt = 0;
  // --- the game (PZMM - Client): its requests, and whether it loaded this profile's mods ---
  /** The game report already looked at (its time): only a newer one is compared. */
  let gameSeen = 0;
  /** Act on what the game asked: tabs and Check setup at once; installs and backups only after asking
   * (any mod can write the game's request file). */
  async function fromGame() {
    // not before the profile has loaded (the requests wait in their queue file); every focus, but not twice at
    // once: what comes meanwhile is taken right after
    if (!view) return;
    if (takingRequests) return void (takeAgain = true);
    takingRequests = true;
    api.gameBusy(true).catch(() => {});
    try {
      do {
        takeAgain = false;
        await takeRequests();
      } while (takeAgain);
    } finally {
      takingRequests = false;
      api.gameBusy(false).catch(() => {});
    }
  }
  let takingRequests = false;
  let takeAgain = false;
  /** A question for the game: it waits for a dialog already open, and asks again if another one takes its place
   * (so neither cancels the other's answer). */
  async function askGame(question: string, yes: Choice, lines: string[] = []) {
    for (;;) {
      while (asking.now) await new Promise((r) => setTimeout(r, 300));
      if ((await ask(question, lines, [yes])) === 0) return true;
      // answered: no; replaced by another dialog: asked again once that one is answered
      if (!asking.now) return false;
    }
  }
  async function takeRequests() {
    let reqs: GameRequest[] = [];
    try {
      reqs = await api.gameRequests();
    } catch {
      return;
    }
    const isSorting = (r: GameRequest) => r.action === "place" || r.action === "section" || r.action === "library" || r.action === "tags";
    // an Accept in the game's mod menu since the last look: taken in first, or the first edit is refused as "reloaded"
    if (reqs.some(isSorting)) await reloadList();
    // sorting edits first: a question about another request (or a long job it starts) mustn't keep them waiting
    reqs = [...reqs.filter(isSorting), ...reqs.filter((r) => !isSorting(r))];
    for (const r of reqs) {
      // why it wasn't done (in English, as the game is): the game's sorting edits settle on the ack
      let why = "";
      const sorting = r.action === "place" || r.action === "section" || r.action === "library";
      // a profile switch earlier in this batch: sorting edits made for the old profile aren't this one's
      // i18n-ignore: the game's words, in English as the game is
      if (sorting && r.profile !== view?.profile) why = `PZMM's profile is now ${view?.profile}.`;
      // i18n-ignore: the game's words, in English as the game is
      else if (!sorting && r.action !== "tags" && Date.now() / 1000 - parseFloat(r.id) > 170) why = "The game stopped waiting for PZMM: ask again.";
      else if (r.action === "place" || r.action === "section") {
        why = await (r.action === "place" ? placeFromGame(r) : sectionFromGame(r));
      } else if (r.action === "library") why = await libraryFromGame(r.lines);
      else if (r.action === "tags")
        try {
          marks = await api.modTags(r.mod, r.tags);
        } catch (e) {
          say(...keyedError(e), true);
          why = `PZMM couldn't save the tags: ${e}`;
        }
      else if (r.action === "open") tab = r.tab as typeof tab;
      else if (r.action === "check") openCheck();
      else if (r.action === "profile") why = await profileFromGame(r.name);
      else if (r.action === "fix") why = await fixFromGame(r.fix);
      else if (r.action === "restore") why = await restoreFromGame(r);
      else if (r.action === "install") {
        // titles, not bare IDs: any mod can write the request, so show what's being installed
        const titles = new Map((await api.workshopItems(r.ids).catch(() => [])).map((i) => [i.id, i.title]));
        if (await askGame(t("game.installAsk"), { label: t("game.install"), icon: "download" }, r.ids.map((id) => (titles.has(id) ? `${titles.get(id)} (${id})` : id)))) {
          // i18n-ignore: the game's words, in English as the game is
          if (!(await installAndSettle(r.ids.length === 1 ? t("busy.installingOne", { name: r.ids[0] }) : t("busy.installingMany", { count: r.ids.length }), () => api.install(r.ids), r.ids.length > 1))) why = "PZMM couldn't install them: see its activity log.";
        }
        // i18n-ignore: the game's words, in English as the game is
        else why = "Not installed: you said no in PZMM.";
      } else if (r.action === "backup") {
        // i18n-ignore: the game's words, in English as the game is
        if (!(await askGame(t("game.backupAsk", { name: r.name }), { label: t("game.backup"), icon: "copy" }))) why = "Not backed up: you said no in PZMM.";
        else
          try {
            await api.saveBackupFromGame(r.mode, r.name);
            say("game.backedUp", { name: r.name });
          } catch (e) {
            say(...keyedError(e), true);
            why = "PZMM couldn't back it up: see its activity log.";
          }
      }
      api.gameAck([{ id: r.id, ok: !why, why }]).catch(() => {});
    }
  }
  /** A drag in the game's My mods: the mod before `before`, else at the end of `section` (the game names
   * mods before any section, and those after a nameless break, "Other"). Returns why it wasn't done ("" if it was). */
  async function placeFromGame(r: Extract<GameRequest, { action: "place" }>) {
    const from = rows.findIndex((x) => x.kind === "mod" && x.id === r.mod);
    if (from < 0) return `${r.mod} isn't in your PZMM profile.`;
    let to = r.before ? rows.findIndex((x) => x.kind === "mod" && x.id === r.before) : -1;
    if (to < 0) {
      const s = rows.findIndex((x) => x.kind === "sep" && (x.name || "Other") === r.section);
      // the mods before any section: the game calls them "Other" too
      if (s < 0 && r.section === "Other") {
        // no nameless break yet: as the game makes it, one at the end with the mod after it
        const next: Row[] = [...rows.filter((_, i) => i !== from), { kind: "sep", name: "" }, rows[from]];
        return (await commit(next)) === false ? `PZMM couldn't move ${r.mod}: see its activity log.` : "";
      }
      // i18n-ignore: the game's words, in English as the game is
      else if (s < 0) return `There's no section called ${r.section} in your PZMM profile.`;
      else for (to = s + 1; to < rows.length && rows[to].kind !== "sep"; to++);
    }
    return (await move(from, to)) === false ? `PZMM couldn't move ${r.mod}: see its activity log.` : "";
  }

  /** Check setup's groups the game shows under Problems (not hidden, not a server's), with their fixes. */
  const gameGroups = () => groups(findings ?? []).filter((g) => !g.hidden && !isServer(g.finding));
  /** A fix's name for the game: stable across checks (the finding, its mods and the fix), so a later check still finds it. */
  const fixId = (g: Group, r: Remedy) => `${g.key}|${g.ids.join(",")}|${r.label}|${JSON.stringify(g.finding.vars)}|${JSON.stringify(r.vars ?? {})}`.replace(/[\t\r\n]/g, " ");
  // the game's Problems follow every check (findings.txt)
  /** The check whose results the game has (a new check, or the same one after a switch, is written again). */
  let findingsSent: Finding[] | null = null;
  $effect(() => {
    // none yet (since start, or a profile switch): a quick local check fills them, and with them the game's
    // list. An empty file would tell the game "nothing wrong" when nothing was checked.
    if (!findings) {
      if (view && !checkingSetup) untrack(() => checkSetup(false));
      return;
    }
    const one = (s: string) => s.replace(/[\t\r\n]+/g, " ");
    const level = { broken: "error", risk: "warn", check: "warn", low: "info" } as const;
    const lines = gameGroups().map((g) => {
      const mod = g.ids.length === 1 && mods.has(g.ids[0]) ? g.ids[0] : "-";
      const text = t(`doctor.${g.key}`, g.finding.vars);
      const said = mod === "-" && g.ids.some((id) => mods.has(id)) ? `${g.ids.map((id) => mods.get(id)?.name || id).join(", ")}: ${text}` : text;
      const fix = remedies(g.finding, g.ids, fixes).flatMap((r) => [fixId(g, r), one(t(`check.fix.${r.label}`, r.vars))]);
      return [level[g.severity], mod, one(said), ...fix].join("\t");
    });
    // every list edit re-runs this (it reads the mods): written once per check, which also stamps its time
    if (findings === findingsSent) return;
    findingsSent = findings;
    api.gameFindings(lines).catch(() => {});
  });
  /** One of Check setup's fixes, from the game: safe ones at once, the rest once you say yes. Returns why it wasn't done. */
  async function fixFromGame(id: string) {
    // PZMM just started: its own findings first, to find the fix the game names
    if (!findings) await checkSetup(false);
    for (const g of gameGroups())
      for (const r of remedies(g.finding, g.ids, fixes)) {
        if (fixId(g, r) !== id) continue;
        const label = t(`check.fix.${r.label}`, r.vars);
        // i18n-ignore: the game's words, in English as the game is
        if (!r.safe && !(await askGame(t("game.fixAsk", { fix: label }), { label, icon: r.icon }, [t(`doctor.${g.key}`, g.finding.vars)]))) return "Not done: you said no in PZMM.";
        try {
          const done = await r.run();
          // i18n-ignore: the game's words, in English as the game is
          if (done === false || done === null) return "Not done: see PZMM's activity log.";
          await checkSetup(false);
          say("game.fixed", { fix: label });
          return "";
        } catch (e) {
          say(...keyedError(e), true);
          return `PZMM couldn't do it: ${e}`;
        }
      }
    // i18n-ignore: the game's words, in English as the game is
    return "That problem has changed since: Check setup again.";
  }

  /** A backup the game asks to put back, once you say yes. Returns why it wasn't done ("" if it was). */
  async function restoreFromGame(r: Extract<GameRequest, { action: "restore" }>) {
    const b = ((await api.saveDetails(r.mode, r.name, true).catch(() => null))?.backups ?? []).find((x) => x.file === r.file);
    // i18n-ignore: the game's words, in English as the game is
    if (!b) return "That backup isn't there any more.";
    const when = fmtDateTime(b.time);
    // i18n-ignore: the game's words, in English as the game is
    if (!(await askGame(t("game.restoreAsk", { name: r.name, time: b.label ? `${b.label} (${when})` : when }), { label: t("game.restore"), icon: "undo" }))) return "Not restored: you said no in PZMM.";
    try {
      await api.saveRestoreFromGame(r.mode, r.name, r.file);
      return "";
    } catch (e) {
      say(...keyedError(e), true);
      return `PZMM couldn't restore it: ${e}`;
    }
  }

  /** The game's profile switch, once you say yes. Returns why it wasn't done ("" if it was). */
  async function profileFromGame(name: string) {
    // i18n-ignore: the game's words, in English as the game is
    if (!view?.profiles.includes(name)) return `There's no PZMM profile called ${name}.`;
    if (name === view.profile) return "";
    // i18n-ignore: the game's words, in English as the game is
    if (profileLock) return "PZMM is searching for a broken mod on this profile: finish that first.";
    // i18n-ignore: the game's words, in English as the game is
    if (!(await askGame(t("game.profileAsk", { name }), { label: t("game.profileUse"), icon: "swap" }))) return "Not switched: you said no in PZMM.";
    // i18n-ignore: the game's words, in English as the game is
    return (await run(t("busy.switchingTo", { name }), () => api.profileUse(name))) ? "" : "PZMM couldn't switch profiles: see its activity log.";
  }

  /** The game's library after an Undo or Redo there: its sections and order. On and off stay this profile's (the
   * game's own turn into the profile when it saves them); mods the game didn't list keep their place at the end. */
  async function libraryFromGame(lines: string[]) {
    const byId = new Map(rows.flatMap((r) => (r.kind === "mod" ? [[r.id, r] as const] : [])));
    const next: Row[] = [];
    for (const l of lines) {
      if (l.startsWith("# ---")) next.push({ kind: "sep", name: l.slice(5).trim() });
      else if (byId.has(l.slice(1))) next.push(byId.get(l.slice(1))!), byId.delete(l.slice(1));
    }
    if (byId.size) next.push({ kind: "sep", name: "" }, ...byId.values());
    // i18n-ignore: the game's words, in English as the game is
    return (await commit(next)) === false ? "PZMM couldn't change your sections: see its activity log." : "";
  }

  /** A section added, renamed or removed in the game's My mods. Returns why it wasn't done ("" if it was). */
  async function sectionFromGame(r: Extract<GameRequest, { action: "section" }>) {
    const at = (name: string) => rows.findIndex((x) => x.kind === "sep" && (x.name || "Other") === name);
    const i = at(r.name);
    const done = (ok: boolean | undefined) => (ok === false ? `PZMM couldn't change ${r.name}: see its activity log.` : "");
    if (r.op === "add") {
      // i18n-ignore: the game's words, in English as the game is
      if (!r.to || i >= 0) return i >= 0 ? "" : "No section to add it under.";
      // at the end of the section it was added under (else the end of the list)
      const s = at(r.to);
      let end = s < 0 ? rows.length : s + 1;
      while (s >= 0 && end < rows.length && rows[end].kind !== "sep") end++;
      return done(await commit([...rows.slice(0, end), { kind: "sep", name: r.name }, ...rows.slice(end)]));
    }
    // i18n-ignore: the game's words, in English as the game is
    if (i < 0) return `There's no section called ${r.name} in your PZMM profile.`;
    if (r.op === "rename" && r.to) {
      // i18n-ignore: the game's words, in English as the game is
      if (r.to !== r.name && separators.includes(r.to)) return `There's a section called ${r.to} already.`;
      return done(await renameSep(i, r.to));
    }
    if (r.op === "remove") return done(await removeSep(i));
    if (r.op === "move") {
      const to = r.to ? at(r.to) : rows.length;
      // i18n-ignore: the game's words, in English as the game is
      if (to < 0) return `There's no section called ${r.to} in your PZMM profile.`;
      const next = moveSection(rows, i, to);
      return next ? done(await commit(next)) : "";
    }
    return "";
  }

  /** A new report from the game's main menu with a different mod list than this profile: say so. */
  async function checkGame(first = false) {
    const g = await api.gameReport().catch(() => null);
    if (!g || g.at <= gameSeen) return;
    gameSeen = g.at;
    if (g.elsewhere) say("game.otherFolder", { folder: g.zomboid }, true);
    if (first || g.state !== "menu") return;
    const loaded = new Set(g.mods);
    const want = new Set(order);
    // only mods the game could load: installed, and for the build played
    const missing = order.filter((id) => { const m = mods.get(id); return m && !m.missing && loadable(m) && !loaded.has(id); }).length;
    const extra = g.mods.filter((id) => !want.has(id)).length;
    if (missing || extra) say("game.notApplied", { missing, extra });
  }

  async function refreshOnFocus() {
    if (busy || dragFrom !== null || editingSep !== null || Date.now() - lastFocusLoad < 3000 || Date.now() - blurredAt < 10_000) return;
    lastFocusLoad = Date.now();
    refreshLists();
    refreshSave(); // the game may have saved since
    await reloadList();
  }

  /** The profile again, with what the game's mod menu changed (on focus, and when default.txt changes while PZMM is in front). */
  async function reloadList() {
    if (busy || dragFrom !== null || editingSep !== null) return;
    try {
      const v = await api.load();
      if (busy) return; // the job shows what's on disk when it ends
      if (dragFrom !== null || editingSep !== null) {
        // a drag or rename started meanwhile works on these rows' indexes: kept unless the game's changes came in
        if (!v.adopted) return;
        dragFrom = editingSep = null;
      }
      show(v);
      refreshOverlaps(); // the game's mod menu may have changed the order
      if (v.adopted) {
        undoStack = redoStack = [];
        say("log.adopted", { profile: v.profile });
      }
      checkGame();
    } catch {}
  }

  // --- actions ---
  /** An activity line: a key from the locale files, or text as is (`key` null in the entry). */
  function say(key: string, vars: Vars = {}, error = false, url?: string) {
    const keyed = /^[a-z][\w-]*(\.[\w-]+)+$/i.test(key);
    const line: LogEntry = { text: t(key, vars), key: keyed ? key : null, vars, ...classify(keyed ? key : null, vars, error), time: Date.now(), url };
    log.push(line);
    if (log.length > 1000) log.splice(0, log.length - 1000); // the newest thousand lines
    if (error) logOpen = true;
    // what concerns the game shows in its activity too (activity_app.txt)
    if (line.kind !== "info" || GAME_LINES.has(key)) api.gameActivityAdd([{ at: Math.floor(line.time / 1000), kind: line.kind, text: line.text }]).catch(() => {});
  }
  /** Info lines about the game or the sync that the game's activity shows too (installs, updates, removals and errors always do). */
  const GAME_LINES = new Set(["log.adopted", "backend.mapOrderAdopted", "backend.profileUsed", "backend.profileRenamed", "backend.profileDeleted", "backend.saveBackedUp",
    "backend.saveRestored", "game.backedUp", "game.fixed", "game.settingOn", "game.settingOff", "game.notApplied", "game.otherFolder"]);

  /** The game's own activity, as "In game: ..." lines: those after the last one shown (its time, and how many of that second). */
  const GAME_SEEN = "pzmm.gameActivitySeen";
  async function fromGameActivity() {
    const lines = await api.gameActivity().catch(() => null);
    if (!lines?.length) return;
    let seen: { at: number; n: number } | null = null;
    try {
      seen = JSON.parse(localStorage.getItem(GAME_SEEN) ?? "null");
    } catch {}
    const last = lines[lines.length - 1];
    const lastN = lines.filter((l) => l.at === last.at).length;
    // the first time, what the game did before is history: not news
    if (seen) {
      const nth = new Map<number, number>();
      for (const l of lines) {
        const n = (nth.get(l.at) ?? 0) + 1;
        nth.set(l.at, n);
        if (l.at < seen.at || (l.at === seen.at && n <= seen.n)) continue;
        log.push({ text: t("game.inGame", { text: l.text }), key: "game.inGame", vars: { text: l.text }, kind: l.kind, items: [], time: Math.max(l.at * 1000, (log.at(-1)?.time ?? 0) + 1) });
      }
      if (log.length > 1000) log.splice(0, log.length - 1000);
    }
    try {
      localStorage.setItem(GAME_SEEN, JSON.stringify({ at: last.at, n: lastN }));
    } catch {}
  }

  async function copyLog() {
    try {
      // paths in backend lines carry the Windows user name: masked like Copy diagnostics
      await navigator.clipboard.writeText(await api.maskUser(log.map((l) => `${fmtTime(l.time)}  ${l.text}`).join("\n")));
      say("log.logCopied");
    } catch (e) {
      say("log.logCopyFailed", { error: t(...keyedError(e)) }, true);
    }
  }

  // a question above the tabs (previews, the launch check, confirmations from any view): lib/ask.svelte.ts

  /** Keep the profile as it's saved before a big change (`label` names it in Snapshots); false if that failed. */
  async function snapshot(label: string) {
    try {
      await api.snapshotCreate(label);
      return true;
    } catch (e) {
      say("log.snapshotFailed", { error: t(...keyedError(e)) }, true);
      return false;
    }
  }

  /** Show view `v`. On another profile, what's open about the last one closes: its snapshots, a question
   * (a preview showed its mods), the game switch. */
  /** Per profile, the mods that came new to it this session (the backend appended them): a New pill until restart. */
  let fresh = $state<Record<string, string[]>>({});
  const freshHere = $derived(new Set(fresh[view?.profile ?? ""] ?? []));
  function show(v: View) {
    const add = v.rows.flatMap((r) => (r.kind === "mod" && r.new && !fresh[v.profile]?.includes(r.id) ? [r.id] : []));
    if (add.length) fresh[v.profile] = [...(fresh[v.profile] ?? []), ...add];
    if (view && v.profile !== view.profile) {
      snapList = null;
      findings = null; // the last profile's check
      checkOpen = false;
      logs = [];
      answer(-1);
      modePlan = null;
      refreshSave();
    }
    view = v;
    rows = v.rows;
    order = v.order;
    loadError = ""; // read: whatever said they couldn't be is over
  }

  /** Why the mods couldn't be read ("" when they could): said above the tabs, with Try again and Settings. */
  let loadError = $state("");
  async function loadView() {
    const ok = await run(t("busy.loading"), api.load);
    loadError = ok ? "" : (log.findLast((l) => l.kind === "error")?.text ?? "");
    return ok;
  }

  /** Long jobs running (installs, updates, imports): download progress belongs to them. */
  let longJobs = 0;

  /**
   * Run a backend job that returns the new view. Errors go to the activity log; returns success.
   * `keepHistory` for list edits (undoable, no new files); `showLog` opens the log for long jobs.
   */
  async function run(label: string, job: () => Promise<View>, { keepHistory = false, showLog = false, renamed = false } = {}): Promise<boolean> {
    if (showLog) logOpen = true;
    if (showLog) longJobs++;
    // undo covers list edits only: anything else (installs, profile switches) resets it
    if (!keepHistory) undoStack = redoStack = [];
    const before = new Set(rows.flatMap((r) => (r.kind === "mod" ? [r.id] : [])));
    const knownProfiles = view?.profiles;
    let failed = false;
    pending++;
    busyLabel = label;
    try {
      const v = await job();
      show(v);
      if (v.adopted) {
        undoStack = redoStack = []; // the steps before are from before the game's changes
        say("log.adopted", { profile: v.profile });
      }
      if (selected && !v.rows.some((r) => r.kind === "mod" && r.id === selected)) selected = null;
      // a profile seen for the first time (first run, New, Import, Viewpoint) without separators gets autosorted
      const fresh = v.created || (!renamed && knownProfiles && !knownProfiles.includes(v.profile));
      if (fresh && cfg?.settings.autosort_new !== false && v.rows.length && !v.rows.some((r) => r.kind === "sep")) await groupLibrary(false);
      else if (!keepHistory && before.size) await placeNew(before);
      return true;
    } catch (e) {
      failed = true;
      say(...keyedError(e), true);
      // resync with what's on disk (the game's changes may be in it: the undo steps before them go)
      try {
        show(await api.load());
        undoStack = redoStack = [];
      } catch {}
      return false;
    } finally {
      pending--;
      if (showLog) longJobs--;
      if (showLog) notify("downloads", t(failed ? "notify.failed" : "notify.done", { job: label.replace(/[.…]+$/, "") }));
      if (!pending) busyLabel = "";
      if (!longJobs) progress = null;
      refreshZb();
      refreshOverlaps(!keepHistory);
      if (!keepHistory) refreshStats(); // files may have changed (installs, updates, removals)
    }
  }

  /** The Workshop categories (kept for placing enabled mods), behind the busy label. */
  async function categories(): Promise<Categories> {
    pending++;
    busyLabel = t("busy.readingTags");
    try {
      return (cats = await api.categories());
    } finally {
      pending--;
    }
  }

  /** Pick a kind to filter on: the kinds come with the Workshop tags, read the first time. */
  async function pickKind(k: string) {
    kind = k;
    if (k && !cats) await categories().catch((e) => say("log.tagsFailed", { error: t(...keyedError(e)) }, true));
  }

  /** Autosort the load order: needed mods on, frameworks first, translations last, every load rule satisfied. */
  async function autosortList() {
    if (!(await snapshot("autosort"))) return;
    let c: Categories;
    try {
      c = await categories();
    } catch (e) {
      // no tags (offline, rate-limited): still fix the load rules
      if (await commitList(sortLoadOrder(list, loadable))) say("log.tagsFailedSorted", { error: t(...keyedError(e)) }, true);
      return;
    }
    if (await commitList(autosort(list, c.of, c.order, loadable))) say("log.autosorted");
  }

  /** Put the load order A-Z: loadFirst / loadLast mods at the ends, requirements and load rules still first. */
  async function sortAz() {
    if (!(await snapshot("sortAz"))) return;
    if (await commitList(sortByName(list, loadable))) say("log.sortedAz");
  }

  /** Group the library into one section per category. `record` keeps it undoable (and asks before replacing your own sections). */
  async function groupLibrary(record = true) {
    let c: Categories;
    try {
      c = await categories();
    } catch (e) {
      return say("log.tagsFailed", { error: t(...keyedError(e)) }, true);
    }
    const own = separators.filter((s) => !c.order.includes(s));
    if (record && own.length && !(await confirmed(t("confirm.group", { sections: listed(own.length > 4 ? [...own.slice(0, 4), "…"] : own) }), { label: t("mods.group"), icon: "layers" }))) return;
    if (record && !(await snapshot("group"))) return;
    const next = groupByCategory(rows, c.of, c.order);
    const sections = next.filter((r) => r.kind === "sep").length;
    if (await commit(next, order, record)) say(record ? "log.groupedUndo" : "log.grouped", { count: sections });
  }

  /** In a library sorted into categories, file newly installed mods under theirs. */
  async function placeNew(before: Set<string>) {
    const fresh = rows.flatMap((r) => (r.kind === "mod" && !before.has(r.id) ? [r.id] : []));
    if (!fresh.length || !rows.some((r) => r.kind === "sep")) return;
    try {
      const c = (cats = await api.categories());
      if (rows.some((r) => r.kind === "sep" && c.order.includes(r.name))) await commit(placeInSections(rows, c.of, c.order, fresh), order, false);
    } catch {}
  }

  /** Enable disabled requirements and reorder the load order so requirements and load hints hold. */
  function fixLoadOrder() {
    commitList(sortLoadOrder(list, loadable));
  }

  async function checkUpdates() {
    try {
      updates = new Set(await api.checkUpdates());
      if (updates.size) say("log.updatesFound", { count: updates.size });
    } catch (e) {
      say("log.updatesCheckFailed", { error: t(...keyedError(e)) });
    }
  }

  /** The scheduled check (Settings: on start / every few hours), installing what it finds if asked to. */
  async function autoCheck() {
    if (busy) return;
    await checkUpdates();
    if (!updates.size || cfg?.settings.update_action !== "install") return;
    // not under a running game: the next check installs them (the Update button stays meanwhile)
    if (!(await api.gameRunning().catch(() => false))) await updateMods([...updates], true);
  }
  $effect(() => {
    const hours = ({ "1h": 1, "3h": 3 } as Record<string, number>)[cfg?.settings.update_check ?? ""];
    if (!hours) return;
    const timer = setInterval(autoCheck, hours * 3_600_000);
    return () => clearInterval(timer);
  });

  let checking = $state(false);
  async function checkUpdatesNow() {
    checking = true;
    await checkUpdates();
    checking = false;
    if (!updates.size) say("log.upToDate");
  }

  /** Re-download everything PZMM installed (repairs damaged copies); asks first, like an update. */
  async function redownloadAll() {
    const ours = [...mods.values()].filter(isOurs).map((m) => m.id);
    if (!ours.length) return say("log.nothingToRedownload");
    if (!(await confirmed(t("confirm.redownload", { count: ours.length }), { label: t("settings.repair.redownload"), icon: "download" }))) return;
    await updateMods(ours);
  }

  /** `background`: the scheduled check's install (its question doesn't take focus). */
  /** Whether it updated them. */
  async function updateMods(ids: string[], background = false) {
    await refreshSave();
    const inSave = latestSave ? ids.filter((i) => latestSave!.mods.includes(i)) : [];
    if (
      inSave.length &&
      !(await confirmed(t("confirm.updateInSave", { count: inSave.length, mods: listed(inSave), save: latestSave!.name }), { label: t("app.updateMods", { count: ids.length }), icon: "refresh" }, [], background))
    )
      return false;
    if (!(await snapshot("update"))) return false;
    const had = ids.filter((id) => updates.has(id));
    // only the ids that did install count (failures are in the log); an id's case may have changed
    let installed = new Set<string>();
    const ran = await run(ids.length === 1 ? t("busy.updatingOne", { name: ids[0] }) : t("busy.updatingMany", { count: ids.length }), async () => {
      const [v, got] = await api.updateMods(ids);
      installed = new Set(got.map((i) => i.toLowerCase()));
      return v;
    }, { showLog: true });
    const done = ids.filter((id) => installed.has(id.toLowerCase()));
    const ok = ran && (!ids.length || done.length > 0);
    if (ok) {
      updates = new Set([...updates].filter((u) => !done.includes(u)));
      (forgetFeed(), forgetGlance()); // What's new marks which updates wait: read afresh
      // what changed: each updated Workshop mod links to its change notes
      for (const id of had.filter((id) => done.includes(id))) {
        const item = mods.get(id)?.source.match(/^workshop:(\d+)$/)?.[1];
        if (item) say("log.updated", { name: mods.get(id)?.name || id }, false, `https://steamcommunity.com/sharedfiles/filedetails/changelog/${item}`);
      }
    }
    return ok;
  }

  /**
   * Every install: run it with the log open, then place what it enabled (the backend appends it) like
   * Autosort would, turning on the installed mods those need. Other mods are left as they are.
   * `many`: several things at once, so the profile is snapshotted first (Snapshots puts it back).
   * `want`: installed mods to turn on too, `skip`: mods to leave off (a collection's picks).
   */
  async function installAndSettle(label: string, job: () => Promise<View>, many = false, want: string[] = [], skip: string[] = []) {
    if (profileLocked() || (many && !(await snapshot("install")))) return false;
    const before = new Set(order);
    const ok = await run(label, job, { showLog: true });
    const fresh = order.filter((id) => !before.has(id));
    const added = [...new Set([...fresh, ...want.filter((id) => mods.has(id) && !before.has(id))])].filter((id) => !skip.includes(id));
    const off = fresh.filter((id) => skip.includes(id));
    if (ok && (added.length || off.length)) {
      const needed = [...withNeeds(added)].filter((id) => !mods.get(id)!.on);
      const next = rows.map((r) => (r.kind !== "mod" ? r : needed.includes(r.id) ? { ...r, on: true } : off.includes(r.id) ? { ...r, on: false } : r));
      await commit(next, placeAll([...added, ...needed], order.filter((id) => !off.includes(id))), false);
    }
    return ok;
  }

  /** Steam downloads (or deletes) in the background: look again once it has had a moment, unless an edit started meanwhile. */
  const reloadSoon = () => setTimeout(reloadList, 10_000);
  const installWorkshop = (id: string) => installAndSettle(t("busy.installingWorkshop", { id }), () => api.install([id]));
  /** Put back a Workshop item from the activity or the download history: installed as PZMM's copy, or subscribed in Steam again. */
  const reinstall = async (item: string, steam: boolean) =>
    editBusy ? false : steam ? run(t("busy.subscribing", { name: item }), () => api.workshopSubscribe(item, true), { showLog: true }).then((ok) => (ok && reloadSoon(), ok)) : installWorkshop(item);
  /** The download history's Remove: PZMM's copy removed (asking first), or the Steam subscription ended. */
  async function removeRecord(r: HistoryRecord) {
    if (editBusy) return;
    const m = mods.get(r.id);
    if (m && removable(m)) return removeMods([r.id]);
    const item = r.source.match(/^steam:(\d+)$/)?.[1] ?? (m?.source.match(/^steam:(\d+)$/)?.[1]);
    if (!item) return;
    if (m) return subscribe(m.id, false);
    if ((await ask(t("confirm.unsubscribeTitle", { name: r.name || item }), [t("confirm.unsubscribeLine")], [{ label: t("confirm.unsubscribe"), icon: "x" }])) === 0) return run(t("busy.unsubscribing", { name: r.name || item }), () => api.workshopSubscribe(item, false), { showLog: true });
  }

  /** Browse Workshop's Uninstall: PZMM's copies are removed (asking first, as Remove does); a Steam
   * subscription is ended in Steam. The Installed marks and My mods follow the library either way. */
  function uninstallWorkshop(item: string) {
    const all = [...mods.values()].filter((m) => !m.missing && m.source.match(/^(?:workshop|steam):(\d+)$/)?.[1] === item);
    const ours = all.filter(removable);
    if (ours.length) return removeMods(ours.map((m) => m.id));
    if (all.length) return subscribe(all[0].id, false);
  }

  async function installMissing() {
    let unresolved: string[] = [];
    const ok = await installAndSettle(t("busy.findingMissing", { count: missingReqs.size }), async () => {
      const [v, left] = await api.installMissing();
      unresolved = left;
      return v;
    }, missingReqs.size > 1);
    if (ok && unresolved.length) say("log.stillMissing", { ids: listed(unresolved) }, true);
  }

  // --- save safety: the latest save vs the active profile ---
  let latestSave = $state<Save | null>(null);
  /** The save the banner was dismissed for (another save, or other mods in it, bring it back; playing on doesn't). */
  let saveNoticeDismissed = $state("");
  const saveKey = (s: Save) => `${s.mode}/${s.name}/${s.mods.join(";")}`;
  const refreshSave = () =>
    api.serversAndSaves().then((d) => (latestSave = d.saves.find((s) => s.mode !== "Multiplayer") ?? null)).catch(() => {});
  const saveDiff = $derived.by(() => {
    if (!latestSave) return null;
    const on = new Set([...mods.values()].filter((m) => m.on).map((m) => m.id));
    const theirs = new Set(latestSave.mods);
    const missing = latestSave.mods.filter((m) => !on.has(m));
    const extra = [...on].filter((m) => !theirs.has(m));
    return missing.length || extra.length ? { missing, extra } : null;
  });
  /** Home's Continue: the latest save, when it's a game of yours played with the active profile's mods. */
  const homeSave = $derived(latestSave && !saveDiff && latestSave.mode !== "Multiplayer" ? latestSave : null);

  // --- share a profile ---
  let importText = $state<string | null>(null);
  /** Workshop collections read for Import, to choose what to take (null: none open). */
  let pack = $state<Pack | null>(null);
  /** The picker's main choice is a new profile (Import), else adding to this one (Browse Workshop). */
  let packNew = $state(false);

  /** Read pasted collections (or items, several at once) before installing anything. */
  async function previewCollection(text: string, asProfile = false) {
    packNew = asProfile;
    pending++;
    busyLabel = t("busy.readingCollection");
    try {
      pack = await api.collectionPreview(text);
    } catch (e) {
      say(...keyedError(e), true);
    } finally {
      pending--;
      if (!pending) busyLabel = "";
    }
  }

  function importPack(items: string[], skip: string[]) {
    const p = pack;
    pack = null;
    if (p && !profileLocked()) run(t("busy.importingCollection"), () => api.collectionProfile(p.title, items, skip), { showLog: true });
  }

  /** The picked items into the active profile: what isn't installed is downloaded, the chosen mods turned on. */
  function addPack(items: string[], skip: string[]) {
    const p = pack;
    pack = null;
    if (!p) return;
    const get = items.filter((i) => !p.installed.includes(i));
    const want = items.flatMap((i) => p.item_mods[i] ?? []);
    installAndSettle(t("busy.installingMany", { count: items.length }), () => (get.length ? api.install(get) : api.load()), true, want, skip);
  }

  /** Open Import, with the share code already in if the clipboard holds one. */
  async function openImport() {
    if (profileLocked()) return;
    importText = "";
    try {
      const clip = (await navigator.clipboard.readText()).trim();
      if (/^PZMM1:|^https:\/\/steamcommunity\.com\/(sharedfiles|workshop)\//.test(clip) && importText === "") importText = clip;
    } catch {}
  }

  // --- snapshots of the profile before big changes ---
  let snapList = $state<Snapshot[] | null>(null);
  const openSnapshots = () => api.snapshotList().then((s) => (snapList = s)).catch((e) => say(...keyedError(e), true));
  async function restoreSnapshot(id: string) {
    if (profileLocked()) return;
    if (await run(t("busy.restoringSnapshot"), () => api.snapshotRestore(id))) {
      say("log.snapshotRestored");
      openSnapshots();
    }
  }

  // --- a preview before a save's mods replace the list ---
  async function useSave(save: Save) {
    if (profileLocked()) return false;
    const profile = `Save ${save.name}`; // the backend's name for it
    const exists = view?.profiles.some((p) => p.toLowerCase() === profile.toLowerCase());
    const lines = [
      t(exists ? "preview.updatesProfile" : "preview.createsProfile", { name: profile }),
      ...modChanges(save.mods.filter((id) => !order.includes(id)), order.filter((id) => !save.mods.includes(id))),
    ];
    if ((await ask(t("preview.saveTitle", { save: save.name }), lines, [{ label: t("preview.useMods"), icon: "swap" }])) !== 0) return false;
    return run(t("busy.loadingSave", { name: save.name }), () => api.saveUse(save.mode, save.name));
  }

  // --- launch: problems first, then Java code ZombieBuddy will ask about ---
  /** Setup check findings the list's own problem marks already cover. */
  const LIST_KEYS = new Set(["notInstalled", "needsDisabled", "needsMissing", "needsLater", "incompatible", "loadAfter", "loadBefore"]);
  /** Whether the game was started. */
  async function launch() {
    if (busy || !(await preflight(false))) return false;
    try {
      await api.launchGame();
      feedCount = 0; // you're playing them now
      (forgetFeed(), forgetGlance());
      return true;
    } catch (e) {
      say("log.playFailed", { error: t(...keyedError(e)) }, true);
      return false;
    }
  }

  /** Before joining a server: the launch check; going on plays the mods too (What's new starts over). */
  async function joinCheck() {
    const ok = await preflight(true);
    if (ok) (feedCount = 0), (forgetFeed(), forgetGlance());
    return ok;
  }
  /** Setup check findings that don't stop the game but go wrong in it: asked about before playing. */
  const PLAY_WARNINGS = new Set(["mapFolderTwice", "mapOverlap", "serverBuild"]);

  /** The check before the game starts (Launch, or joining a server with `join`): problems first, then Java code
   * ZombieBuddy will ask about. False when the player stopped there. Warnings the player chose to play with
   * aren't asked about again on this profile until they change. */
  async function preflight(join: boolean): Promise<boolean> {
    const go = t(join ? "launch.joinAnyway" : "launch.anyway");
    pending++;
    busyLabel = t("busy.checking");
    const found = await api.doctor().catch((e) => (say("log.checkSetupFailed", { error: t(...keyedError(e)) }, true), [] as Finding[])).finally(() => {
      pending--;
      if (!pending) busyLabel = "";
    });
    // what stops a mod loading (game version, Java files) and the code scans' warnings; servers aren't launched here
    // what stops the game or could harm you (Check setup's Broken and Risk), and the map clashes
    const more = found.filter((f) => !f.hidden && !isServer(f) && !LIST_KEYS.has(f.key) && (["broken", "risk"].includes(severity(f)) || PLAY_WARNINGS.has(f.key)));
    const line = (f: Finding) => `${mods.get(f.subject)?.name || f.subject}: ${t(`doctor.${f.key}`, f.vars)}`;
    const errors = more.filter((f) => !PLAY_WARNINGS.has(f.key));
    const lines = [
      ...[...mods.values()].filter((m) => m.on && m.missing).map((m) => t("launch.notInstalled", { id: m.id })),
      ...[...problems].map(([id, p]) => `${mods.get(id)?.name || id}: ${t(p.key, { id: p.id })}`),
      ...errors.map(line),
    ];
    const warnings = more.filter((f) => PLAY_WARNINGS.has(f.key));
    const seenKey = `pzmm.playWarnings.${view?.profile ?? ""}`;
    const warnSet = JSON.stringify(warnings.map((f) => `${f.key} ${f.subject} ${JSON.stringify(f.vars)}`).sort());
    let seen = "";
    try {
      seen = localStorage.getItem(seenKey) ?? "";
    } catch {}
    const warn = warnings.length && warnSet !== seen ? warnings.map(line) : [];
    if (lines.length || warn.length) {
      // fixes PZMM can make here, then the check runs again
      const has = (...keys: string[]) => errors.some((f) => keys.includes(f.key));
      const trust = [...new Set(errors.flatMap((f) => (f.action?.dismiss ? [f.action.dismiss] : [])))];
      const autoFix = [
        ...(has("zbAgent") ? [() => run(t("busy.zbSetup"), () => api.zbInstall(false), { showLog: true })] : []),
        ...(has("viewpoint41") ? [() => run(t("busy.saving"), () => api.setBuild(42))] : []),
      ];
      const tooNew = [...new Set(errors.filter((f) => ["versionMin", "versionMax", "only41"].includes(f.key)).map((f) => f.subject))];
      // a newer game fixes a version limit; a Build 41-only mod it doesn't
      const gameUpdate = has("versionMin", "versionMax");
      // Leaf is set up by hand in the Java tab
      const leafFix = has("javaNoLeaf", "javaLeafOff") && !autoFix.length && !problems.size;
      const again = () => (refreshZb(), preflight(join));
      const choices = [
        ...(autoFix.length ? [{ label: t("launch.fixAndPlay"), icon: "wand" as IconName, act: async () => {
          for (const fix of autoFix) if (!(await fix())) return false;
          return again();
        } }] : []),
        ...(has("javaBlocked") ? [{ label: t("check.fix.allow"), icon: "check" as IconName, act: async () => {
          await Promise.all(errors.filter((f) => f.key === "javaBlocked" && f.vars.sha).map((f) => api.zbUnblock(f.vars.sha)));
          return again();
        } }] : []),
        // trusting flagged Java files is never folded into a general fix
        ...(trust.length ? [{ label: t("launch.trustFiles", { count: trust.length }), icon: "check" as IconName, act: async () => {
          await Promise.all(trust.map((sha) => api.scanDismiss(sha)));
          return again();
        } }] : []),
        ...(tooNew.length ? [{ label: t("launch.turnOff", { count: tooNew.length }), icon: "x" as IconName, act: async () => {
          const ids = await withDependants(tooNew);
          return !!ids && (await setMods(ids, false)) && again();
        } }] : []),
        ...(gameUpdate ? [{ label: t("launch.updateGame"), icon: "steam" as IconName, act: () => api.openUrl("steam://nav/games/details/108600") }] : []),
        // a Build 42-only mod on a Build 41 profile: switching is the player's call (their save may be a 41 game)
        ...(has("only42") ? [{ label: t("check.fix.switchBuild", { build: "42" }), icon: "swap" as IconName, act: async () => (await run(t("busy.saving"), () => api.setBuild(42))) && again() }] : []),
        ...(missingReqs.size || fixable || leafFix ? [{ label: t("launch.fix"), icon: "wand" as IconName, act: () => (leafFix ? (tab = "zb") : fixProblems()) }] : []),
        ...(warnings.some((f) => f.key.startsWith("map")) ? [{ label: t("launch.arrangeMaps"), icon: "layers" as IconName, act: () => (tab = "maps") }] : []),
        { label: go, icon: "play" as IconName, act: () => true },
        // only warnings: playing with them can be the player's call for good
        ...(!lines.length ? [{ label: t(join ? "launch.joinStopAsking" : "launch.stopAsking"), icon: "check" as IconName, act: () => {
          try {
            localStorage.setItem(seenKey, warnSet);
          } catch {}
          return true;
        } }] : []),
      ];
      const title = lines.length ? t("launch.title", { count: lines.length }) : t("launch.warnTitle", { count: warn.length });
      const hint = gameUpdate && view?.steam_branch ? [t("launch.branchHint", { branch: view.steam_branch })] : [];
      const i = await ask(title, [...lines, ...warn, ...hint], choices);
      try {
        if (i < 0 || (await choices[i].act()) !== true) return false;
      } catch (e) {
        say(...keyedError(e), true);
        return false;
      }
    }
    // ZombieBuddy doesn't ask about the enabled Java mods: starting the game allows them (their scans' warnings are above)
    return true;
  }

  /** Install what's missing and fix the order, then list what's left. */
  async function fixProblems() {
    if (missingReqs.size) await installMissing();
    else if (fixable) await commitList(sortLoadOrder(list, loadable));
    await tick();
    tab = "mods";
    filter = problemCount ? "problems" : "all";
  }

  async function shareProfile() {
    try {
      await navigator.clipboard.writeText(encodeShare(await api.profileExport()));
      say("log.shareCopied", { profile: view?.profile ?? "" });
      logOpen = true;
    } catch (e) {
      say("log.shareFailed", { error: t(...keyedError(e)) }, true);
    }
  }

  /** Import what was pasted: a share code, a Workshop collection (installed as a profile of its own), or a preset
   * from the game's mod menu (its Share text, `name:A;B;C`). */
  async function importProfile(e: Event) {
    e.preventDefault();
    const text = (importText ?? "").trim();
    importText = null;
    try {
      if (/^(PZMM1:|\{)/.test(text)) {
        const shared = decodeShare(text);
        // what it brings, before anything installs: enabled mods, and those to download
        const on = shared.list.split("\n").flatMap((l) => (l.trim().startsWith("+") ? [l.trim().slice(1).trim()] : []));
        const get = on.filter((id) => !mods.has(id) && shared.workshop[id]).length;
        const lines = [t("preview.shareMods", { count: on.length }), ...(get ? [t("preview.shareDownloads", { count: get })] : [])];
        if ((await ask(t("preview.shareTitle", { name: shared.name }), lines, [{ label: t("common.import"), icon: "upload" }])) !== 0) return void (importText = text);
        await run(t("busy.importing", { name: shared.name }), () => api.profileImport(shared), { showLog: true });
      } else if (/steamcommunity\.com|^[\d\s,]+$/.test(text)) {
        await previewCollection(text, true);
      } else {
        const preset = parsePreset(text);
        if (!preset) throw new Error(t("api.badShareCode"));
        await importPreset({ from: "game", ...preset, workshop: [] });
      }
    } catch (err) {
      say(...keyedError(err), true);
    }
  }

  /** A preset the game's mod menu or a Workshop mod manager saved, as a profile (a server preset's items are installed). */
  const importPreset = (p: Preset) => run(t("busy.importing", { name: p.name }), () => api.presetImport(p), { showLog: true });
  let presetPick = $state(0);

  /** Save the active profile's load order as a preset in Mod Manager's list or the game's mod menu. */
  async function savePreset(from: "modManager" | "game") {
    const name = view?.profile ?? "";
    try {
      marks = await api.presetSave({ from, name, mods: order, workshop: [] });
      say(from === "game" ? "log.savedGamePreset" : "log.savedModManagerPreset", { name });
    } catch (e) {
      say(...keyedError(e), true);
    }
  }
  let shareAt = $state<{ x: number; y: number } | null>(null);

  // --- health check ---
  let findings = $state<Finding[] | null>(null);
  let logs = $state<LogReport[]>([]);
  /** A finding's Install: the Workshop item that surely provides `req` (else pick one in `by`'s details). */
  /** Whether it installed it. */
  async function installRequirement(req: string, by: string) {
    if (profileLocked()) return false;
    pending++;
    busyLabel = t("busy.lookingUp", { id: req });
    try {
      const best = (await api.findRequirement(req, by))[0];
      if (best?.confirmed && best.fits_build) {
        const ok = await installWorkshop(best.id);
        if (ok) checkSetup();
        return ok;
      }
      selected = by;
      say("log.pickRequirement", { id: req });
      return false;
    } catch (e) {
      say(...keyedError(e), true);
      return false;
    } finally {
      pending--;
    }
  }

  async function disableMod(id: string) {
    if (mods.get(id)?.on && (await toggle(id))) checkSetup();
  }

  /** A check setup running (its button says so); its findings show only on the profile it checked. */
  let checkingSetup = $state(false);
  /** The Check setup window, and the game log opened from it. */
  let checkOpen = $state(false);
  let readingLog = $state(false);
  /** The window opens at once with the last results (or "Checking"), and fills in as the check answers. */
  function openCheck() {
    checkOpen = true;
    checkSetup();
  }

  /** The game can load this mod on the build played (Build 42 skips a mod built for 41 only). */
  const loadable = (r: ModRow) => !(view?.build !== 41 && r.only41);

  const failed = (e: unknown) => say(...keyedError(e), true);

  /** What Check setup's fixes do here. */
  const fixes: Fixes = {
    isMod: (id) => mods.has(id),
    canLoad: (id) => !!mods.get(id) && loadable(mods.get(id)!),
    itemOf: (id) => mods.get(id)?.source.match(/^(?:workshop|steam):(\d+)$/)?.[1] ?? "",
    install: (id, by) => installRequirement(id, by),
    // turning off asks first about the enabled mods that need them, as the list's own toggle does
    setMods: async (ids, on) => {
      const turning = on ? ids : await withDependants(ids);
      return turning && setMods(turning, on);
    },
    uninstall: async (ids) => {
      const ours = ids.filter((id) => mods.get(id) && removable(mods.get(id)!));
      const steam = ids.filter((id) => !ours.includes(id) && mods.get(id)?.source.startsWith("steam:"));
      const ok = !ours.length || (await removeMods(ours));
      let done = ok;
      for (const id of steam) done = (await subscribe(id, false)) && done;
      return done;
    },
    sort: () => commitList(sortLoadOrder(list, loadable)),
    trust: (sha) => api.scanDismiss(sha).then(() => true, (e) => (failed(e), false)),
    setBuild: (b) => run(t("busy.saving"), () => api.setBuild(b)),
    zbSetup: () => run(t("busy.zbSetup"), () => api.zbInstall(false), { showLog: true }),
    zbRemove: () => run(t("busy.zbRemove"), api.zbUninstall, { showLog: true }),
    update: (ids) => {
      const ours = ids.filter((id) => mods.get(id) && isOurs(mods.get(id)!));
      if (!ours.length) return say("check.steamUpdates"), Promise.resolve(false);
      return updateMods(ours);
    },
    // the Workshop's own search, for the same name tagged Build 42
    findB42: async (id) => api.openUrl(`https://steamcommunity.com/workshop/browse/?appid=108600&searchtext=${encodeURIComponent(mods.get(id)?.name || id)}&requiredtags[]=Build+42`),
    unblock: (sha) => api.zbUnblock(sha).then(() => true, (e) => (failed(e), false)),
    folderAuto: (key) => api.folderSet(key, "", false).then((c) => (cfg = c)).then(loadView).then(() => (refreshSave(), refreshZb(), true), (e) => (failed(e), false)),
    installItem: (item) => installWorkshop(item),
    unsubscribe: async (items) => {
      let ok = true;
      for (const item of items) ok = (await run(t("busy.unsubscribing", { name: item }), () => api.workshopSubscribe(item, false), { showLog: true })) && ok;
      return ok;
    },
    go: (to) => ((checkOpen = false), (tab = to)),
    openUrl: (url) => api.openUrl(url),
    launch: () => launch(),
  };
  /** Findings only the online check makes (the Workshop's view of items): a local re-check keeps them. */
  const ONLINE = new Set(["workshopGone", "workshopBanned", "workshopHidden", "workshopUnlisted", "notOriginal"]);
  let checkSeq = 0;
  async function checkSetup(online = true) {
    const profile = view?.profile;
    const seq = ++checkSeq;
    const current = () => seq === checkSeq && view?.profile === profile;
    checkingSetup = true;
    try {
      // the local checks answer at once; the Workshop's (online) follow
      const local = await api.doctor(false);
      if (current()) findings = [...local, ...(findings ?? []).filter((f) => ONLINE.has(f.key) && mods.has(f.subject))];
      if (!online) return;
      const got = await Promise.all([api.doctor(true), api.logReports().catch(() => [])]);
      if (current()) [findings, logs] = got;
    } catch (e) {
      say("log.checkSetupFailed", { error: t(...keyedError(e)) }, true);
    } finally {
      if (seq === checkSeq) checkingSetup = false;
    }
  }

  // --- find a broken mod by halving the suspects each game run ---
  /** `profile`: where it started; its test lists and the restore go there only. */
  let bisect = $state<{ profile: string; original: Lists; suspects: string[]; testing: string[]; round: number } | null>(null);
  /** Why profile changes are blocked now ("" when they aren't): a search for a broken mod runs on this profile. */
  const profileLock = $derived(bisect && bisect.profile === view?.profile ? t("findings.finishFirst") : "");
  /** List edits wait for a job, and for a search for a broken mod (it owns the lists until it ends). */
  const editBusy = $derived(busy || !!profileLock);
  /** Update and Play are in the title row: beside them, the tabs wouldn't fit on the tab bar. */
  let tight = $state(false);
  /** `use:fitTabs` on the tab bar: keeps `tight` true while the tabs, Update and Play and the links don't fit in one row. */
  function fitTabs(bar: HTMLElement) {
    const fit = () => {
      const tabs = bar.querySelector<HTMLElement>(".tabs")!;
      const right = [...document.querySelectorAll<HTMLElement>(".right")].find((r) => !r.hidden);
      const rest = [...bar.children].reduce((w, c) => (c.contains(tabs) || c.classList.contains("right") ? w : w + (c as HTMLElement).offsetWidth), 0);
      // the tabs as wide as they are untight (tight ones pad 5px less a side), so it doesn't flip back and forth
      const wide = tabs.scrollWidth + (tight ? 10 * tabs.children.length : 0);
      tight = wide + (right?.offsetWidth ?? 0) + rest + 24 > bar.clientWidth; // 24: the bar's padding
      tabEnds();
    };
    const seen = new ResizeObserver(fit);
    [bar, ...document.querySelectorAll(".right, .tabs button")].forEach((e) => seen.observe(e));
    return { destroy: () => seen.disconnect() };
  }
  /** The title row in one line: 1 when its button labels went (icons and tooltips stay), 2 when it wraps too. */
  let squeeze = $state(0);
  /** `use:fitTitle` on the title row: squeezes it a step while it overflows, back a step once the window is as
   * wide as that step needed. */
  function fitTitle(top: HTMLElement) {
    const need: number[] = [];
    const fit = () => {
      // how far the row's items reach past its padding (the window buttons sit in it, positioned on their own)
      const box = top.getBoundingClientRect();
      const items = [...top.children].filter((c) => ["static", "relative"].includes(getComputedStyle(c).position) && c.getClientRects().length);
      const over = Math.max(...items.map((c) => c.getBoundingClientRect().right)) - (box.right - parseFloat(getComputedStyle(top).paddingRight));
      if (squeeze < 2 && over > 1) need[squeeze] = box.width + over, squeeze++;
      else if (squeeze && box.width >= need[squeeze - 1]) squeeze--;
    };
    const seen = new ResizeObserver(fit);
    [top, ...top.querySelectorAll(":scope > *")].forEach((e) => seen.observe(e));
    return { destroy: () => seen.disconnect() };
  }
  /** Tabs that don't fit scroll sideways: which ends have more (a chevron each, over a fade), and whether a tab
   * past that end has an alert dot or a count (the chevron carries the dot). */
  let tabsEl = $state<HTMLElement>();
  let ends = $state({ l: false, r: false, la: false, ra: false });
  function tabEnds() {
    const el = tabsEl;
    if (!el) return;
    const box = el.getBoundingClientRect();
    const marks = [...el.querySelectorAll(".alert, .badge")].map((m) => m.getBoundingClientRect());
    const l = el.scrollLeft > 1;
    const r = el.scrollLeft + el.clientWidth < el.scrollWidth - 1;
    // 36: the chevron's width, over the tabs' ends
    ends = { l, r, la: l && marks.some((m) => m.left < box.left + 36), ra: r && marks.some((m) => m.right > box.right - 36) };
  }
  // the open tab in view (arrow keys, a link from another tab), and the ends' dots kept up with the alerts
  $effect(() => {
    void [tab, zbNeeded, folderTrouble, feedCount, tight];
    tick().then(() => {
      // scrolled by hand: scrollIntoView would move where the first Tab starts (past the title bar, to after the tabs)
      const at = tabsEl?.querySelector(".active")?.getBoundingClientRect(), box = tabsEl?.getBoundingClientRect();
      if (tabsEl && at && box) tabsEl.scrollLeft += Math.min(0, at.left - box.left) + Math.max(0, at.right - box.right);
      tabEnds();
    });
  });
  /** Narrow windows: the load order instead of the list (it's beside it otherwise). */
  let orderOpen = $state(false);
  /** True (and said in the log) when the profile can't change now. */
  const profileLocked = () => !!profileLock && (say("findings.finishFirst", {}, true), true);
  /** The server setup being asked about (see ServerSetup). */
  type SetupAnswer = { items: string[]; dropMods: string[] } | null;
  let setupAsk = $state<{ server: GameServer; ids: string[]; items: SetupItem[] | null; unlisted: number; done: (a: SetupAnswer) => void } | null>(null);
  /** Before a server's mods download into a profile of its own: asked, unless there's nothing to get or the
      server already has its profile (and none of them is a guess). `known`: the items' details already at
      hand; only the rest are asked; `unlisted`: mods the server runs but doesn't list. The items to download
      and the mod ids the guesses ticked off leave out of the profile, or null: cancelled. */
  function confirmSetup(server: GameServer, ids: string[], known: SetupItem[], unlisted = 0): Promise<SetupAnswer> {
    if (!ids.length || (Object.values(view?.servers ?? {}).includes(server.addr) && !known.some((k) => k.likely && ids.includes(k.id)))) return Promise.resolve({ items: ids, dropMods: [] });
    setupAsk?.done(null); // a new ask answers the one it replaces: cancelled
    return new Promise((done) => {
      const have = [...new Map(known.filter((k) => (k.title || k.likely) && ids.includes(k.id)).map((k) => [k.id, k])).values()];
      const rest = ids.filter((id) => !have.some((k) => k.id === id));
      setupAsk = { server, ids, items: have.length ? have : null, unlisted, done: (a) => ((setupAsk = null), done(a)) };
      if (!rest.length) return;
      // compared as the state holds it (a proxy), so a later ask isn't filled with this one's details
      const asked = setupAsk;
      const fill = (items: SetupItem[]) => setupAsk === asked && asked && (asked.items = [...have, ...items]);
      api.workshopItems(rest).then(fill, () => fill([]));
    });
  }
  async function setupServer(server: GameServer, items: string[], ids: string[], maps: string[], known: SetupItem[] = [], unlisted = 0) {
    const chosen = profileLocked() ? null : await confirmSetup(server, items, known, unlisted);
    if (!chosen) return false;
    // every download ticked off: nothing to get, so no profile is made for it (a join goes on as things are)
    if (items.length && !chosen.items.length) return true;
    const keep = ids.filter((id) => !chosen.dropMods.includes(id));
    return run(t("busy.settingUpServer", { name: server.name }), () => api.serverSetup(server, chosen.items, keep, maps), { showLog: true });
  }
  /** The search as it stands, kept on this computer while it runs: if PZMM closes mid-way, the next start
   * offers to go on with it or put the list back (`resumeBisect`). */
  const BISECT_KEY = "pzmm.bisect";
  function setBisect(b: typeof bisect) {
    bisect = b;
    try {
      if (b) localStorage.setItem(BISECT_KEY, JSON.stringify(b));
      else localStorage.removeItem(BISECT_KEY);
    } catch {}
  }
  async function resumeBisect() {
    let b: typeof bisect = null;
    try {
      b = JSON.parse(localStorage.getItem(BISECT_KEY) ?? "null");
    } catch {}
    if (!b || !view) return; // the mods weren't read: asked next start
    if (!view.profiles.includes(b.profile)) return setBisect(null); // its profile is gone
    const i = await ask(t("findings.bisectLeft", { profile: b.profile }), [t("findings.bisectLeftLine", { count: b.suspects.length })],
      [{ label: t("findings.bisectResume"), icon: "play" }, { label: t("findings.bisectRestore"), icon: "rollback" }]);
    if (i === 0) bisect = b;
    else if (i === 1) {
      setBisect(null);
      restoreBisected(b.original, b.profile);
    } // Cancel: asked again next start
  }

  /** False (and said) when the search's profile isn't the active one: its lists mustn't go to another. */
  const onBisectProfile = (b: NonNullable<typeof bisect>) => b.profile === view?.profile || (say("log.bisectOtherProfile", { profile: b.profile }, true), false);

  /** Enabled = everything not under suspicion + the half (or the first `count`) being tested + what those require.
   * The search moves on only once that list is saved (else it stays where it was, and says so). */
  async function bisectApply(b: NonNullable<typeof bisect>, count?: number) {
    const { on, testing } = bisectRound(b.original.rows, b.suspects, (id) => mods.get(id)?.requires ?? [], count);
    const o = placeAll([...on].filter((id) => !b.original.order.includes(id)), b.original.order); // a requirement that was off
    if (await commit(b.original.rows.map((r) => (r.kind === "mod" ? { ...r, on: on.has(r.id) } : r)), o, false, b.profile)) setBisect({ ...b, testing });
    else say("log.bisectSaveFailed", {}, true);
  }

  /** `first`: mods the logs point at; the first round tests just them, then the rest is halved. */
  async function bisectStart(first: string[] = []) {
    if (bisect || busy) return; // one search at a time (a second click lands while the first one saves)
    const enabled = [...mods.values()].filter((m) => m.on && !m.missing).map((m) => m.id);
    if (enabled.length < 2) return say("log.bisectNeedsTwo", {}, true);
    // Snapshots keeps the list from before it too
    pending++;
    busyLabel = t("busy.saving");
    const kept = await snapshot("bisect");
    pending--;
    if (!kept) return;
    const lead = first.filter((id) => enabled.includes(id));
    const suspects = [...lead, ...enabled.filter((id) => !lead.includes(id))];
    findings = null;
    undoStack = redoStack = []; // undoing into a half-disabled test list would be a trap
    bisectApply({ profile: view?.profile ?? "", original: $state.snapshot({ rows, order }), suspects, testing: [], round: 1 }, lead.length && lead.length < suspects.length ? lead.length : undefined);
  }

  function bisectAnswer(stillBroken: boolean) {
    if (!bisect || !onBisectProfile(bisect)) return;
    const suspects = stillBroken ? bisect.testing : bisect.suspects.filter((s) => !bisect!.testing.includes(s));
    if (suspects.length <= 1) {
      const { original, profile } = bisect;
      setBisect(null);
      undoStack = redoStack = [];
      restoreBisected(original, profile);
      say(suspects.length ? "log.bisectFound" : "log.bisectNone", { id: suspects[0] ?? "" }, !!suspects.length);
      logOpen = true;
      if (suspects.length) selected = suspects[0];
      return;
    }
    bisectApply({ ...bisect, suspects, round: bisect.round + 1 });
  }

  /** The list from before the search, back; if that can't be saved, Snapshots has it. */
  const restoreBisected = (original: Lists, profile: string) =>
    commit(original.rows, original.order, false, profile).then((ok) => ok || say("log.bisectRestoreFailed", {}, true));

  function bisectStop() {
    if (!bisect || !onBisectProfile(bisect)) return;
    const { original, profile } = bisect;
    setBisect(null);
    undoStack = redoStack = [];
    restoreBisected(original, profile);
  }

  // --- undo/redo for list edits (toggle, reorder, separators, fixes) in the library and the load order ---
  type Lists = { rows: Row[]; order: string[] };
  let undoStack = $state<Lists[]>([]);
  let redoStack = $state<Lists[]>([]);

  /** Save the library and the load order (kept to the library's enabled mods) to `profile`, the one they're from.
   * Locked while a search for a broken mod runs: only its own test lists (which name their profile) go through. */
  const commit = async (next: Row[], nextOrder = order, record = true, profile?: string) => {
    if (profile === undefined && profileLocked()) return false;
    if (record) {
      undoStack = [...undoStack.slice(-49), $state.snapshot({ rows, order })];
      redoStack = [];
    }
    rows = next;
    order = syncOrder(next, nextOrder);
    return run(t("busy.saving"), () => api.save(rows, order, profile ?? view?.profile ?? ""), { keepHistory: true });
  };

  /** Save a sorted load list: its order, and the mods it turned on. */
  function commitList(sorted: Row[]) {
    const on = orderOf(sorted);
    return commit(rows.map((r) => (r.kind === "mod" && !r.on && on.includes(r.id) ? { ...r, on: true } : r)), on);
  }

  /** `o` with each of `ids` placed where Autosort would put it. */
  function placeAll(ids: string[], o = order) {
    for (const id of ids) o = placeInOrder(o, id, (i) => mods.get(i), cats ? tiers(cats.of, cats.order) : undefined);
    return o;
  }

  function undo() {
    const prev = undoStack.at(-1);
    if (!prev || busy) return;
    undoStack = undoStack.slice(0, -1);
    redoStack = [...redoStack, $state.snapshot({ rows, order })];
    commit(prev.rows, prev.order, false);
  }

  function redo() {
    const next = redoStack.at(-1);
    if (!next || busy) return;
    redoStack = redoStack.slice(0, -1);
    undoStack = [...undoStack, $state.snapshot({ rows, order })];
    commit(next.rows, next.order, false);
  }

  const TAB_IDS = ["home", "mods", "feed", "workshop", "maps", "servers", "saves", "zb", "authoring", "settings"];
  const KINDS = ["framework", "patch", "map", "ui", "translation"];
  /** The status filters on the bar; favourites, hidden, kind, tag and language live in the Filters menu. */
  const STATUS_IDS = ["all", "on", "off", "problems"];
  const SORT_KEYS: Record<(typeof SORTS)[number], string> = { order: "mods.sortOrder", name: "mods.sortName", enabled: "mods.sortEnabled", installed: "mods.sortInstalled", size: "mods.sortSize", updated: "mods.sortUpdated", status: "mods.sortStatus" };
  let filtersAt = $state<{ x: number; y: number } | null>(null);
  let sortAt = $state<{ x: number; y: number } | null>(null);
  let moreAt = $state<{ x: number; y: number } | null>(null);
  /** The filters beyond the status bar that are on, each with how to clear it (shown as chips). */
  const extraFilters = $derived([
    ...(filter === "fav" ? [{ key: "fav", label: t("mods.filterFavourites"), clear: () => (filter = "all") }] : []),
    ...(filter === "hidden" ? [{ key: "hidden", label: t("mods.filterHidden"), clear: () => (filter = "all") }] : []),
    ...(kind ? [{ key: "kind", label: t(`kinds.${kind}`), clear: () => pickKind("") }] : []),
    ...(tag ? [{ key: "tag", label: tag, clear: () => (tag = "") }] : []),
    ...(inLanguage && language ? [{ key: "lang", label: t("mods.inLanguage", { language }), clear: () => (inLanguage = false) }] : []),
  ]);
  const filterItems = $derived<MenuItem[]>([
    { heading: t("mods.showLabel") },
    { label: t("mods.filterFavourites"), checked: filter === "fav", keep: true, run: () => (filter = filter === "fav" ? "all" : "fav") },
    { label: hidden.size ? t("mods.filterHiddenN", { count: hidden.size }) : t("mods.filterHidden"), checked: filter === "hidden", keep: true, run: () => (filter = filter === "hidden" ? "all" : "hidden") },
    ...(language ? [{ label: t("mods.inLanguage", { language }), checked: inLanguage, keep: true, run: () => (inLanguage = !inLanguage) }] : []),
    { heading: t("mods.kindHeading") },
    { label: t("mods.kindAny"), checked: !kind, keep: true, run: () => pickKind("") },
    ...KINDS.map((k) => ({ label: t(`kinds.${k}`), checked: kind === k, keep: true, run: () => pickKind(kind === k ? "" : k) })),
    ...(allTags.length ? [{ heading: t("mods.tagHeading") }, ...allTags.map((x) => ({ label: x, checked: tag === x, keep: true, run: () => (tag = tag === x ? "" : x) }))] : []),
  ]);
  function clearFilters() {
    search = "";
    filter = "all";
    tag = "";
    inLanguage = false;
    sortBy = "order";
    pickKind("");
  }

  function globalKey(e: KeyboardEvent) {
    // the WebView's own reload, print and find aren't PZMM's (a reload would drop what's running)
    if (e.key === "F5" || ((e.ctrlKey || e.metaKey) && ["r", "p", "f"].includes(e.key.toLowerCase()))) return e.preventDefault();
    // a box or window over the page: its shortcuts (Ctrl+Z, /, Ctrl+A) stay off the list behind; Escape closes a box
    const box = asking.now || importText !== null || snapList || modePlan;
    if ((checkOpen || activityView || whatsNew || readingLog || pack || box) && !(e.key === "Escape" && box)) return;
    const typing = isTyping(e);
    if (e.key === "/" && !typing && tab === "mods") {
      e.preventDefault();
      document.getElementById("mod-search")?.focus();
      return;
    }
    if (e.key === "Escape" && !typing && (asking.now || importText !== null || snapList || picked.size || selected || modePlan)) {
      if (asking.now) answer(-1);
      else if (importText !== null) importText = null;
      else if (snapList) snapList = null;
      else if (modePlan) modePlan = null;
      else if (picked.size) {
        picked = new Set();
        // the bulk bar goes with them: focus back on the table, not lost to the page
        if ((e.target as HTMLElement).closest?.(".bulk")) tick().then(() => (document.querySelector<HTMLElement>(".list tr.selected") ?? document.querySelector<HTMLElement>(".list tr.mod"))?.focus());
      } else selected = null;
      return;
    }
    // the Maps tab undoes its preview changes too, then calls undo; it has no redo
    if (typing || !(e.ctrlKey || e.metaKey) || tab !== "mods") return;
    const k = e.key.toLowerCase();
    if (k === "a") {
      e.preventDefault();
      selectAll();
    } else if (k === "z" && !e.shiftKey) {
      e.preventDefault();
      undo();
    } else if (k === "y" || (k === "z" && e.shiftKey)) {
      e.preventDefault();
      redo();
    }
  }

  async function toggle(id: string) {
    if (profileLocked()) return false;
    const on = !mods.get(id)?.on;
    const ids = on ? [id] : await withDependants([id]);
    if (!ids) return false;
    return commit(rows.map((r) => (r.kind === "mod" && ids.includes(r.id) ? { ...r, on } : r)), on ? placeAll([id]) : order);
  }

  /** Subscribe to a mod's Workshop item in Steam (Steam keeps it updated), or unsubscribe (Steam deletes its files).
   * Without the Steam client, its Workshop page opens instead. */
  async function subscribe(id: string, on: boolean) {
    const m = mods.get(id);
    const item = m?.source.split(":")[1];
    if (!m || !item || editBusy) return false;
    if (!on && (await ask(t("confirm.unsubscribeTitle", { name: m.name || id }), [t("confirm.unsubscribeLine")], [{ label: t("confirm.unsubscribe"), icon: "x" }])) !== 0) return false;
    if (!(await run(t(on ? "busy.subscribing" : "busy.unsubscribing", { name: m.name || id }), () => api.workshopSubscribe(item, on, m.id, m.name), { showLog: true }))) return api.openUrl(workshopUrl(item)), false;
    // Steam downloads (or deletes) it in the background: look again once it has had a moment
    reloadSoon();
    return true;
  }

  /** A row's checkbox, Space or menu: every picked mod when the row is one of several picked. */
  const toggleRow = async (id: string) => {
    const on = !mods.get(id)?.on;
    if (on && !(await buildFor(batch(id)))) return false;
    return batch(id).length > 1 ? bulkSet(on) : toggle(id);
  };

  /** Before turning on `ids` while playing Build 41: offer Build 42 to the ones only it loads. False when the player cancelled. */
  async function buildFor(ids: string[]) {
    const only42 = ids.filter((id) => rows.some((r) => r.kind === "mod" && r.id === id && r.only42));
    if (view?.build !== 41 || !only42.length) return true;
    const i = await ask(t("confirm.needs42Title", { count: only42.length }), only42.map((id) => mods.get(id)?.name || id),
      [{ label: t("confirm.switch42"), icon: "swap" }, { label: t("confirm.keep41"), icon: "check" }]);
    if (i === 0) return run(t("busy.saving"), () => api.setBuild(42));
    return i === 1;
  }

  /** Before turning `ids` off: when enabled mods need them, ask whether to turn those off too.
   * The ids to turn off, or null when the player cancelled. */
  async function withDependants(ids: string[]): Promise<string[] | null> {
    const deps = dependants(ids, needed);
    if (!deps.length) return ids;
    const name = (id: string) => mods.get(id)?.name || id;
    const lines = deps.map((d) => t("confirm.needsLine", { name: name(d), needs: listed((mods.get(d)?.requires ?? []).filter((r) => ids.includes(r) || deps.includes(r)).map(name)) }));
    const what = ids.length > 3 ? t("confirm.theseMods", { count: ids.length }) : listed(ids.map(name));
    const i = await ask(t("confirm.neededTitle", { count: deps.length, name: what }), lines,
      [{ label: t("confirm.disableToo", { count: deps.length }), icon: "x" }, { label: t("confirm.disableOnly"), icon: "x" }]);
    return i === 0 ? [...ids, ...deps] : i === 1 ? ids : null;
  }

  /** `ids` and the installed mods they need (and those need), as far as they're installed. */
  function withNeeds(ids: string[]) {
    const want = new Set<string>();
    const visit = (id: string) => {
      const m = mods.get(id);
      if (!m || m.missing || want.has(id)) return;
      want.add(id);
      m.requires.forEach(visit);
    };
    ids.forEach(visit);
    return want;
  }

  /** Turn mods on or off from the Maps tab; turning one on also turns on the installed mods it needs. */
  function setMods(ids: string[], on: boolean) {
    const want = on ? withNeeds(ids) : new Set(ids.filter((id) => mods.has(id)));
    const turning = [...want].filter((id) => mods.get(id)!.on !== on);
    return commit(rows.map((r) => (r.kind === "mod" && turning.includes(r.id) ? { ...r, on } : r)), on ? placeAll(turning) : order);
  }

  /** A drag in the load order: the mod at `from` dropped before position `to`. */
  function moveInOrder(from: number, to: number) {
    if (busy || to === from || to === from + 1 || to < 0 || to > order.length) return;
    const next = [...order];
    const [id] = next.splice(from, 1);
    next.splice(to > from ? to - 1 : to, 0, id);
    commit(rows, next);
  }

  /** A drop from the list: a separator carries its whole section. */
  function dropOn(from: number, to: number) {
    if (busy) return;
    if (rows[from]?.kind === "sep") {
      const next = moveSection(rows, from, to);
      if (next) commit(next);
    } else move(from, rows[to]?.kind === "sep" && rows[from]?.kind === "mod" ? to + 1 : to); // a mod dropped on a section's tape goes to the top of that section
  }

  function move(from: number, to: number) {
    if (from === to || to < 0 || to > rows.length) return;
    const next = [...rows];
    const [item] = next.splice(from, 1);
    const at = to > from ? to - 1 : to;
    next.splice(at, 0, item);
    // a mod landing in a collapsed section would vanish: open that section
    if (item.kind === "mod") {
      let s = at;
      while (s >= 0 && next[s].kind !== "sep") s--;
      const name = s >= 0 && next[s].kind === "sep" ? (next[s] as { name: string }).name : "";
      if (name && collapsed.has(name)) toggleSection(name);
    }
    return commit(next);
  }

  async function install(e: Event) {
    e.preventDefault();
    const words = installText.split(/\s+/).filter(Boolean);
    if (!words.length) return;
    // several at once only when each is an ID, a link or owner/repo: else it's one path with spaces in it
    const sources = words.every((w) => /^(\d+|https?:\/\/\S+|[\w.-]+\/[\w.-]+)$/i.test(w)) ? words : [installText.trim()];
    installText = "";
    await installAndSettle(sources.length === 1 ? t("busy.installingOne", { name: sources[0] }) : t("busy.installingMany", { count: sources.length }), () => api.install(sources), sources.length > 1);
  }

  /** A new separator above the selected mod (or at the top), ready to rename. */
  async function addSeparator() {
    const at = Math.max(0, selected ? rows.findIndex((r) => r.kind === "mod" && r.id === selected) : 0);
    const taken = new Set(separators);
    let name = t("mods.newSection");
    for (let n = 2; taken.has(name); n++) name = t("mods.newSectionN", { n });
    await commit([...rows.slice(0, at), { kind: "sep", name }, ...rows.slice(at)]);
    editingSep = at;
  }

  /** Drop a separator. Its mods stay where they are as loose mods (a nameless break ends the section above). */
  function removeSep(i: number) {
    return commit(tidyBreaks(rows.map((r, j) => (j === i ? { kind: "sep", name: "" } : r))));
  }

  /** Keep a break only where it ends a named section and loose mods follow. */
  function tidyBreaks(list: Row[]): Row[] {
    let section = "";
    return list.filter((r, j) => {
      if (r.kind !== "sep") return true;
      if (r.name) return (section = r.name), true;
      const keep = section !== "" && list[j + 1]?.kind === "mod";
      if (keep) section = "";
      return keep;
    });
  }

  function renameSep(i: number, name: string) {
    editingSep = null;
    const r = rows[i];
    if (r?.kind !== "sep") return;
    if (name.trim() === r.name) return;
    if (!name.trim()) return removeSep(i);
    else if (name.trim() !== r.name && separators.includes(name.trim())) say("log.sectionExists", { name: name.trim() }, true);
    else return commit(rows.map((x, j) => (j === i ? { kind: "sep", name: name.trim() } : x)));
  }

  async function submitName(e: Event) {
    e.preventDefault();
    if (busy || profileLocked()) return;
    const [kind, name, from] = [nameFor?.kind, nameFor?.value.trim(), nameFor?.from ?? view?.profile ?? ""];
    nameFor = null;
    if (!name) return;
    if (kind === "new") await run(t("busy.creatingProfile", { name }), () => api.profileNew(name));
    else if (kind === "duplicate") await run(t("busy.copyingProfile", { name }), () => api.profileDuplicate(from, name));
    else if (name !== from) {
      try {
        localStorage.setItem(`pzmm.collapsed.${name}`, localStorage.getItem(`pzmm.collapsed.${from}`) ?? "[]"); // folded sections go along
      } catch {}
      await run(t("busy.renamingProfile", { name }), () => api.profileRename(from, name), { renamed: true });
    }
  }

  // --- right-click menus: a mod row (library or load order), and Browse… ---
  let rowMenu = $state<{ id: string; x: number; y: number; d: ModDetails | null } | null>(null);
  let browseAt = $state<{ x: number; y: number } | null>(null);
  /** The Maps tab's search, for "Show in Maps". */
  let mapsFind = $state("");
  $effect(() => {
    if (tab !== "maps") mapsFind = "";
  });

  function openMenu(id: string, x: number, y: number) {
    if (rowMenu?.id === id) return; // the keyboard's own contextmenu event after Shift+F10
    rowMenu = { id, x, y, d: null };
    api.details(id).then((d) => rowMenu?.id === id && (rowMenu.d = d)).catch(() => {});
  }
  /** Shift+F10 or the menu key on a focused row: its menu, under it. */
  function menuKey(e: KeyboardEvent, id: string) {
    if (e.key !== "ContextMenu" && !(e.shiftKey && e.key === "F10")) return false;
    e.preventDefault();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu(id, r.left + 40, r.bottom);
    return true;
  }
  const rowMenuItems = $derived.by((): MenuItem[] => {
    const m = rowMenu && mods.get(rowMenu.id);
    if (!rowMenu || !m) return [];
    const { id, d } = rowMenu;
    const at = order.indexOf(id);
    const ids = batch(id);
    const many = ids.length > 1;
    const src = describeSource(m.source);
    const github = src.label === "GitHub"; // i18n-ignore: a source kind, not text
    return [
      { label: t(m.on ? "common.disable" : "common.enable"), icon: m.on ? "x" : "check", run: () => toggleRow(id), disabled: editBusy },
      ...(many ? [{ label: t("menu.toTop"), icon: "chevron-up" as IconName, run: () => bulkMove("top"), disabled: editBusy || filtering },
          { label: t("menu.toBottom"), icon: "chevron-down" as IconName, run: () => bulkMove("bottom"), disabled: editBusy || filtering }]
        : [{ label: t("menu.toTop"), icon: "chevron-up" as IconName, run: () => moveInOrder(at, 0), disabled: editBusy || at <= 0 },
          { label: t("menu.toBottom"), icon: "chevron-down" as IconName, run: () => moveInOrder(at, order.length), disabled: editBusy || at < 0 || at === order.length - 1 }]),
      ...(many ? [] : m.pinned ? [{ label: t("menu.unpin"), icon: "lock" as IconName, run: () => pin(id, false), disabled: editBusy }]
        : at >= 0 ? [{ label: t("menu.pin"), icon: "lock" as IconName, run: () => pin(id, true), disabled: editBusy || order.length < 2 }] : []),
      null,
      { label: t(favourites.has(id) ? "menu.unfavourite" : "menu.favourite"), icon: "star", run: () => markAll(ids, "favourite", !favourites.has(id)) },
      { label: t(hidden.has(id) ? "menu.unhide" : "menu.hide"), icon: hidden.has(id) ? "eye" : "eye-off", run: () => markAll(ids, "hidden", !hidden.has(id)) },
      null,
      { label: t("details.showFiles"), icon: "folder", run: () => d && api.openPath(d.dir), disabled: !d },
      ...(src.url ? [{ label: t(github ? "details.githubPage" : "details.workshopPage"), icon: (github ? "external" : "steam") as IconName, run: () => api.openUrl(src.url!) }] : []),
      { label: t("menu.copyId"), icon: "copy", run: () => navigator.clipboard.writeText(id).then(() => say("log.idCopied", { id })).catch(() => {}) },
      { label: t("mods.group"), icon: "layers", run: () => groupLibrary(), disabled: editBusy },
      ...(d?.maps ? [{ label: t("menu.showInMaps"), icon: "search" as IconName, run: () => ((mapsFind = id), (tab = "maps")) }] : []),
      ...(removable(m) ? [null, { label: t("common.remove"), icon: "trash" as IconName, run: () => (picked.size > 1 && picked.has(id) ? bulkRemove() : remove(id)), disabled: editBusy, danger: true }] : []),
    ];
  });

  /** Star or hide a mod: in the game's mod menu and the Workshop mod managers' lists too. */
  async function mark(id: string, kind: "favourite" | "hidden", on: boolean) {
    try {
      marks = await api.modMark(id, kind, on);
    } catch (e) {
      say(...keyedError(e), true);
    }
  }

  async function markAll(ids: string[], kind: "favourite" | "hidden", on: boolean) {
    for (const id of ids) await mark(id, kind, on);
  }

  /** Pin a mod between its neighbours in the load order (Mod Load Order Sorter's rules file, which Autosort reads too), or unpin it. */
  function pin(id: string, on: boolean) {
    if (profileLocked()) return;
    const at = order.indexOf(id);
    const [after, before] = on ? [order.slice(Math.max(0, at - 1), at), order.slice(at + 1, at + 2)] : [[], []];
    run(t("busy.saving"), () => api.modPin(id, after, before), { keepHistory: true }).then((ok) => ok && say(on ? "log.pinned" : "log.unpinned", { id }));
  }

  /** Browse…: install picked .zip files, or a mod folder. */
  async function browseInstall(folder: boolean) {
    const paths = await api.pickInstall(folder).catch((e) => (say(...keyedError(e), true), [] as string[]));
    if (paths.length) await installAndSettle(paths.length === 1 ? t("busy.installingOne", { name: paths[0] }) : t("busy.installingMany", { count: paths.length }), () => api.install(paths), paths.length > 1);
  }

  // --- bulk selection: Ctrl/Cmd+click toggles, Shift+click selects a range ---
  let picked = $state<Set<string>>(new Set());
  let anchor = $state<string | null>(null);

  function rowClick(e: MouseEvent, id: string) {
    if (e.shiftKey && anchor) {
      const ids = visible.filter(({ r }) => r.kind === "mod").map(({ r }) => (r as ModRow).id);
      const [a, b] = [ids.indexOf(anchor), ids.indexOf(id)].sort((x, y) => x - y);
      if (a >= 0) picked = new Set(ids.slice(a, b + 1));
    } else if (e.ctrlKey || e.metaKey) {
      const next = new Set(picked);
      next.has(id) ? next.delete(id) : next.add(id);
      picked = next;
      anchor = id;
    } else {
      picked = new Set([id]);
      anchor = id;
    }
    selected = id;
  }

  /** Bulk Enable / Disable leave favourites as they are. */
  let skipFavourites = $state(false);

  /** Pick every mod shown (Ctrl+A). */
  function selectAll() {
    const ids = visible.flatMap(({ r }) => (r.kind === "mod" ? [r.id] : []));
    picked = new Set(ids);
    anchor = ids[0] ?? null;
  }

  /** The mods an action on `id` applies to: every picked mod when `id` is one of several picked, else just `id`. */
  const batch = (id: string) => (picked.size > 1 && picked.has(id) ? [...picked] : [id]);

  async function bulkSet(on: boolean) {
    let ids = [...picked].filter((id) => !(skipFavourites && favourites.has(id)));
    if (!on) ids = (await withDependants(ids)) ?? [];
    const set = new Set(ids);
    const turning = rows.flatMap((r) => (r.kind === "mod" && set.has(r.id) && r.on !== on ? [r.id] : []));
    return turning.length ? commit(rows.map((r) => (r.kind === "mod" && set.has(r.id) ? { ...r, on } : r)), on ? placeAll(turning) : order) : false;
  }

  /** Set a mod's own tags (PZMM keeps them for every profile). */
  async function setTags(id: string, list: string[]) {
    try {
      marks = await api.modTags(id, list);
    } catch (e) {
      say(...keyedError(e), true);
    }
  }

  /** Move the picked mods (in their current order) to just under separator `to` ("top" / "bottom"). */
  function bulkMove(to: string) {
    const moving = rows.filter((r) => r.kind === "mod" && picked.has(r.id));
    const rest = rows.filter((r) => !(r.kind === "mod" && picked.has(r.id)));
    let at = to === "top" ? 0 : to === "bottom" ? rest.length : rest.findIndex((r) => r.kind === "sep" && r.name === to) + 1;
    if (to !== "top" && to !== "bottom" && at === 0) return;
    commit([...rest.slice(0, at), ...moving, ...rest.slice(at)]);
  }

  function bulkRemove() {
    const ids = [...picked].filter((id) => mods.get(id) && removable(mods.get(id)!));
    if (!ids.length) return say("log.noneOurs", {}, true);
    const skipped = picked.size - ids.length;
    removeMods(ids, skipped ? [t("confirm.removeSkipped", { count: skipped })] : []).then(() => (picked = new Set()));
  }

  /** Remove mods (asking first), and, as Settings say, the required mods nothing else uses. */
  /** Whether it removed them. */
  async function removeMods(ids: string[], notes: string[] = []) {
    if (profileLocked()) return false;
    const s = cfg?.settings;
    const key = `confirm.remove${s?.recycle === false ? "Delete" : WINDOWS ? "Recycle" : "Trash"}${ids.length === 1 ? "One" : "Many"}`;
    // other profiles lose them too
    const others = Object.keys(await api.profilesUsing(ids).catch(() => ({})));
    const usedBy = others.length ? [t("confirm.removeUsedBy", { count: others.length, profiles: listed(others) })] : [];
    if (!(await confirmed(t(key, { count: ids.length, name: ids[0] }), { label: t("common.remove"), icon: "trash" }, [...notes, ...usedBy]))) return false;
    let extra = s?.remove_requirements === "never" ? [] : orphanedRequirements(ids, [...mods.values()]).filter((id) => isOurs(mods.get(id)!));
    if (extra.length && s?.remove_requirements !== "always") {
      const n = extra.length;
      const i = await ask(t("confirm.alsoRemove", { count: n, ids: listed(extra) }), [], [{ label: t("confirm.removeToo", { count: n }), icon: "trash" }, { label: t("confirm.keepThem", { count: n }), icon: "check" }]);
      if (i < 0) return false;
      if (i === 1) extra = [];
    }
    const all = ids.length + extra.length;
    if (all > 1 && !(await snapshot("remove"))) return false;
    return run(all === 1 ? t("busy.removingOne", { name: ids[0] }) : t("busy.removingMany", { count: all }), () => api.remove([...ids, ...extra]));
  }

  const separators = $derived(rows.flatMap((r) => (r.kind === "sep" && r.name ? [r.name] : [])));

  const remove = (id: string) => removeMods([id]);

  /** Delete a profile; the active one only after switching to another of its game. */
  async function deleteProfile(name: string) {
    if (profileLocked() || !name) return;
    if (name !== view?.profile) {
      if (await confirmed(t("confirm.deleteOther", { name }), { label: t("common.delete"), icon: "trash" })) run(t("busy.deletingProfile", { name }), () => api.profileDelete(name));
      return;
    }
    const other = modeProfiles.find((p) => p !== name); // stay in this game: switching game goes through the mode plan
    if (!other) return;
    if (await confirmed(t("confirm.deleteProfile", { name, other }), { label: t("common.delete"), icon: "trash" })) {
      run(t("busy.deletingProfile", { name }), async () => {
        await api.profileUse(other);
        return api.profileDelete(name);
      });
    }
  }

  /** Delete the server profiles whose server is gone from your history, favourites and hosted servers. */
  async function pruneProfiles() {
    if (profileLocked()) return;
    let names: string[];
    try {
      names = await api.profilesUnused();
    } catch (e) {
      return say(...keyedError(e), true);
    }
    if (!names.length) return say("profile.pruneNone");
    if (await confirmed(t("confirm.pruneProfiles", { count: names.length }), { label: t("common.delete"), icon: "trash" }, names)) {
      run(t("busy.pruningProfiles", { count: names.length }), () => api.profilesPrune(names));
    }
  }

  // --- keyboard: arrows move focus, Alt+arrows reorder, Space toggles ---
  async function rowKey(e: KeyboardEvent, i: number, r: Row) {
    const tr = e.currentTarget as HTMLElement;
    if (r.kind === "mod" && menuKey(e, r.id)) return;
    const edits = e.key === " " || e.key === "Delete" || (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown"));
    if (busy && edits) return;
    if (e.key === " " && r.kind === "mod") {
      e.preventDefault();
      toggleRow(r.id);
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && e.altKey && !filtering) {
      e.preventDefault();
      if (r.kind === "sep") {
        // before the section above, or after the one below
        const below = rows.findIndex((x, j) => j > i && x.kind === "sep");
        const above = rows.findLastIndex((x, j) => j < i && x.kind === "sep");
        const next = e.key === "ArrowUp" ? above >= 0 && moveSection(rows, i, above) : below >= 0 && moveSection(rows, i, below + 1);
        if (next) {
          await commit(next);
          (document.querySelector(`[data-index="${rows.findIndex((x) => x.kind === "sep" && x.name === r.name)}"]`) as HTMLElement)?.focus();
        }
        return;
      }
      const to = e.key === "ArrowUp" ? i - 1 : i + 2;
      if (to >= 0 && to <= rows.length) {
        move(i, to);
        await tick();
        (document.querySelector(`[data-index="${e.key === "ArrowUp" ? i - 1 : i + 1}"]`) as HTMLElement)?.focus();
      }
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const sib = (e.key === "ArrowUp" ? tr.previousElementSibling : tr.nextElementSibling) as HTMLElement | null;
      sib?.focus();
    } else if (e.key === "Enter" && r.kind === "mod") {
      selected = r.id;
      picked = new Set([r.id]);
      anchor = r.id;
    } else if (e.key === "Enter" && r.kind === "sep") {
      editingSep = i;
    } else if (e.key === "Delete" && r.kind === "sep") {
      removeSep(i);
    }
  }

  /** Backend lines worth a desktop notification, by the setting that turns them on. */
  const NOTIFY_LINES: Record<string, NotifyKind> = { serverCrashed: "crash", serverCrashRestart: "crash", serverCrashGaveUp: "crash", serverModsUpdated: "restart", favouriteUp: "favourites" };
  /** A desktop notification, when it's on in Settings and PZMM's window isn't in front. */
  async function notify(kind: NotifyKind, text: string) {
    if (document.hasFocus()) return;
    const s = await api.settingsGet().catch(() => null);
    if (s?.settings.notify.includes(kind)) api.notify(text).catch(() => {});
  }

  // the tray menu speaks the UI's language
  $effect(() => {
    api.trayWords(t("tray.open"), t("tray.quit"), t("tray.hidden")).catch(() => {});
  });

  onMount(() => {
    // the splash plays at least this long; the slow first loads start behind it
    const started = Date.now();
    const lift = () => api.splashDone().catch(() => {}); // the splash window closes, this one shows
    setTimeout(lift, 8000); // whatever is still loading carries on in the activity bar
    // the backend's lines: the ones PZMM words come with a key (backend.<key>) to show in the UI's language
    const unlisten = listen<{ text: string; key: string | null; vars: Vars }>("log", ({ payload: l }) => {
      say(l.key ? `backend.${l.key}` : l.text, l.vars, l.text.startsWith("error:"));
      if (l.key && NOTIFY_LINES[l.key]) notify(NOTIFY_LINES[l.key], t(`backend.${l.key}`, l.vars));
    });
    // pzmm://join links: the one PZMM started with, then any opened while it runs
    api.joinLinks().then((l) => l[0] && openLink(l[0])).catch(() => {});
    refreshLists();
    // what the game asked while PZMM was closed; its last report is the starting point, not news
    checkGame(true);
    const unlistenLink = listen<string[]>("join-link", ({ payload }) => payload[0] && openLink(payload[0]));
    const unlistenGame = listen("game-requests", gameAsked);
    fromGameActivity();
    const unlistenActivity = listen("game-activity", fromGameActivity);
    const unlistenBridge = listen("game-bridge", () => ((findingsSent = null), findings && (findings = [...findings]), api.feed(false).catch(() => {})));
    const unlistenList = listen("game-list", reloadList);
    // a Maps check downloads with SteamCMD too: its progress isn't an install's
    const unlistenProgress = listen<[number, number]>("progress", (e) => longJobs && (progress = e.payload));
    // closing with servers PZMM runs: stop them, or leave them running (PZMM finds them again next time)
    // the window's close button: quit or the tray, unless Settings already say which
    const unlistenClose = listen("close-ask", async () => {
      const { i, checked } = await askChecked(t("close.title"), [t("close.line"), t("close.settingsHint")], [{ label: t("close.tray"), icon: "minimize" }, { label: t("close.quit"), icon: "x" }], t("close.dontAsk"));
      if (i < 0) return;
      if (checked) {
        try {
          const s = $state.snapshot(cfg) ?? (await api.settingsGet());
          s.settings.close_ask = false; // and tray says which
          s.settings.tray = i === 0;
          await api.settingsSet(s.settings);
          cfg = s;
        } catch (e) {
          say(...keyedError(e), true);
        }
      }
      api.appClose(i === 0).catch((e) => say(...keyedError(e), true));
    });
    const unlistenQuit = listen<string[]>("servers-running", async ({ payload: names }) => {
      const choices = [{ label: t("hosting.quit.stop"), icon: "x" as IconName }, { label: t("hosting.quit.leave"), icon: "external" as IconName }];
      const i = await ask(t("hosting.quit.title", { count: names.length }), [t("hosting.quit.line", { count: names.length, names: listed(names) })], choices);
      if (i < 0) return;
      pending++;
      busyLabel = t(i === 0 ? "hosting.quit.stopping" : "hosting.quit.leaving");
      api.appQuit(i === 0).catch((e) => {
        say(...keyedError(e), true);
        pending--;
        busyLabel = "";
      });
    });
    const settings = api.settingsGet().catch(() => null);
    loadView().then(async (loaded) => {
      setTimeout(lift, Math.max(0, 2000 - (Date.now() - started)));
      workshopSeen = true; // the Workshop tab loads its first page now (for the build just read), so it opens at once
      api.feed(true).catch(() => {}); // What's new: its change notes, kept on disk for when the tab opens
      if (loaded) whatsNewCheck(!!view?.created); // a first load that failed can't tell a first install
      fromGame(); // what the game asked while PZMM was closed, now there's a profile to apply it to
      refreshStats();
      if (kind) pickKind(kind); // a kind filter left on: its kinds come with the Workshop tags
      resumeBisect();
      cfg = await settings;
      if (cfg?.settings.update_check !== "off") {
        autoCheck();
        glance().then((f) => (feedCount = feedUnseen(f))).catch(() => {});
      }
      if (cfg?.settings.app_updates !== false) checkAppUpdate();
    });
    refreshSave();
    getVersion().then((v) => (appVersion = v));
    return () => {
      unlisten.then((f) => f());
      unlistenProgress.then((f) => f());
      unlistenLink.then((f) => f());
      unlistenGame.then((f) => f());
      unlistenActivity.then((f) => f());
      unlistenBridge.then((f) => f());
      unlistenList.then((f) => f());
      unlistenQuit.then((f) => f());
      unlistenClose.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={globalKey} onfocus={() => (refreshOnFocus(), busy || fromGame())} onblur={() => (blurredAt = Date.now())}
  oncontextmenu={(e) => !(e.target as HTMLElement).closest?.("input:is([type=text], [type=search], [type=url], [type=number], [type=password], :not([type])), textarea, [contenteditable]") && e.preventDefault()} />

<div class="app">
  <header class="top" class:squeeze={squeeze > 0} class:wrap={squeeze > 1} data-tauri-drag-region use:fitTitle>
    <WindowControls />
    <!-- i18n-ignore -->
    <img class="brand" src="/brand/pzmm.png" alt="PZMM" width="44" height="44" />

    <div class="profile">
      {#if nameFor}
        <form onsubmit={submitName} class="inline" use:trap={{ fallback: ".picker .trigger" }}>
          <input type="text" bind:value={nameFor.value} placeholder={t(nameFor.kind === "new" ? "app.newProfilePlaceholder" : "app.profileNamePlaceholder")}
            aria-label={t("app.profileNamePlaceholder")} onkeydown={(e) => e.key === "Escape" && (nameFor = null)} />
          <button type="submit" disabled={busy || !nameFor.value.trim()}>
            <Icon name={nameFor.kind === "new" ? "plus" : nameFor.kind === "rename" ? "check" : "copy"} size={14} />{t(nameFor.kind === "new" ? "app.create" : nameFor.kind === "rename" ? "app.rename" : "app.duplicate")}</button>
          <button type="button" class="quiet" onclick={() => (nameFor = null)}><Icon name="x" size={14} />{t("common.cancel")}</button>
        </form>
      {:else}
        <ProfilePicker {view} {busy} lock={profileLock} onUse={(p) => run(t("busy.switchingProfile"), () => api.profileUse(p))}
          onSwitchGame={planMode} onNew={() => (nameFor = { kind: "new", value: "" })}
          onRename={(p) => (nameFor = { kind: "rename", value: p, from: p })}
          onDuplicate={(p) => (nameFor = { kind: "duplicate", value: t("app.copyName", { name: p }), from: p })} onDelete={deleteProfile} onPrune={pruneProfiles} onSnapshots={openSnapshots} />
        <button class="quiet" onpointerdown={(e) => shareAt && e.stopPropagation()} disabled={busy} title={t("app.shareTip")} aria-label={t("app.share")} aria-haspopup="menu" aria-expanded={!!shareAt}
          onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); shareAt = shareAt ? null : { x: r.left, y: r.bottom + 4 }; }}><Icon name="share" size={14} /><span class="lbl">{t("app.share")}</span></button>
        <button class="quiet" onclick={openImport} disabled={busy || !!profileLock} title={profileLock || t("app.importTip")} aria-label={t("common.import")}><Icon name="upload" size={14} /><span class="lbl">{t("common.import")}</span></button>
        <!-- mods subscribed or copied in while PZMM was open -->
        <button class="quiet" onclick={() => { refreshLists(); loadView(); }} disabled={busy} title={t("app.rescanTip")} aria-label={t("app.rescan")}><Icon name="refresh" size={14} /><span class="lbl">{t("app.rescan")}</span></button>
      {/if}
    </div>

    <form class="install" onsubmit={install}>
      <label for="install" class="visually-hidden">{t("app.installLabel")}</label>
      <input id="install" type="text" bind:value={installText} autocomplete="off" spellcheck="false"
        placeholder={t("app.installPlaceholder")} />
      <button type="button" onpointerdown={(e) => browseAt && e.stopPropagation()} disabled={editBusy} title={profileLock || t("app.browseTip")} aria-label={t("app.browse")} aria-haspopup="menu" aria-expanded={!!browseAt}
        onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); browseAt = browseAt ? null : { x: r.left, y: r.bottom + 4 }; }}><Icon name="folder" size={14} /><span class="lbl">{t("app.browse")}</span></button>
      <!-- one filled button: Update's, when it's beside this in the title row -->
      <button class:primary={!!installText.trim() && !(tight && updates.size)} type="submit" disabled={editBusy || !installText.trim()} title={profileLock}><Icon name="download" size={14} />{t("common.install")}</button>
    </form>
    {@render actions("in-title")}
    {#if filtersAt}
      <Menu x={filtersAt.x} y={filtersAt.y} label={t("mods.filters")} items={filterItems} onClose={() => (filtersAt = null)} />
    {/if}
    {#if sortAt}
      <Menu x={sortAt.x} y={sortAt.y} label={t("mods.sortLabel")} onClose={() => (sortAt = null)}
        items={SORTS.map((v) => ({ label: t(SORT_KEYS[v]), checked: sortBy === v, run: () => (sortBy = v) }))} />
    {/if}
    {#if moreAt}
      <Menu x={moreAt.x} y={moreAt.y} label={t("mods.more")} onClose={() => (moreAt = null)} items={[
        { label: t("mods.selectAll"), icon: "check", run: selectAll, disabled: !visible.length },
        { label: t("mods.addSeparator"), icon: "plus", run: addSeparator, disabled: editBusy || filtering },
        null,
        { heading: t("mods.hint"), hint: true },
      ]} />
    {/if}
    {#if browseAt}
      <Menu x={browseAt.x} y={browseAt.y} label={t("app.browse")} onClose={() => (browseAt = null)}
        items={[{ label: t("app.browseZip"), icon: "download", run: () => browseInstall(false) }, { label: t("app.browseFolder"), icon: "folder", run: () => browseInstall(true) }]} />
    {/if}
    {#if shareAt}
      <Menu x={shareAt.x} y={shareAt.y} label={t("app.share")} onClose={() => (shareAt = null)}
        items={[{ label: t("app.shareCode"), icon: "copy", run: shareProfile }, { label: t("app.saveGamePreset"), icon: "upload", run: () => savePreset("game") }, { label: t("app.saveModManagerPreset"), icon: "upload", run: () => savePreset("modManager") }]} />
    {/if}
    {#if rowMenu}
      <Menu x={rowMenu.x} y={rowMenu.y} label={t("menu.label", { name: mods.get(rowMenu.id)?.name || rowMenu.id })} items={rowMenuItems} onClose={() => (rowMenu = null)} />
    {/if}

  </header>
  <!-- Update and Play: on the tab bar while every tab fits beside them, else in the title row -->
  {#snippet actions(where: "in-tabs" | "in-title")}
    <div class="right" hidden={(where === "in-title") !== tight}>
      {#if updates.size}
        <!-- the one filled button, unless a question above the tabs has its own -->
        <button class:primary={importText === null && !asking.now && !modePlan?.lines.length && !setupAsk && !pack} onclick={() => updateMods([...updates])} disabled={busy}><Icon name="refresh" size={14} />{t("app.updateMods", { count: updates.size })}</button>
      {:else}
        <button onclick={checkUpdatesNow} disabled={busy || checking} title={t("app.checkUpdatesTip")}>
          <Icon name="refresh" size={14} />{checking ? t("app.checking") : t("app.checkUpdates")}</button>
      {/if}
      <button onclick={launch} disabled={busy} title={cfg?.settings.launch_args ? t("app.launchTipArgs", { args: cfg.settings.launch_args }) : t("app.launchTip")}><Icon name="play" size={14} />{t("app.launch")}</button>
    </div>
  {/snippet}

  {#if importText !== null}
    <div class="backdrop" onclick={() => (importText = null)} aria-hidden="true"></div>
    <form class="strip boxed" onsubmit={importProfile} use:trap>
      <label for="import-code">{@html th("app.importIntro")}</label>
      <div class="row">
        <textarea id="import-code" rows="1" bind:value={importText} onkeydown={(e) => { if (e.key === "Escape") importText = null; else if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); e.currentTarget.form?.requestSubmit(); } }} placeholder={t("app.importPlaceholder")} spellcheck="false" style="flex: 1"></textarea>
        <button class="primary" type="submit" disabled={!importText.trim()}><Icon name="upload" size={14} />{t("common.import")}</button>
        <button class="quiet" type="button" onclick={() => (importText = null)}><Icon name="x" size={14} />{t("common.cancel")}</button>
      </div>
      {#if marks.presets.length}
        <div class="row">
          <label for="import-preset">{t("app.importSaved")}</label>
          <select id="import-preset" bind:value={presetPick} style="flex: 1">
            {#each marks.presets as p, i}<option value={i}>{t(`presets.${p.from}`, { name: p.name, count: p.mods.length })}</option>{/each}
          </select>
          <button type="button" onclick={() => { const p = marks.presets[presetPick]; importText = null; if (p) importPreset(p); }}><Icon name="upload" size={14} />{t("common.import")}</button>
        </div>
      {/if}
    </form>
  {/if}

  {#if setupAsk}
    <ServerSetup server={setupAsk.server} ids={setupAsk.ids} items={setupAsk.items} unlisted={setupAsk.unlisted} onAnswer={setupAsk.done} />
  {/if}
  {#if pack}
    <CollectionImport {pack} asProfile={packNew} onImport={importPack} onAdd={addPack} onCancel={() => (pack = null)} />
  {/if}

  {#key asking.now}{#if asking.now}
    <!-- a box in the middle, over everything (the app's windows too); a new question is a new box, so it takes focus -->
    <div class="backdrop asking-backdrop" onclick={() => answer(-1)} aria-hidden="true"></div>
    <div class="strip boxed asking" role="alertdialog" aria-modal="true" aria-label={asking.now.title} use:trap={{ focus: !asking.now.background }}>
      <p><strong>{asking.now.title}</strong></p>
      {#if asking.now.lines.length}<ul class="mono">{#each asking.now.lines as l}<li>{l}</li>{/each}</ul>{/if}
      <div class="row">
        {#if asking.now.check}<label class="check"><input type="checkbox" bind:checked={asking.now.checked} />{asking.now.check}</label>{/if}
        {#each asking.now.choices as c, i}
          <button class={i === 0 ? "primary" : ""} onclick={() => answer(i)}><Icon name={c.icon} size={14} />{c.label}</button>
        {/each}
        <button class="quiet" onclick={() => answer(-1)}><Icon name="x" size={14} />{t("common.cancel")}</button>
      </div>
    </div>
  {/if}{/key}

  {#if snapList}
    <div class="backdrop" onclick={() => (snapList = null)} aria-hidden="true"></div>
    <div class="strip boxed" role="dialog" aria-modal="true" aria-label={t("snapshots.title", { profile: view?.profile ?? "" })} use:trap>
      <p><strong>{t("snapshots.title", { profile: view?.profile ?? "" })}</strong> {t("snapshots.intro")}</p>
      {#if snapList.length}
        <ul class="snaps">
          {#each snapList as s (s.id)}
            <li>
              <span>{fmtDateTime(s.time)} · {t(`snapshots.why.${s.label}`)} <span class="hint-line">· {t("snapshots.counts", { count: s.mods, enabled: fmtNum(s.enabled) })}</span></span>
              <button onclick={() => restoreSnapshot(s.id)} disabled={busy || !!profileLock} title={profileLock}><Icon name="rollback" size={14} />{t("snapshots.restore")}</button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="hint-line">{t("snapshots.none")}</p>
      {/if}
      <div class="row"><button class="quiet" onclick={() => (snapList = null)}><Icon name="x" size={14} />{t("common.close")}</button></div>
    </div>
  {/if}

  {#if modePlan}
    <div class="backdrop" onclick={() => (modePlan = null)} aria-hidden="true"></div>
    <div class="strip boxed" role="dialog" aria-modal="true" aria-label={t("app.switchGameLabel")} use:trap>
      {#if modePlan.error}
        <p class="error" role="alert">{modePlan.error}</p>
        <button class="quiet" onclick={() => (modePlan = null)}><Icon name="x" size={14} />{t("common.close")}</button>
      {:else}
        <p>
          <strong>{t(modePlan.target === "viewpoint" ? "app.switchToViewpoint" : "app.switchToZomboid")}</strong>
          {#if modePlan.target === "viewpoint"}
            {t("app.viewpointNote")}
          {/if}
        </p>
        {#if modePlan.lines.length}
          <ul class="mono">{#each modePlan.lines as l}<li>{t(...keyedError(l))}</li>{/each}</ul>
          <div class="row">
            <button class="primary" onclick={switchMode} disabled={busy}><Icon name="swap" size={14} />{t("app.switch")}</button>
            <button class="quiet" onclick={() => (modePlan = null)}><Icon name="x" size={14} />{t("common.cancel")}</button>
          </div>
        {:else}
          <p class="hint-line">{t("app.checkingNeeded")}</p>
        {/if}
      {/if}
    </div>
  {/if}

  {#if appUpdate && appUpdateLater !== appUpdate.version}
    <div class="app-update" role="status">
      {#if appProgress === null}
        <span>{@html th("app.updateAvailable", { version: appUpdate.version, current: appVersion })}</span>
        <button class="quiet" onclick={() => api.openUrl(`https://github.com/SEVNC3/pz-mod-manager/releases/tag/v${appUpdate?.version}`)}>{t("app.whatsNew")}</button>
        <button class="quiet" onclick={() => (appUpdateLater = appUpdate?.version ?? "")}>{t("app.later")}</button>
        <button onclick={installAppUpdate} disabled={busy} title={busy ? t("app.waitTip") : ""}><Icon name="update" size={14} />{t("app.updateRestart")}</button>
      {:else}
        <span id="app-downloading">{t("app.downloading", { version: appUpdate.version, progress: appProgress })}</span>
        <progress max="100" value={appProgress} aria-labelledby="app-downloading"></progress>
      {/if}
    </div>
  {/if}

  {#if loadError}
    <div class="banner" role="alert">
      <Icon name="alert" size={16} />
      <span>{t("app.loadFailed", { error: loadError })}</span>
      <button onclick={loadView} disabled={busy}><Icon name="refresh" size={14} />{t("common.tryAgain")}</button>
      <button class="quiet" onclick={() => (tab = "settings")}><Icon name="settings" size={14} />{t("app.openFolders")}</button>
    </div>
  {/if}

  <div class="tabbar" class:tight use:fitTabs>
  <div class="tabs-wrap">
  {#each [["l", "chevron-left", -1], ["r", "chevron-right", 1]] as const as [end, icon, dir]}
    {#if ends[end]}
      <!-- the arrow keys move between tabs; the mouse gets these -->
      <button class="chev {end}" tabindex="-1" aria-label={t("app.moreTabs")} title={t("app.moreTabs")}
        onclick={() => tabsEl?.scrollBy({ left: dir * tabsEl.clientWidth * 0.7, behavior: "smooth" })}><Icon name={icon} size={14} />{#if ends[end === "l" ? "la" : "ra"]}<span class="alert"></span>{/if}</button>
    {/if}
  {/each}
  <div class="tabs" role="tablist" aria-label={t("app.sectionsLabel")} tabindex="-1" bind:this={tabsEl} onscroll={tabEnds} onwheel={(e) => e.deltaY && (e.currentTarget.scrollLeft += e.deltaY)} onkeydown={(e) => arrowPick(e, TAB_IDS, tab, (v) => (tab = v as typeof tab))}>
    {#each TAB_IDS as id}
      <!-- one Tab stop: the open tab (the arrow keys move between them) -->
      <button role="tab" aria-selected={tab === id} tabindex={tab === id ? 0 : -1} class:active={tab === id} onclick={() => (tab = id as typeof tab)}>
        {t(`tabs.${id}`)}{#if (id === "zb" && zbNeeded) || (id === "settings" && folderTrouble)}<span class="alert" aria-label={t("app.needsSetup")}></span>{/if}{#if id === "feed" && feedCount}<span class="badge" aria-label={t("feed.countLabel", { count: feedCount })}>{feedCount}</span>{/if}
      </button>
    {/each}
  </div>
  </div>
  {@render actions("in-tabs")}
  <Links />
  </div>

  {#if workshopSeen}
    <Workshop onLoading={(l) => (workshopLoading = l)} hidden={tab !== "workshop"} busy={editBusy} defaultBuild={view?.build ?? null} installed={installedItems} onInstall={installWorkshop} onUninstall={uninstallWorkshop} onCollection={previewCollection} viewpoint={view?.mode === "viewpoint"} />
  {/if}
  <!-- kept mounted once opened, so a server's setup (a long SteamCMD download) and its Stop survive another tab -->
  {#if serversSeen}
    <div class="kept" hidden={tab !== "servers"}>
      {#key serversFor}
        <Servers {busy} lock={profileLock} hidden={tab !== "servers"} profile={view?.profile ?? ""} profiles={view?.profiles ?? []}
          installedIds={[...mods.values()].filter((m) => !m.missing).map((m) => m.id)} {rows} link={joinLink} onSettings={() => (tab = "settings")}
          onSetup={setupServer}
          onImport={async (name) => !profileLocked() && run(t("busy.importingServer", { name }), () => api.serverImport(name), { showLog: true })}
          onCheck={joinCheck} {ask} />
      {/key}
    </div>
  {/if}
  {#if tab === "workshop" || tab === "servers"}
    <!-- kept mounted above -->
  {:else if tab === "home"}
    <Home {view} profiles={modeProfiles} enabled={enabledCount} problems={problemCount} {busy} lock={profileLock} {ask} onLaunch={launch} onGo={(id) => (tab = id as typeof tab)}
      zbNeed={zbNeeded ? { count: zb?.required_by.length ?? 0, partial: !!(zb?.files_current || zb?.launcher_patched) } : null}
      save={homeSave}
      installedIds={[...mods.values()].filter((m) => !m.missing).map((m) => m.id)} onUseProfile={(p) => run(t("busy.switchingProfile"), () => api.profileUse(p))}
      onSetup={setupServer}
      onCheck={joinCheck} onOpenLink={openLink}
      onCheckSetup={openCheck} />
  {:else if tab === "feed"}
    <Feed {mods} {order} {busy} onShowMod={(id) => { selected = id; tab = "mods"; }} onUpdate={(ids) => updateMods(ids)} onCount={feedSeen} />
  {:else if tab === "maps"}
    <!-- one profile's view: a switch starts it afresh (its map order is that profile's) -->
    {#key view?.profile}
      <Maps {busy} profile={view?.profile ?? ""} {mods} loadOrder={order} {say} find={mapsFind} onSetMods={setMods} onInstallWorkshop={installWorkshop}
        onShowMod={(id) => { selected = id; tab = "mods"; }} canUndo={undoStack.length > 0} onUndo={undo} />
    {/key}
  {:else if tab === "saves"}
    {#key view?.profile}
      <Saves {busy} lock={profileLock} profile={view?.profile ?? ""} enabledIds={order} onUseSave={useSave} onSettings={() => (tab = "settings")} {ask} />
    {/key}
  {:else if tab === "zb"}
    <ZombieBuddy status={zb} {view} {updates} {busy} lock={profileLock}
      onSetup={async (vp) => { await run(t("busy.zbSetup"), () => api.zbInstall(vp), { showLog: true }); }}
      onRemove={async () => { await run(t("busy.zbRemove"), api.zbUninstall, { showLog: true }); }}
      onSwitch={() => planMode("viewpoint")} onInstall={() => installWorkshop(VIEWPOINT.workshop)}
      onUpdate={() => updateMods([VIEWPOINT.mod])} onSettings={() => (tab = "settings")} />
  {:else if tab === "authoring"}
    <Authoring />
  {:else if tab === "settings"}
    <Settings bind:cfg {view} {busy} {appVersion} {say} onBuild={(b) => run(t("busy.saving"), () => api.setBuild(b))}
      onRedownload={redownloadAll} onCheckUpdate={() => checkAppUpdate(true)}
      onFoldersChanged={() => loadView().then(() => { refreshSave(); refreshZb(); })} />
  {:else}
  <section class="mods">
  {#if saveDiff && latestSave && saveNoticeDismissed !== saveKey(latestSave)}
    <div class="banner save" role="status">
      <Icon name="alert" size={16} />
      <span>
        {@html th("app.saveDiff", { save: latestSave.name, diffs: new Intl.ListFormat(intlLang(), { type: "conjunction" }).format([saveDiff.missing.length ? t("app.saveMissing", { count: saveDiff.missing.length }) : "", saveDiff.extra.length ? t("app.saveExtra", { count: saveDiff.extra.length }) : ""].filter(Boolean)) })}
      </span>
      <button onclick={() => latestSave && useSave(latestSave)} disabled={busy || !!profileLock} title={profileLock}><Icon name="swap" size={14} />{t("app.useSaveMods")}</button>
      <button class="quiet" onclick={() => latestSave && (saveNoticeDismissed = saveKey(latestSave))}>{t("app.dismiss")}</button>
    </div>
  {/if}
  {#if zbNeeded}
    <div class="banner" role="status">
      <Icon name="alert" size={16} />
      <span>{t(zb?.files_current || zb?.launcher_patched ? "app.zbNeededPartial" : "app.zbNeeded", { count: zb?.required_by.length ?? 0 })}</span>
      <button onclick={() => (tab = "zb")}><Icon name="settings" size={14} />{t("app.zbSetup")}</button>
    </div>
  {/if}
  <div class="filters">
    <input id="mod-search" type="search" bind:value={search} placeholder={t("mods.searchPlaceholder")} aria-label={t("mods.searchLabel")} />
    <div class="seg" role="radiogroup" aria-label={t("mods.showLabel")} tabindex="-1" onkeydown={(e) => arrowPick(e, STATUS_IDS, filter, (v) => (filter = v as typeof filter))}>
      {#each [["all", t("mods.filterAll")], ["on", t("common.enabled")], ["off", t("common.disabled")], ["problems", problemCount ? t("mods.filterProblemsN", { count: problemCount }) : t("mods.filterProblems")]] as [v, label]}
        <button role="radio" aria-checked={filter === v} class:active={filter === v} onclick={() => (filter = v as typeof filter)}>{label}</button>
      {/each}
    </div>
    <button class="quiet" onclick={() => groupLibrary()} disabled={editBusy}><Icon name="layers" size={14} /><span class="lbl">{t("mods.group")}</span></button>
    <button class="quiet" class:on={extraFilters.length} aria-haspopup="menu" aria-expanded={!!filtersAt} onpointerdown={(e) => filtersAt && e.stopPropagation()} title={t("mods.filtersTip")}
      onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); filtersAt = filtersAt ? null : { x: r.left, y: r.bottom + 4 }; }}>
      <Icon name="filter" size={14} /><span class="lbl">{extraFilters.length ? t("mods.filtersN", { count: extraFilters.length }) : t("mods.filters")}</span></button>
    {#each extraFilters as f (f.key)}
      <button class="chip active" onclick={f.clear} title={t("mods.clearFilterTip")}>{f.label}<Icon name="x" size={12} /></button>
    {/each}
    <button class="quiet" aria-haspopup="menu" aria-expanded={!!sortAt} onpointerdown={(e) => sortAt && e.stopPropagation()} title={t("mods.sortLabel")}
      onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); sortAt = sortAt ? null : { x: r.left, y: r.bottom + 4 }; }}>
      <Icon name="sort" size={14} /><span class="lbl">{t(SORT_KEYS[sortBy])}</span></button>
    {#if missingReqs.size}
      <button class="primary" onclick={installMissing} disabled={editBusy}
        title={t("mods.installMissingTip", { ids: listed([...missingReqs]) })}><Icon name="download" size={14} />{t("mods.installMissing", { count: missingReqs.size })}</button>
    {/if}
    {#if fixable}
      <button onclick={fixLoadOrder} disabled={editBusy}
        title={t("mods.fixTip")}><Icon name="wand" size={14} />{t("mods.fix")}</button>
    {/if}
    {#if picked.size >= 2}
      <span class="bulk" role="group" aria-label={t("mods.selectedLabel")}>
        <strong>{t("mods.selected", { count: picked.size })}</strong>
        <button onclick={async () => (await buildFor([...picked])) && bulkSet(true)} disabled={editBusy}><Icon name="check" size={14} />{t("common.enable")}</button>
        <button onclick={() => bulkSet(false)} disabled={editBusy}><Icon name="x" size={14} />{t("common.disable")}</button>
        {#if favourites.size}
          <label class="skip" title={t("mods.skipFavouritesTip")}><input type="checkbox" bind:checked={skipFavourites} />{t("mods.skipFavourites")}</label>
        {/if}
        <select aria-label={t("mods.moveLabel")} disabled={editBusy || filtering}
          onchange={(e) => { const v = e.currentTarget.value; e.currentTarget.value = ""; if (v) bulkMove(v); }}>
          <option value="">{t("mods.moveTo")}</option>
          <option value="top">{t("mods.moveTop")}</option>
          {#each separators as s}<option value={s}>{t("mods.moveUnder", { name: s })}</option>{/each}
          <option value="bottom">{t("mods.moveBottom")}</option>
        </select>
        <button class="danger" onclick={bulkRemove} disabled={editBusy}><Icon name="trash" size={14} />{t("common.remove")}</button>
        <button class="quiet" onclick={() => (picked = new Set())}><Icon name="x" size={14} />{t("common.clear")}</button>
      </span>
    {:else}
      <span class="spacer"></span>
    {/if}
    <span class="tools">
    {#if filtering}
      <button class="quiet" onclick={clearFilters} title={t("mods.hintFiltering")}><Icon name="x" size={14} /><span class="lbl">{t("mods.clearFilters")}</span></button>
    {/if}
    <button class="quiet" onclick={openCheck} disabled={busy || checkingSetup} title={t("mods.checkSetupTip")}><Icon name="pulse" size={14} /><span class="lbl">{checkingSetup ? t("app.checking") : t("mods.checkSetup")}</span></button>
    {#if !selected && rows.length}
      <button class="quiet icon narrow-only" aria-pressed={orderOpen} onclick={() => (orderOpen = !orderOpen)} title={t("loadOrder.title")} aria-label={t("loadOrder.title")}><Icon name="layers" size={16} /></button>
    {/if}
    <button class="quiet icon" onclick={undo} disabled={busy || !undoStack.length} title={t("mods.undoTip")} aria-label={t("mods.undo")}><Icon name="undo" size={16} /></button>
    <button class="quiet icon" onclick={redo} disabled={busy || !redoStack.length} title={t("mods.redoTip")} aria-label={t("mods.redo")}><Icon name="redo" size={16} /></button>
    <button class="quiet icon" aria-haspopup="menu" aria-expanded={!!moreAt} onpointerdown={(e) => moreAt && e.stopPropagation()} title={t("mods.more")} aria-label={t("mods.more")}
      onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); moreAt = moreAt ? null : { x: r.right - 240, y: r.bottom + 4 }; }}><Icon name="more" size={16} /></button>
    </span>
  </div>

  <!-- the search for a broken mod's rounds (Check setup itself is the window above) -->
  <Findings {bisect} {busy} onBisectAnswer={bisectAnswer} onBisectStop={bisectStop} />
  <main class:with-details={selected} class:with-panel={!selected && rows.length} class:order-open={orderOpen}>
    <div class="list blueprint">
      {#if !view}
        <p class="loading">{loadError ? t("app.loadFailed", { error: loadError }) : t("mods.loading")}</p>
      {:else if rows.length === 0}
        <div class="empty">
          <p class="marker">{t("mods.emptyTitle")}</p>
          <p>{t("mods.emptyHint")}</p>
        </div>
      {:else}
        <ModTable {rows} {order} {visible} {sectionOf} {filtering} busy={editBusy} build={view?.build ?? null} {stats} {overlaps} {problems} {updates} {favourites} fresh={freshHere} {hidden} kinds={cats?.kinds ?? {}} neededBy={needed} tags={marks.tags}
          {collapsed} {picked} {selected} bind:dragFrom bind:editingSep onRowClick={rowClick} onRowKey={rowKey} onToggle={toggleRow} onSubscribe={subscribe} onMenu={openMenu}
          onToggleSection={toggleSection} onDrop={dropOn} onRename={renameSep} onRemoveSep={removeSep} />
      {/if}
    </div>

    {#if selected}
      <Details id={selected} {mods} {busy} lock={!!profileLock} hasUpdate={updates.has(selected)} neededBy={needed.get(selected) ?? []} tags={marks.tags[selected] ?? []} {allTags}
        onTags={setTags} onRule={(id, rule) => run(t("busy.saving"), () => api.modRuleSet(id, rule), { keepHistory: true })} onUpdate={(id) => updateMods([id])} onInstallWorkshop={installWorkshop}
        onToggle={toggle} onSelect={(id) => (selected = id)}
        onRemove={remove} onRollback={(id) => run(t("busy.restoring", { id }), () => api.rollback(id))} onClose={() => (selected = null)} />
    {:else if rows.length}
      <LoadOrder {rows} {order} {mods} {problems} {fixable} {overlaps} busy={editBusy} onAutosort={autosortList} onSortAz={sortAz} onFix={fixLoadOrder}
        onMove={moveInOrder} onSelect={(id) => (selected = id)} onMenu={openMenu} onMenuKey={menuKey} />
    {/if}
  </main>
  </section>
  {/if}

  {#if whatsNew}<WhatsNew version={whatsNew} onClose={() => (whatsNew = "")} />{/if}
  <ActivityLog {log} busy={busy || workshopLoading || background.jobs > 0} busyLabel={busyLabel || (workshopLoading ? t("workshop.loadingWorkshop") : "")} {progress} bind:open={logOpen} onCancel={api.installStop} onCopy={copyLog}
    onExpand={() => (activityView = "expand")} onHistory={() => (activityView = "history")} />
  <!-- Check setup opens over any tab (Home's pill, My mods' button) -->
  {#if checkOpen}
    <CheckSetup findings={findings ?? []} {logs} onSelect={(id) => ((checkOpen = false), (tab = "mods"), (selected = id))} failed={!findings && !checkingSetup} checking={checkingSetup} busy={editBusy} {fixes} names={(id) => mods.get(id)?.name || id} onHide={(f, on) => api.findingHide(f, on).catch((e) => say(...keyedError(e), true))}
      onRecheck={() => checkSetup(false)} onReadLog={() => (readingLog = true)} onFindBroken={() => ((checkOpen = false), bisectStart([...new Set(logs.flatMap((l) => [...(l.first?.ids ?? []), ...l.mods.flatMap((m) => (m.id ? [m.id] : []))]))]))}
      onClose={() => (checkOpen = false)} />
  {/if}
  {#if readingLog}<GameLog names={(id) => mods.get(id)?.name || id} onSelect={(id) => ((readingLog = checkOpen = false), (tab = "mods"), (selected = id))} onClose={() => (readingLog = false)} onBisect={busy || bisect ? undefined : bisectStart} />{/if}
  {#if activityView === "expand"}<ActivityModal {log} installed={installedItems} onReinstall={reinstall} onClose={() => (activityView = "")} />{/if}
  {#if activityView === "history"}<DownloadHistory installed={installedItems} onReinstall={reinstall} onRemove={removeRecord} onClose={() => (activityView = "")} />{/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .app > :global(section),
  .kept > :global(section) {
    flex: 1;
    min-height: 0;
  }
  .kept:not([hidden]) {
    display: contents;
  }
  /* banners, filters and findings take what they need; the list gets the rest, and at least enough to use
     (the tab scrolls when long translations and banners leave less) */
  .mods {
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .mods > :global(*) {
    flex: none;
  }
  /* the list and its side panel get the rest: a height of their own, so each scrolls inside it */
  .mods > main {
    flex: 1 1 0;
    min-height: 320px;
  }

  /* one row: tabs that don't fit scroll sideways (wheel, arrow keys, the chevrons), no scrollbar */
  .tabbar {
    display: flex;
    align-items: center;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }
  .tabs {
    display: flex;
    gap: 2px;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  .tabs-wrap {
    position: relative;
    display: flex;
    min-width: 0;
  }
  /* a chevron at an end with more tabs past it, over a fade of the bar */
  .chev {
    position: absolute;
    top: 0;
    bottom: 2px;
    z-index: 1;
    display: flex;
    align-items: center;
    width: 36px;
    padding: 0;
    border: 0;
    border-radius: 0;
    color: var(--text);
  }
  .chev.l {
    left: 0;
    justify-content: flex-start;
    background: linear-gradient(to right, var(--panel) 55%, transparent);
  }
  .chev.r {
    right: 0;
    justify-content: flex-end;
    background: linear-gradient(to left, var(--panel) 55%, transparent);
  }
  .chev:hover {
    color: var(--on-selected);
    box-shadow: none;
  }
  .chev .alert {
    position: absolute;
    top: 6px;
    margin: 0;
  }
  .chev.l .alert {
    left: 12px;
  }
  .chev.r .alert {
    right: 12px;
  }
  .tabs button {
    border: 0;
    border-radius: 4px 4px 0 0; /* the highlight rounded on top, square on the underline */
    background: transparent;
    padding: 9px 14px 8px;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    position: relative;
    flex: none;
    white-space: nowrap;
  }
  .tabs button:hover:not(:disabled),
  .tabs button.active {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-bottom-color: var(--on-selected);
    box-shadow: inset 0 1px 0 var(--hover-line), inset 1px 0 0 var(--hover-line), inset -1px 0 0 var(--hover-line);
  }
  .badge {
    display: inline-block;
    min-width: 18px;
    padding: 0 5px;
    margin-left: 6px;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 11px;
    line-height: 18px;
    text-align: center;
  }
  .alert {
    display: inline-block;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--danger);
    margin-left: 6px;
    vertical-align: 2px;
  }
  .strip {
    padding: 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .strip p {
    margin: 0 0 6px;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: rgb(0 0 0 / 0.45);
  }
  /* above the app's windows (z-index 31) */
  .asking-backdrop {
    z-index: 40;
  }
  /* questions and short forms: a box in the middle over the dimmed window */
  .strip.boxed {
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
    border: 0;
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .strip.asking {
    z-index: 41;
  }
  .boxed p {
    margin-bottom: 10px;
  }
  .boxed .row {
    justify-content: flex-end;
  }
  .strip ul {
    margin: 0 0 8px;
    padding-left: 18px;
    overflow-wrap: anywhere;
  }
  .strip .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  /* a "don't ask again" box sits at the start of the buttons' row */
  .strip .row .check {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-right: auto;
  }
  .strip .snaps {
    list-style: none;
    padding: 0;
  }
  .snaps li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 2px 0;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .hint-line {
    color: var(--muted);
  }
  .app-update {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    background: var(--accent-soft);
    border-bottom: 1px solid var(--line);
  }
  .app-update span {
    flex: 1;
  }
  .app-update progress {
    accent-color: var(--accent-text);
    width: 220px;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 16px;
    background: color-mix(in oklab, var(--danger) 12%, var(--panel));
    border-bottom: 1px solid var(--line);
  }
  .banner.save {
    background: color-mix(in oklab, var(--accent) 12%, var(--panel));
  }
  .banner span {
    flex: 1;
  }

  /* --- top bar --- */
  /* the top bar is the title bar: drag it to move the window, double-click to maximise */
  .top {
    position: relative;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 150px 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .brand {
    width: 44px;
    height: 44px;
    margin: -6px 0;
  }
  .profile,
  .inline {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .right {
    display: flex;
    flex: none;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  .right[hidden],
  .narrow-only {
    display: none;
  }
  /* one row: the install box gives way first (down to 80px), then the buttons' labels (fitTitle), and only
     then does the row wrap */
  .install {
    display: flex;
    gap: 6px;
    flex: 1 1 260px;
  }
  .top.squeeze .lbl {
    display: none;
  }
  .top.wrap {
    flex-wrap: wrap;
  }
  .install input {
    flex: 1;
    width: 0;
    min-width: 80px;
  }
  .profile,
  .install button {
    flex: none;
  }

  /* --- filters --- */
  .filters {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .filters input[type="search"] {
    width: 180px;
  }
  .seg button {
    padding: 5px 12px;
  }
  .filters {
    container-type: inline-size;
  }
  .filters .on {
    color: var(--accent-text);
  }
  .spacer {
    margin-left: auto;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px 4px 10px;
  }
  /* a narrow window: the labels go, the buttons and their tooltips stay */
  @container (max-width: 980px) {
    .filters .lbl {
      display: none;
    }
    .filters input[type="search"] {
      width: 150px;
    }
  }
  .chip.active {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-color: var(--hover-line);
  }
  .skip {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
  }
  .bulk {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    flex-wrap: wrap;
  }

  /* --- list + details --- */
  main {
    display: grid;
    grid-template-columns: 1fr;
    flex: 1;
    min-height: 200px;
  }
  main.with-details,
  main.with-panel {
    grid-template-columns: 1fr minmax(300px, 380px);
  }
  .list {
    overflow-y: auto;
    min-height: 0;
    padding: 14px 16px 0;
    container-type: inline-size; /* the table drops columns by its own width */
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
  }
  .icon {
    padding: 4px 8px;
    font-size: 16px;
    line-height: 1;
  }
  .loading {
    padding: 24px 16px;
    color: var(--muted);
  }
  .empty {
    max-width: 460px;
    margin: 12vh auto 0;
    text-align: center;
    color: var(--muted);
    padding: 0 16px;
  }
  .marker {
    font-family: var(--font-marker);
    font-size: 26px;
    color: var(--text);
    margin-bottom: 4px;
  }

  /* strips above the list (banners, plans, app update; findings and settings too) are cards like everything else */
  .banner,
  .strip,
  .app-update {
    margin: 10px 16px 0;
    border-bottom: 0;
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  /* the update strip sits above the tabs: the same gap below it as above */
  .app-update {
    margin-bottom: 10px;
  }
  .banner > :global(.icon) {
    flex: none;
    color: var(--danger);
  }

  @media (max-width: 900px) {
    .narrow-only {
      display: inline-flex;
    }
    main.with-panel {
      grid-template-columns: 1fr;
    }
    /* one at a time, not stacked: the list, or the load order (its button), or a mod's details (Close) */
    main.with-panel:not(.order-open) :global(.panel),
    main.with-panel.order-open .list,
    main.with-details .list {
      display: none;
    }
    main.with-details {
      grid-template-columns: 1fr;
    }
  }
  /* a short window: one scroll for the tab, not a list scrolling inside it */
  @media (max-height: 600px) {
    .mods > main {
      flex: none;
      min-height: auto;
    }
    .list {
      overflow: visible;
    }
  }
  .tight .tabs button {
    padding: 9px 9px 8px;
  }
</style>
