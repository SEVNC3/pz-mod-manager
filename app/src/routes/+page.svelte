<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getVersion } from "@tauri-apps/api/app";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { decodeShare, encodeShare } from "$lib/api";
  import { api, zbReady, type Mode, type ModRow, type Row, type View, type ZbStatus } from "$lib/api";
  import Details from "$lib/Details.svelte";
  import Workshop from "$lib/Workshop.svelte";
  import ZombieBuddy from "$lib/ZombieBuddy.svelte";
  import ServersSaves from "$lib/ServersSaves.svelte";
  import Maps from "$lib/Maps.svelte";
  import Links from "$lib/Links.svelte";
  import Icon from "$lib/Icon.svelte";
  import ActivityLog from "$lib/ActivityLog.svelte";
  import Findings from "$lib/Findings.svelte";
  import Settings from "$lib/Settings.svelte";
  import { applyPrefs } from "$lib/prefs.svelte";
  import ModTable from "$lib/ModTable.svelte";
  import { autosort, bisectRound, findProblems, groupByCategory, loadList, moveSection, orderOf, orphanedRequirements, placeInOrder, placeInSections, sortLoadOrder, syncOrder, tiers } from "$lib/order";
  import LoadOrder from "$lib/LoadOrder.svelte";
  import WindowControls from "$lib/WindowControls.svelte";
  import ProfilePicker from "$lib/ProfilePicker.svelte";
  import type { Categories, Finding, ModErrors, Overlap, Save, SettingsView, Stats } from "$lib/api";
  import { t, th, type Vars } from "$lib/i18n.svelte";

  let tab = $state<"mods" | "workshop" | "maps" | "servers" | "zb" | "settings">("mods");
  let updates = $state<Set<string>>(new Set());
  /** settings.json, folders and the rest of the Settings tab (null until read). */
  let cfg = $state<SettingsView | null>(null);
  $effect(applyPrefs);

  // shared files between enabled mods: who overrides whom (refreshed after every change)
  let overlaps = $state<Record<string, Overlap>>({});
  const refreshOverlaps = () => api.conflictSummary().then((o) => (overlaps = o)).catch(() => {});

  // --- PZMM's own updates (signed releases on GitHub) ---
  let appVersion = $state("");
  let appUpdate = $state.raw<Update | null>(null);
  let appProgress = $state<number | null>(null); // 0..100 while downloading

  async function checkAppUpdate(manual = false) {
    try {
      appUpdate = await check();
      if (!appUpdate && manual) say("log.appLatest", { version: appVersion });
    } catch (e) {
      if (manual) say("log.appCheckFailed", { error: String(e) }, true);
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
      say("log.appUpdateFailed", { error: String(e) }, true);
      appProgress = null;
    }
  }
  let appUpdateLater = $state(false);
  let workshopSeen = $state(false);
  $effect(() => {
    if (tab === "workshop") workshopSeen = true;
  });
  let zb = $state<ZbStatus | null>(null);
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
    (view?.profiles ?? []).filter((p) => p === view?.profile || (view?.kinds[p] ?? "zomboid") === (view?.mode ?? "zomboid")),
  );

  /** Show what switching game does; `profile` is the one picked in the profile menu, if any. */
  async function planMode(target: Mode, profile?: string) {
    if (!view || target === view.mode) return;
    modePlan = { target, profile, lines: [], error: "" };
    try {
      const lines = await api.modePreview(target);
      if (modePlan?.target === target) modePlan.lines = lines;
    } catch (e) {
      if (modePlan?.target === target) modePlan.error = String(e);
      else say(String(e), {}, true);
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
    await settleOrder();
  }
  let selected = $state<string | null>(null);
  /** Runs in flight: busy until every one has finished. */
  let pending = $state(0);
  const busy = $derived(pending > 0);
  let busyLabel = $state("");
  let log = $state<{ text: string; en: string; error: boolean }[]>([]);
  let logOpen = $state(false);
  let search = $state("");
  let filter = $state<"all" | "on" | "off" | "problems">("all");
  let installText = $state("");
  let newProfile = $state<string | null>(null);
  let editingSep = $state<number | null>(null);
  let dragFrom = $state<number | null>(null);

  // --- derived ---
  const mods = $derived(new Map(rows.filter((r): r is ModRow => r.kind === "mod").map((r) => [r.id, r])));
  const enabledCount = $derived([...mods.values()].filter((m) => m.on).length);
  /** Every mod, enabled ones first in load order: load rules are checked and sorted on this. */
  const list = $derived(loadList(rows, order));
  const problems = $derived(findProblems(list));
  const fixable = $derived([...problems.values()].filter((p) => p.fixable).length);
  const missingReqs = $derived.by(() => {
    const out = new Set<string>();
    for (const m of mods.values()) if (m.on && !m.missing) for (const r of m.requires) if (!mods.has(r)) out.add(r);
    return out;
  });
  const problemCount = $derived(problems.size + [...mods.values()].filter((m) => m.on && m.missing).length);
  // --- columns: size and last change, loaded after the list (they walk every file) ---
  let stats = $state<Record<string, Stats>>({});
  const refreshStats = () => api.modStats().then((s) => (stats = s)).catch(() => {});
  let sortBy = $state<"order" | "name" | "size" | "updated" | "status">("order");

  const filtering = $derived(search.trim() !== "" || filter !== "all" || sortBy !== "order");
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
        return !q || r.id.toLowerCase().includes(q) || r.name.toLowerCase().includes(q);
      })
      .sort((a, b) => {
        if (sortBy === "order" || a.r.kind !== "mod" || b.r.kind !== "mod") return 0;
        const [x, y] = [a.r, b.r];
        const name = (m: ModRow) => (m.name || m.id).toLowerCase();
        if (sortBy === "name") return name(x).localeCompare(name(y));
        if (sortBy === "size") return (stats[y.id]?.size ?? 0) - (stats[x.id]?.size ?? 0);
        if (sortBy === "updated") return (stats[y.id]?.modified ?? 0) - (stats[x.id]?.modified ?? 0);
        const rank = (m: ModRow) => (m.missing || problems.has(m.id) ? 0 : updates.has(m.id) ? 1 : 2);
        return rank(x) - rank(y) || name(x).localeCompare(name(y));
      });
  });

  /** Back from the game: reload, so changes made in its mod menu show before anything is saved over them. */
  let lastFocusLoad = 0;
  async function refreshOnFocus() {
    if (busy || dragFrom !== null || editingSep !== null || Date.now() - lastFocusLoad < 3000) return;
    lastFocusLoad = Date.now();
    try {
      const v = await api.load();
      if (busy) return;
      view = v;
      rows = v.rows;
      order = v.order;
      if (v.adopted) {
        undoStack = redoStack = [];
        say("log.adopted", { profile: v.profile });
      }
    } catch {}
  }

  // --- actions ---
  /** An activity line: a key from the locale files, or text as is (the backend's). Its English text picks the icon. */
  function say(key: string, vars: Vars = {}, error = false) {
    log.push({ text: t(key, vars), en: t(key, vars, "en"), error });
    if (error) logOpen = true;
  }

  /**
   * Run a backend job that returns the new view. Errors go to the activity log; returns success.
   * `keepHistory` for list edits (undoable, no new files); `showLog` opens the log for long jobs.
   */
  async function run(label: string, job: () => Promise<View>, { keepHistory = false, showLog = false } = {}): Promise<boolean> {
    if (showLog) logOpen = true;
    // undo covers list edits only: anything else (installs, profile switches) resets it
    if (!keepHistory) undoStack = redoStack = [];
    const before = new Set(rows.flatMap((r) => (r.kind === "mod" ? [r.id] : [])));
    const knownProfiles = view?.profiles;
    pending++;
    busyLabel = label;
    try {
      const v = await job();
      view = v;
      rows = v.rows;
      order = v.order;
      if (v.adopted) say("log.adopted", { profile: v.profile });
      if (selected && !v.rows.some((r) => r.kind === "mod" && r.id === selected)) selected = null;
      // a profile seen for the first time (first run, New, Import, Viewpoint) without separators gets autosorted
      const fresh = v.created || (knownProfiles && !knownProfiles.includes(v.profile));
      if (fresh && cfg?.settings.autosort_new !== false && v.rows.length && !v.rows.some((r) => r.kind === "sep")) await groupLibrary(false);
      else if (!keepHistory && before.size) await placeNew(before);
      return true;
    } catch (e) {
      say(String(e), {}, true);
      // resync with what's on disk
      try {
        view = await api.load();
        rows = view.rows;
        order = view.order;
      } catch {}
      return false;
    } finally {
      pending--;
      if (!pending) busyLabel = "";
      refreshZb();
      refreshOverlaps();
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

  /** Autosort the load order: needed mods on, frameworks first, translations last, every load rule satisfied. */
  async function autosortList() {
    let c: Categories;
    try {
      c = await categories();
    } catch (e) {
      // no tags (offline, rate-limited): still fix the load rules
      if (await commitList(sortLoadOrder(list))) say("log.tagsFailedSorted", { error: String(e) }, true);
      return;
    }
    if (await commitList(autosort(list, c.of, c.order))) say("log.autosorted");
  }

  /** Group the library into one section per category. `record` keeps it undoable (and asks before replacing your own sections). */
  async function groupLibrary(record = true) {
    let c: Categories;
    try {
      c = await categories();
    } catch (e) {
      return say("log.tagsFailed", { error: String(e) }, true);
    }
    const own = separators.filter((s) => !c.order.includes(s));
    if (record && own.length && !confirm(t("confirm.group", { sections: own.slice(0, 4).join(", ") + (own.length > 4 ? ", …" : "") }))) return;
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
    commitList(sortLoadOrder(list));
  }

  async function checkUpdates() {
    try {
      updates = new Set(await api.checkUpdates());
      if (updates.size) say("log.updatesFound", { count: updates.size });
    } catch (e) {
      say("log.updatesCheckFailed", { error: String(e) });
    }
  }

  /** The scheduled check (Settings: on start / every few hours), installing what it finds if asked to. */
  async function autoCheck() {
    if (busy) return;
    await checkUpdates();
    if (updates.size && cfg?.settings.update_action === "install") await updateMods([...updates]);
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
    const ours = [...mods.values()].filter((m) => /^(workshop|github|local):/.test(m.source)).map((m) => m.id);
    if (!ours.length) return say("log.nothingToRedownload");
    if (!confirm(t("confirm.redownload", { count: ours.length }))) return;
    await updateMods(ours);
  }

  async function updateMods(ids: string[]) {
    const inSave = latestSave ? ids.filter((i) => latestSave!.mods.includes(i)) : [];
    if (
      inSave.length &&
      !confirm(t("confirm.updateInSave", { count: inSave.length, mods: inSave.join(", "), save: latestSave!.name }))
    )
      return;
    if (await run(ids.length === 1 ? t("busy.updatingOne", { name: ids[0] }) : t("busy.updatingMany", { count: ids.length }), () => api.updateMods(ids), { showLog: true })) {
      updates = new Set([...updates].filter((u) => !ids.includes(u)));
    }
  }

  /** After installing requirements, move them above the mods that need them. */
  async function settleOrder() {
    await tick();
    if (fixable) fixLoadOrder();
  }

  /** Every install: run it with the log open, place what it enabled (the backend appends it) like Autosort would, then settle the order. */
  async function installAndSettle(label: string, job: () => Promise<View>) {
    const before = new Set(order);
    const ok = await run(label, job, { showLog: true });
    const added = order.filter((id) => !before.has(id));
    if (ok && added.length) await commit(rows, placeAll(added), false);
    if (ok) await settleOrder();
    return ok;
  }

  const installWorkshop = (id: string) => installAndSettle(t("busy.installingWorkshop", { id }), () => api.install([id]));

  async function installMissing() {
    let unresolved: string[] = [];
    const ok = await installAndSettle(t("busy.findingMissing", { count: missingReqs.size }), async () => {
      const [v, left] = await api.installMissing();
      unresolved = left;
      return v;
    });
    if (ok && unresolved.length) say("log.stillMissing", { ids: unresolved.join(", ") }, true);
  }

  // --- save safety: the latest save vs the active profile ---
  let latestSave = $state<Save | null>(null);
  let saveNoticeDismissed = $state(false);
  const refreshSave = () =>
    api.serversAndSaves().then((d) => (latestSave = d.saves[0] ?? null)).catch(() => {});
  const saveDiff = $derived.by(() => {
    if (!latestSave) return null;
    const on = new Set([...mods.values()].filter((m) => m.on).map((m) => m.id));
    const theirs = new Set(latestSave.mods);
    const missing = latestSave.mods.filter((m) => !on.has(m));
    const extra = [...on].filter((m) => !theirs.has(m));
    return missing.length || extra.length ? { missing, extra } : null;
  });

  // --- share a profile ---
  let importText = $state<string | null>(null);

  async function shareProfile() {
    try {
      await navigator.clipboard.writeText(encodeShare(await api.profileExport()));
      say("log.shareCopied", { profile: view?.profile ?? "" });
      logOpen = true;
    } catch (e) {
      say("log.shareFailed", { error: String(e) }, true);
    }
  }

  async function importProfile(e: Event) {
    e.preventDefault();
    const text = importText ?? "";
    importText = null;
    try {
      const shared = decodeShare(text);
      await run(t("busy.importing", { name: shared.name }), () => api.profileImport(shared), { showLog: true });
    } catch (err) {
      say(String(err), {}, true);
    }
  }

  // --- health check ---
  let findings = $state<Finding[] | null>(null);
  let gameErrors = $state<ModErrors[]>([]);
  async function checkSetup() {
    try {
      [findings, gameErrors] = await Promise.all([api.doctor(), api.consoleErrors().catch(() => [])]);
    } catch (e) {
      say("log.checkSetupFailed", { error: String(e) }, true);
    }
  }

  // --- find a broken mod by halving the suspects each game run ---
  let bisect = $state<{ original: Lists; suspects: string[]; testing: string[]; round: number } | null>(null);

  /** Enabled = everything not under suspicion + the half being tested + what those require. */
  function bisectApply(b: NonNullable<typeof bisect>) {
    const { on, testing } = bisectRound(b.original.rows, b.suspects, (id) => mods.get(id)?.requires ?? []);
    bisect = { ...b, testing };
    const o = placeAll([...on].filter((id) => !b.original.order.includes(id)), b.original.order); // a requirement that was off
    commit(b.original.rows.map((r) => (r.kind === "mod" ? { ...r, on: on.has(r.id) } : r)), o, false);
  }

  function bisectStart() {
    const suspects = [...mods.values()].filter((m) => m.on && !m.missing).map((m) => m.id);
    if (suspects.length < 2) return say("log.bisectNeedsTwo", {}, true);
    findings = null;
    undoStack = redoStack = []; // undoing into a half-disabled test list would be a trap
    bisectApply({ original: $state.snapshot({ rows, order }), suspects, testing: [], round: 1 });
  }

  function bisectAnswer(stillBroken: boolean) {
    if (!bisect) return;
    const suspects = stillBroken ? bisect.testing : bisect.suspects.filter((s) => !bisect!.testing.includes(s));
    if (suspects.length <= 1) {
      const original = bisect.original;
      bisect = null;
      undoStack = redoStack = [];
      commit(original.rows, original.order, false);
      say(suspects.length ? "log.bisectFound" : "log.bisectNone", { id: suspects[0] ?? "" }, !!suspects.length);
      logOpen = true;
      if (suspects.length) selected = suspects[0];
      return;
    }
    bisectApply({ ...bisect, suspects, round: bisect.round + 1 });
  }

  function bisectStop() {
    if (!bisect) return;
    const original = bisect.original;
    bisect = null;
    undoStack = redoStack = [];
    commit(original.rows, original.order, false);
  }

  // --- undo/redo for list edits (toggle, reorder, separators, fixes) in the library and the load order ---
  type Lists = { rows: Row[]; order: string[] };
  let undoStack = $state<Lists[]>([]);
  let redoStack = $state<Lists[]>([]);

  /** Save the library and the load order (kept to the library's enabled mods). */
  const commit = (next: Row[], nextOrder = order, record = true) => {
    if (record) {
      undoStack = [...undoStack.slice(-49), $state.snapshot({ rows, order })];
      redoStack = [];
    }
    rows = next;
    order = syncOrder(next, nextOrder);
    return run(t("busy.saving"), () => api.save(rows, order), { keepHistory: true });
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

  const TAB_IDS = ["mods", "workshop", "maps", "servers", "zb", "settings"];
  const FILTER_IDS = ["all", "on", "off", "problems"];
  /** Left/Right arrows step through a tab list or segmented control and move focus along. */
  function arrowPick(e: KeyboardEvent, ids: string[], current: string, set: (v: string) => void) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const i = (ids.indexOf(current) + (e.key === "ArrowRight" ? 1 : ids.length - 1)) % ids.length;
    set(ids[i]);
    const buttons = (e.currentTarget as HTMLElement).querySelectorAll("button");
    tick().then(() => (buttons[i] as HTMLElement | undefined)?.focus());
  }

  function globalKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
    if (e.key === "/" && !typing && tab === "mods") {
      e.preventDefault();
      document.getElementById("mod-search")?.focus();
      return;
    }
    if (e.key === "Escape" && !typing && (picked.size || selected || modePlan)) {
      if (modePlan) modePlan = null;
      else if (picked.size) picked = new Set();
      else selected = null;
      return;
    }
    if (typing || !(e.ctrlKey || e.metaKey) || (tab !== "mods" && tab !== "maps")) return;
    const k = e.key.toLowerCase();
    if (k === "z" && !e.shiftKey) {
      if (tab === "maps") return; // the Maps tab undoes its preview changes too, then calls undo
      e.preventDefault();
      undo();
    } else if (k === "y" || (k === "z" && e.shiftKey)) {
      e.preventDefault();
      redo();
    }
  }

  function toggle(id: string) {
    const on = !mods.get(id)?.on;
    commit(rows.map((r) => (r.kind === "mod" && r.id === id ? { ...r, on } : r)), on ? placeAll([id]) : order);
  }

  /** Turn mods on or off from the Maps tab; turning one on also turns on the installed mods it needs. */
  function setMods(ids: string[], on: boolean) {
    const want = new Set<string>();
    const visit = (id: string) => {
      const m = mods.get(id);
      if (!m || m.missing || want.has(id)) return;
      want.add(id);
      if (on) m.requires.forEach(visit);
    };
    ids.forEach(visit);
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
    commit(next);
  }

  async function install(e: Event) {
    e.preventDefault();
    const sources = installText.split(/\s+/).filter(Boolean);
    if (!sources.length) return;
    installText = "";
    await installAndSettle(sources.length === 1 ? t("busy.installingOne", { name: sources[0] }) : t("busy.installingMany", { count: sources.length }), () => api.install(sources));
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
    commit(tidyBreaks(rows.map((r, j) => (j === i ? { kind: "sep", name: "" } : r))));
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
    if (!name.trim()) removeSep(i);
    else if (name.trim() !== r.name && separators.includes(name.trim())) say("log.sectionExists", { name: name.trim() }, true);
    else commit(rows.map((x, j) => (j === i ? { kind: "sep", name: name.trim() } : x)));
  }

  async function createProfile(e: Event) {
    e.preventDefault();
    const name = newProfile?.trim();
    newProfile = null;
    if (name) await run(t("busy.creatingProfile", { name }), () => api.profileNew(name));
  }

  // --- bulk selection: Ctrl/Cmd+click toggles, Shift+click selects a range ---
  let picked = $state<Set<string>>(new Set());
  let anchor = $state<string | null>(null);
  const isOurs = (m: ModRow) => /^(workshop|github|local):/.test(m.source);

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

  function bulkSet(on: boolean) {
    const turning = rows.flatMap((r) => (r.kind === "mod" && picked.has(r.id) && r.on !== on ? [r.id] : []));
    commit(rows.map((r) => (r.kind === "mod" && picked.has(r.id) ? { ...r, on } : r)), on ? placeAll(turning) : order);
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
    const ids = [...picked].filter((id) => mods.get(id) && isOurs(mods.get(id)!));
    if (!ids.length) return say("log.noneOurs", {}, true);
    const skipped = picked.size - ids.length;
    removeMods(ids, skipped ? t("confirm.removeSkipped", { count: skipped }) : "").then(() => (picked = new Set()));
  }

  /** Remove mods (asking first), and, as Settings say, the required mods nothing else uses. */
  async function removeMods(ids: string[], note = "") {
    const s = cfg?.settings;
    const ask = `confirm.remove${s?.recycle === false ? "Delete" : "Recycle"}${ids.length === 1 ? "One" : "Many"}`;
    if (!confirm(t(ask, { count: ids.length, name: ids[0] }) + note)) return;
    let extra = s?.remove_requirements === "never" ? [] : orphanedRequirements(ids, [...mods.values()]).filter((id) => isOurs(mods.get(id)!));
    if (extra.length && s?.remove_requirements !== "always" &&
      !confirm(t("confirm.alsoRemove", { count: extra.length, ids: extra.join(", ") }))) extra = [];
    const all = ids.length + extra.length;
    await run(all === 1 ? t("busy.removingOne", { name: ids[0] }) : t("busy.removingMany", { count: all }), () => api.remove([...ids, ...extra]));
  }

  const separators = $derived(rows.flatMap((r) => (r.kind === "sep" && r.name ? [r.name] : [])));

  const remove = (id: string) => removeMods([id]);

  function deleteProfile() {
    const name = view?.profile;
    const other = modeProfiles.find((p) => p !== name); // stay in this game: switching game goes through the mode plan
    if (!name || !other) return;
    if (confirm(t("confirm.deleteProfile", { name, other }))) {
      run(t("busy.deletingProfile", { name }), async () => {
        await api.profileUse(other);
        return api.profileDelete(name);
      });
    }
  }

  // --- keyboard: arrows move focus, Alt+arrows reorder, Space toggles ---
  async function rowKey(e: KeyboardEvent, i: number, r: Row) {
    const tr = e.currentTarget as HTMLElement;
    const edits = e.key === " " || e.key === "Delete" || (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown"));
    if (busy && edits) return;
    if (e.key === " " && r.kind === "mod") {
      e.preventDefault();
      toggle(r.id);
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && e.altKey && !filtering) {
      e.preventDefault();
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

  onMount(() => {
    const unlisten = listen<string>("log", (e) => say(e.payload, {}, e.payload.startsWith("error:")));
    const settings = api.settingsGet().catch(() => null);
    run(t("busy.loading"), api.load).then(async () => {
      refreshStats();
      cfg = await settings;
      if (cfg?.settings.update_check !== "off") autoCheck();
      if (cfg?.settings.app_updates !== false) checkAppUpdate();
    });
    refreshSave();
    getVersion().then((v) => (appVersion = v));
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={globalKey} onfocus={refreshOnFocus} />

<div class="app">
  <header class="top" data-tauri-drag-region>
    <WindowControls />
    <!-- i18n-ignore -->
    <img class="brand" src="/brand/pzmm.png" alt="PZMM" width="44" height="44" />

    <div class="profile">
      {#if newProfile !== null}
        <form onsubmit={createProfile} class="inline">
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" bind:value={newProfile} placeholder={t("app.newProfilePlaceholder")} autofocus
            onkeydown={(e) => e.key === "Escape" && (newProfile = null)} />
          <button type="submit" disabled={!newProfile.trim()}><Icon name="plus" size={14} />{t("app.create")}</button>
        </form>
      {:else}
        <ProfilePicker {view} {busy} onUse={(p) => run(t("busy.switchingProfile"), () => api.profileUse(p))}
          onSwitchGame={planMode} onNew={() => (newProfile = "")} />
        <button class="quiet" onclick={shareProfile} disabled={busy} title={t("app.shareTip")}><Icon name="share" size={14} />{t("app.share")}</button>
        <button class="quiet" onclick={() => (importText = "")} disabled={busy} title={t("app.importTip")}><Icon name="upload" size={14} />{t("common.import")}</button>
        {#if modeProfiles.length > 1}
          <button class="quiet" onclick={deleteProfile} disabled={busy}><Icon name="trash" size={14} />{t("common.delete")}</button>
        {/if}
      {/if}
    </div>

    <form class="install" onsubmit={install}>
      <label for="install" class="visually-hidden">{t("app.installLabel")}</label>
      <input id="install" type="text" bind:value={installText} autocomplete="off" spellcheck="false"
        placeholder={t("app.installPlaceholder")} />
      <button class="primary" type="submit" disabled={busy || !installText.trim()}><Icon name="download" size={14} />{t("common.install")}</button>
    </form>

  </header>
  {#snippet actions()}
    <div class="right">
      {#if updates.size}
        <button class="primary" onclick={() => updateMods([...updates])} disabled={busy}><Icon name="refresh" size={14} />{t("app.updateMods", { count: updates.size })}</button>
      {:else}
        <button onclick={checkUpdatesNow} disabled={busy || checking} title={t("app.checkUpdatesTip")}>
          <Icon name="refresh" size={14} />{checking ? t("app.checking") : t("app.checkUpdates")}</button>
      {/if}
      <button onclick={api.launchGame} title={cfg?.settings.launch_args ? t("app.launchTipArgs", { args: cfg.settings.launch_args }) : t("app.launchTip")}><Icon name="play" size={14} />{t("app.launch")}</button>
    </div>
  {/snippet}

  {#if importText !== null}
    <form class="mode-plan" onsubmit={importProfile}>
      <label for="import-code">{@html th("app.importIntro")}</label>
      <div class="row">
        <!-- svelte-ignore a11y_autofocus --><!-- i18n-ignore: the share code's prefix -->
        <input id="import-code" type="text" bind:value={importText} onkeydown={(e) => e.key === "Escape" && (importText = null)} placeholder="PZMM1:…" autofocus spellcheck="false" style="flex: 1" />
        <button class="primary" type="submit" disabled={!importText.trim()}><Icon name="upload" size={14} />{t("common.import")}</button>
        <button class="quiet" type="button" onclick={() => (importText = null)}><Icon name="x" size={14} />{t("common.cancel")}</button>
      </div>
    </form>
  {/if}

  {#if modePlan}
    <div class="mode-plan" role="region" aria-label={t("app.switchGameLabel")}>
      {#if modePlan.error}
        <p class="error">{modePlan.error}</p>
        <button class="quiet" onclick={() => (modePlan = null)}><Icon name="x" size={14} />{t("common.close")}</button>
      {:else}
        <p>
          <strong>{t(modePlan.target === "viewpoint" ? "app.switchToViewpoint" : "app.switchToZomboid")}</strong>
          {#if modePlan.target === "viewpoint"}
            {t("app.viewpointNote")}
          {/if}
        </p>
        {#if modePlan.lines.length}
          <ul class="mono">{#each modePlan.lines as l}<li>{l}</li>{/each}</ul>
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

  {#if appUpdate && !appUpdateLater}
    <div class="app-update" role="status">
      {#if appProgress === null}
        <span>{@html th("app.updateAvailable", { version: appUpdate.version, current: appVersion })}</span>
        <button class="quiet" onclick={() => api.openUrl(`https://github.com/SEVNC3/pz-mod-manager/releases/tag/v${appUpdate?.version}`)}>{t("app.whatsNew")}</button>
        <button class="quiet" onclick={() => (appUpdateLater = true)}>{t("app.later")}</button>
        <button class="primary" onclick={installAppUpdate} disabled={busy} title={busy ? t("app.waitTip") : ""}><Icon name="update" size={14} />{t("app.updateRestart")}</button>
      {:else}
        <span>{t("app.downloading", { version: appUpdate.version, progress: appProgress })}</span>
        <progress max="100" value={appProgress}></progress>
      {/if}
    </div>
  {/if}

  <div class="tabbar">
  <div class="tabs" role="tablist" aria-label={t("app.sectionsLabel")} tabindex="-1" onkeydown={(e) => arrowPick(e, TAB_IDS, tab, (v) => (tab = v as typeof tab))}>
    {#each TAB_IDS as id}
      <button role="tab" aria-selected={tab === id} class:active={tab === id} onclick={() => (tab = id as typeof tab)}>
        {t(`tabs.${id}`)}{#if (id === "zb" && zbNeeded) || (id === "settings" && folderTrouble)}<span class="alert" aria-label={t("app.needsSetup")}></span>{/if}
      </button>
    {/each}
  </div>
  {@render actions()}
  <Links />
  </div>

  {#if workshopSeen}
    <Workshop hidden={tab !== "workshop"} {busy} defaultBuild={view?.build ?? null} onInstall={installWorkshop} viewpoint={view?.mode === "viewpoint"} />
  {/if}
  {#if tab === "workshop"}
    <!-- kept mounted above -->
  {:else if tab === "maps"}
    <Maps {busy} {mods} loadOrder={order} {say} onSetMods={setMods} onInstallWorkshop={installWorkshop} onShowMod={(id) => { selected = id; tab = "mods"; }}
      canUndo={undoStack.length > 0} onUndo={undo} />
  {:else if tab === "servers"}
    <ServersSaves {busy} profile={view?.profile ?? ""} enabledIds={order}
      onImport={(name) => run(t("busy.importingServer", { name }), () => api.serverImport(name), { showLog: true })}
      onUseSave={(mode, name) => run(t("busy.loadingSave", { name }), () => api.saveUse(mode, name))} />
  {:else if tab === "zb"}
    <ZombieBuddy status={zb} profile={view?.profile ?? ""} {busy}
      onSetup={async () => { await run(t("busy.zbSetup"), api.zbInstall, { showLog: true }); }}
      onRemove={async () => { await run(t("busy.zbRemove"), api.zbUninstall, { showLog: true }); }} />
  {:else if tab === "settings"}
    <Settings bind:cfg {view} {busy} {appVersion} {say} onBuild={(b) => run(t("busy.saving"), () => api.setBuild(b))}
      onRedownload={redownloadAll} onCheckUpdate={() => checkAppUpdate(true)}
      onFoldersChanged={() => run(t("busy.loading"), api.load).then(() => { refreshSave(); refreshZb(); })} />
  {:else}
  <section class="mods">
  {#if saveDiff && latestSave && !saveNoticeDismissed}
    <div class="banner save" role="status">
      <Icon name="alert" size={16} />
      <span>
        {@html th("app.saveDiff", { save: latestSave.name, diffs: [saveDiff.missing.length && t("app.saveMissing", { count: saveDiff.missing.length }), saveDiff.extra.length && t("app.saveExtra", { count: saveDiff.extra.length })].filter(Boolean).join(", ") })}
      </span>
      <button onclick={() => latestSave && run(t("busy.loadingSave", { name: latestSave.name }), () => api.saveUse(latestSave!.mode, latestSave!.name))}><Icon name="swap" size={14} />{t("app.useSaveMods")}</button>
      <button class="quiet" onclick={() => (saveNoticeDismissed = true)}>{t("app.dismiss")}</button>
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
    <div class="seg" role="radiogroup" aria-label={t("mods.showLabel")} tabindex="-1" onkeydown={(e) => arrowPick(e, FILTER_IDS, filter, (v) => (filter = v as typeof filter))}>
      {#each [["all", t("mods.filterAll")], ["on", t("common.enabled")], ["off", t("common.disabled")], ["problems", problemCount ? t("mods.filterProblemsN", { count: problemCount }) : t("mods.filterProblems")]] as [v, label]}
        <button role="radio" aria-checked={filter === v} class:active={filter === v} onclick={() => (filter = v as typeof filter)}>{label}</button>
      {/each}
    </div>
    <label class="sort">
      <select bind:value={sortBy} aria-label={t("mods.sortLabel")} title={t("mods.sortLabel")}>
        <option value="order">{t("mods.sortOrder")}</option>
        <option value="name">{t("mods.sortName")}</option>
        <option value="size">{t("mods.sortSize")}</option>
        <option value="updated">{t("mods.sortUpdated")}</option>
        <option value="status">{t("mods.sortStatus")}</option>
      </select>
    </label>
    <span class="count">{t("mods.count", { enabled: enabledCount, count: mods.size })}</span>
    {#if missingReqs.size}
      <button class="primary" onclick={installMissing} disabled={busy}
        title={t("mods.installMissingTip", { ids: [...missingReqs].join(", ") })}><Icon name="download" size={14} />{t("mods.installMissing", { count: missingReqs.size })}</button>
    {/if}
    {#if fixable}
      <button class="primary" onclick={fixLoadOrder} disabled={busy}
        title={t("mods.fixTip")}><Icon name="wand" size={14} />{t("mods.fix")}</button>
    {/if}
    {#if picked.size >= 2}
      <span class="bulk" role="group" aria-label={t("mods.selectedLabel")}>
        <strong>{t("mods.selected", { count: picked.size })}</strong>
        <button onclick={() => bulkSet(true)} disabled={busy}><Icon name="check" size={14} />{t("common.enable")}</button>
        <button onclick={() => bulkSet(false)} disabled={busy}><Icon name="x" size={14} />{t("common.disable")}</button>
        <select aria-label={t("mods.moveLabel")} disabled={busy || filtering}
          onchange={(e) => { const v = e.currentTarget.value; e.currentTarget.value = ""; if (v) bulkMove(v); }}>
          <option value="">{t("mods.moveTo")}</option>
          <option value="top">{t("mods.moveTop")}</option>
          {#each separators as s}<option value={s}>{t("mods.moveUnder", { name: s })}</option>{/each}
          <option value="bottom">{t("mods.moveBottom")}</option>
        </select>
        <button class="danger" onclick={bulkRemove} disabled={busy}><Icon name="trash" size={14} />{t("common.remove")}</button>
        <button class="quiet" onclick={() => (picked = new Set())}><Icon name="x" size={14} />{t("common.clear")}</button>
      </span>
    {:else if !missingReqs.size && !fixable}
      <span class="hint">
        {t(filtering ? "mods.hintFiltering" : "mods.hint")}
      </span>
    {:else}
      <span class="spacer"></span>
    {/if}
    <span class="tools">
    <button class="quiet" onclick={checkSetup} disabled={busy} title={t("mods.checkSetupTip")}><Icon name="pulse" size={14} />{t("mods.checkSetup")}</button>
    <button class="quiet icon" onclick={undo} disabled={busy || !undoStack.length} title={t("mods.undoTip")} aria-label={t("mods.undo")}><Icon name="undo" size={16} /></button>
    <button class="quiet icon" onclick={redo} disabled={busy || !redoStack.length} title={t("mods.redoTip")} aria-label={t("mods.redo")}><Icon name="redo" size={16} /></button>
    <button class="quiet" onclick={addSeparator} disabled={busy || filtering} title={t("mods.separatorTip")}><Icon name="plus" size={14} />{t("mods.separator")}</button>
    <button class="quiet" onclick={() => groupLibrary()} disabled={busy} title={t("mods.groupTip")}><Icon name="layers" size={14} />{t("mods.group")}</button>
    </span>
  </div>

  <Findings {findings} {gameErrors} {bisect} {busy} onClose={() => (findings = null)} onSelect={(id) => (selected = id)}
    onBisectStart={bisectStart} onBisectAnswer={bisectAnswer} onBisectStop={bisectStop} />
  <main class:with-details={selected} class:with-panel={!selected && rows.length}>
    <div class="list blueprint">
      {#if !view}
        <p class="loading">{t("mods.loading")}</p>
      {:else if rows.length === 0}
        <div class="empty">
          <p class="marker">{t("mods.emptyTitle")}</p>
          <p>{t("mods.emptyHint")}</p>
        </div>
      {:else}
        <ModTable {rows} {order} {visible} {sectionOf} {filtering} {busy} build={view?.build ?? null} {stats} {overlaps} {problems} {updates}
          {collapsed} {picked} {selected} bind:dragFrom bind:editingSep onRowClick={rowClick} onRowKey={rowKey} onToggle={toggle}
          onToggleSection={toggleSection} onDrop={dropOn} onRename={renameSep} onRemoveSep={removeSep} />
      {/if}
    </div>

    {#if selected}
      <Details id={selected} {mods} {busy} hasUpdate={updates.has(selected)} onUpdate={(id) => updateMods([id])} onInstallWorkshop={installWorkshop}
        onToggle={toggle} onSelect={(id) => (selected = id)}
        onRemove={remove} onRollback={(id) => run(t("busy.restoring", { id }), () => api.rollback(id))} onClose={() => (selected = null)} />
    {:else if rows.length}
      <LoadOrder {rows} {order} {mods} {problems} {fixable} {overlaps} {busy} onAutosort={autosortList} onFix={fixLoadOrder}
        onMove={moveInOrder} onSelect={(id) => (selected = id)} />
    {/if}
  </main>
  </section>
  {/if}

  <ActivityLog {log} {busy} {busyLabel} bind:open={logOpen} />
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .app > :global(section) {
    flex: 1;
    min-height: 0;
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

  /* long tab names (and narrow windows) wrap rather than push the actions off-screen */
  .tabbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
  }
  .tabs button {
    border: 0;
    border-radius: 0;
    background: transparent;
    padding: 9px 14px 8px;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    position: relative;
  }
  .tabs button:hover:not(:disabled),
  .tabs button.active {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-bottom-color: var(--on-selected);
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
  .mode-plan {
    padding: 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .mode-plan p {
    margin: 0 0 6px;
  }
  .mode-plan ul {
    margin: 0 0 8px;
    padding-left: 18px;
    overflow-wrap: anywhere;
  }
  .mode-plan .row {
    display: flex;
    gap: 8px;
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
    flex-wrap: wrap;
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
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  .install {
    display: flex;
    gap: 6px;
    flex: 1 1 200px;
  }
  .install input {
    flex: 1;
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
  .count {
    color: var(--muted);
  }
  .hint {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
    margin-left: auto;
  }
  .spacer {
    margin-left: auto;
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
  .sort {
    display: flex;
    align-items: center;
    gap: 6px;
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
  .mode-plan,
  .app-update {
    margin: 10px 16px 0;
    border-bottom: 0;
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  .banner > :global(.icon) {
    flex: none;
    color: var(--danger);
  }

  @media (max-width: 900px) {
    main.with-panel {
      grid-template-columns: 1fr;
    }
    main.with-panel :global(.panel) {
      display: none;
    }
    main.with-details {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr 45%;
    }
    .hint {
      display: none;
    }
    .tabs button {
      padding: 9px 9px 8px;
    }
  }
</style>
