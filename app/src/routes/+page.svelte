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
  import Links from "$lib/Links.svelte";
  import Icon from "$lib/Icon.svelte";
  import ActivityLog from "$lib/ActivityLog.svelte";
  import Findings from "$lib/Findings.svelte";
  import SettingsPanel from "$lib/SettingsPanel.svelte";
  import ModTable from "$lib/ModTable.svelte";
  import { autosort, bisectRound, findProblems, moveSection, placeInSections, sortLoadOrder } from "$lib/order";
  import LoadOrder from "$lib/LoadOrder.svelte";
  import WindowControls from "$lib/WindowControls.svelte";
  import ProfilePicker from "$lib/ProfilePicker.svelte";
  import type { Categories, Finding, ModErrors, Overlap, Save, Stats } from "$lib/api";

  let tab = $state<"mods" | "workshop" | "servers" | "zb">("mods");
  let updates = $state<Set<string>>(new Set());

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
      if (!appUpdate && manual) say(`PZMM ${appVersion} is the latest version.`);
    } catch (e) {
      if (manual) say(`Couldn't check for PZMM updates: ${e}`, true);
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
      say(`Couldn't update PZMM: ${e}`, true);
      appProgress = null;
    }
  }
  let appUpdateLater = $state(false);
  let workshopSeen = $state(false);
  $effect(() => {
    if (tab === "workshop") workshopSeen = true;
  });
  let zb = $state<ZbStatus | null>(null);
  const zbNeeded = $derived(!!zb && zb.required_by.length > 0 && !zbReady(zb));
  const refreshZb = () => api.zbStatus().then((s) => (zb = s)).catch(() => {});

  let view = $state<View | null>(null);
  let rows = $state<Row[]>([]);

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
      else say(String(e), true);
    }
  }

  async function switchMode() {
    if (!modePlan) return;
    const { target, profile } = modePlan;
    modePlan = null;
    const label = target === "viewpoint" ? "Setting up Project Viewpoint" : "Switching to Project Zomboid";
    if (!(await run(label, () => api.modeSwitch(target), { showLog: true }))) return;
    // the switch lands on that game's last profile; go on to the one that was picked
    if (profile && view && profile !== view.profile && view.profiles.includes(profile)) await run(`Switching to "${profile}"`, () => api.profileUse(profile));
    await settleOrder();
  }
  let selected = $state<string | null>(null);
  /** Runs in flight: busy until every one has finished. */
  let pending = $state(0);
  const busy = $derived(pending > 0);
  let busyLabel = $state("");
  let log = $state<{ text: string; error: boolean }[]>([]);
  let logOpen = $state(false);
  let search = $state("");
  let filter = $state<"all" | "on" | "off" | "problems">("all");
  let installText = $state("");
  let settingsOpen = $state(false);
  let newProfile = $state<string | null>(null);
  let editingSep = $state<number | null>(null);
  let dragFrom = $state<number | null>(null);

  // --- derived ---
  const mods = $derived(new Map(rows.filter((r): r is ModRow => r.kind === "mod").map((r) => [r.id, r])));
  const enabledCount = $derived([...mods.values()].filter((m) => m.on).length);
  const problems = $derived(findProblems(rows));
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
      if (v.adopted) {
        undoStack = redoStack = [];
        say(`Picked up changes made in the game's mod menu (profile "${v.profile}").`);
      }
    } catch {}
  }

  // --- actions ---
  function say(text: string, error = false) {
    log.push({ text, error });
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
      if (v.adopted) say(`Picked up changes made in the game's mod menu (profile "${v.profile}").`);
      if (selected && !v.rows.some((r) => r.kind === "mod" && r.id === selected)) selected = null;
      // a profile seen for the first time (first run, New, Import, Viewpoint) without separators gets autosorted
      const fresh = v.created || (knownProfiles && !knownProfiles.includes(v.profile));
      if (fresh && v.rows.length && !v.rows.some((r) => r.kind === "sep")) await autosortList(false);
      else if (!keepHistory && before.size) await placeNew(before);
      return true;
    } catch (e) {
      say(String(e), true);
      // resync with what's on disk
      try {
        view = await api.load();
        rows = view.rows;
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

  /** Autosort: needed mods on, grouped by category, every load rule satisfied. `record` keeps it undoable. */
  async function autosortList(record = true) {
    let c: Categories;
    pending++;
    busyLabel = "Reading Workshop tags";
    try {
      c = await api.categories();
    } catch (e) {
      // no tags (offline, rate-limited): still fix the load rules, keep the sections as they are
      if (await commit(sortLoadOrder(rows), record)) say(`Couldn't read Workshop tags, so only the load rules were fixed (${e}).`, true);
      return;
    } finally {
      pending--;
    }
    const own = separators.filter((s) => !c.order.includes(s));
    if (record && own.length && !confirm(`Autosort replaces your own sections (${own.slice(0, 4).join(", ")}${own.length > 4 ? ", …" : ""}) with category sections. Ctrl+Z undoes it. Autosort now?`)) return;
    const next = autosort(rows, c.of, c.order);
    const sections = next.filter((r) => r.kind === "sep").length;
    if (await commit(next, record)) say(`Autosorted into ${sections} categories.${record ? " Ctrl+Z puts the old order back." : ""}`);
  }

  /** In a list sorted into categories, file newly installed mods under theirs. */
  async function placeNew(before: Set<string>) {
    const fresh = rows.flatMap((r) => (r.kind === "mod" && !before.has(r.id) ? [r.id] : []));
    if (!fresh.length || !rows.some((r) => r.kind === "sep")) return;
    try {
      const c = await api.categories();
      if (rows.some((r) => r.kind === "sep" && c.order.includes(r.name))) await commit(placeInSections(rows, c.of, c.order, fresh), false);
    } catch {}
  }

  /** Enable disabled requirements and reorder so requirements and load hints hold. */
  function fixLoadOrder() {
    commit(sortLoadOrder(rows));
  }

  async function checkUpdates() {
    try {
      updates = new Set(await api.checkUpdates());
      if (updates.size) say(`${updates.size} mod${updates.size === 1 ? " has an update" : "s have updates"} on the Workshop.`);
    } catch (e) {
      say(`Couldn't check for updates: ${e}`);
    }
  }

  let checking = $state(false);
  async function checkUpdatesNow() {
    checking = true;
    await checkUpdates();
    checking = false;
    if (!updates.size) say("Your mods are up to date.");
  }

  /** Re-download everything PZMM installed (repairs damaged copies); asks first, like an update. */
  async function redownloadAll() {
    const ours = [...mods.values()].filter((m) => /^(workshop|github|local):/.test(m.source)).map((m) => m.id);
    if (!ours.length) return say("PZMM hasn't installed any mods to re-download (Steam keeps subscriptions up to date).");
    if (!confirm(`Re-download all ${ours.length} mods PZMM installed? Each keeps its current version as a backup.`)) return;
    await updateMods(ours);
  }

  async function updateMods(ids: string[]) {
    const inSave = latestSave ? ids.filter((i) => latestSave!.mods.includes(i)) : [];
    if (
      inSave.length &&
      !confirm(
        `${inSave.join(", ")} ${inSave.length === 1 ? "is" : "are"} used by your latest save (${latestSave!.name}). ` +
          "Updates can break a save in progress. Each updated mod keeps its previous version (Restore previous version). Update now?",
      )
    )
      return;
    if (await run(`Updating ${ids.length === 1 ? ids[0] : `${ids.length} mods`}`, () => api.updateMods(ids), { showLog: true })) {
      updates = new Set([...updates].filter((u) => !ids.includes(u)));
    }
  }

  /** After installing requirements, move them above the mods that need them. */
  async function settleOrder() {
    await tick();
    if (fixable) fixLoadOrder();
  }

  /** Every install: run it with the log open, then settle the order of what came in. */
  async function installAndSettle(label: string, job: () => Promise<View>) {
    const ok = await run(label, job, { showLog: true });
    if (ok) await settleOrder();
    return ok;
  }

  const installWorkshop = (id: string) => installAndSettle(`Installing Workshop mod ${id}`, () => api.install([id]));

  async function installMissing() {
    let unresolved: string[] = [];
    const ok = await installAndSettle(`Finding ${missingReqs.size} missing ${missingReqs.size === 1 ? "mod" : "mods"} on the Workshop`, async () => {
      const [v, left] = await api.installMissing();
      unresolved = left;
      return v;
    });
    if (ok && unresolved.length) say(`Still missing: ${unresolved.join(", ")}. Select a mod that needs one and use Find to pick a match.`, true);
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
      say(`Copied a share code for "${view?.profile}". Paste it to a friend; they use Import in PZMM.`);
      logOpen = true;
    } catch (e) {
      say(`Couldn't copy the share code: ${e}`, true);
    }
  }

  async function importProfile(e: Event) {
    e.preventDefault();
    const text = importText ?? "";
    importText = null;
    try {
      const shared = decodeShare(text);
      await run(`Importing "${shared.name}"`, () => api.profileImport(shared), { showLog: true });
    } catch (err) {
      say(String(err), true);
    }
  }

  // --- health check ---
  let findings = $state<Finding[] | null>(null);
  let gameErrors = $state<ModErrors[]>([]);
  async function checkSetup() {
    try {
      [findings, gameErrors] = await Promise.all([api.doctor(), api.consoleErrors().catch(() => [])]);
    } catch (e) {
      say(`Couldn't check the setup: ${e}`, true);
    }
  }

  // --- find a broken mod by halving the suspects each game run ---
  let bisect = $state<{ original: Row[]; suspects: string[]; testing: string[]; round: number } | null>(null);

  /** Enabled = everything not under suspicion + the half being tested + what those require. */
  function bisectApply(b: NonNullable<typeof bisect>) {
    const { on, testing } = bisectRound(b.original, b.suspects, (id) => mods.get(id)?.requires ?? []);
    bisect = { ...b, testing };
    commit(b.original.map((r) => (r.kind === "mod" ? { ...r, on: on.has(r.id) } : r)), false);
  }

  function bisectStart() {
    const suspects = [...mods.values()].filter((m) => m.on && !m.missing).map((m) => m.id);
    if (suspects.length < 2) return say("Enable at least two mods to search for a broken one.", true);
    findings = null;
    undoStack = redoStack = []; // undoing into a half-disabled test list would be a trap
    bisectApply({ original: $state.snapshot(rows), suspects, testing: [], round: 1 });
  }

  function bisectAnswer(stillBroken: boolean) {
    if (!bisect) return;
    const suspects = stillBroken ? bisect.testing : bisect.suspects.filter((s) => !bisect!.testing.includes(s));
    if (suspects.length <= 1) {
      const original = bisect.original;
      bisect = null;
      undoStack = redoStack = [];
      commit(original, false);
      say(suspects.length ? `Found it: ${suspects[0]} causes the problem. Your mod list is restored.` : "The problem didn't follow any single mod. Your mod list is restored.", !!suspects.length);
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
    commit(original, false);
  }

  // --- undo/redo for list edits (toggle, reorder, separators, fixes) ---
  let undoStack = $state<Row[][]>([]);
  let redoStack = $state<Row[][]>([]);

  const commit = (next: Row[], record = true) => {
    if (record) {
      undoStack = [...undoStack.slice(-49), $state.snapshot(rows)];
      redoStack = [];
    }
    rows = next;
    return run("Saving", () => api.save(next), { keepHistory: true });
  };

  function undo() {
    const prev = undoStack.at(-1);
    if (!prev || busy) return;
    undoStack = undoStack.slice(0, -1);
    redoStack = [...redoStack, $state.snapshot(rows)];
    commit(prev, false);
  }

  function redo() {
    const next = redoStack.at(-1);
    if (!next || busy) return;
    redoStack = redoStack.slice(0, -1);
    undoStack = [...undoStack, $state.snapshot(rows)];
    commit(next, false);
  }

  const TAB_IDS = ["mods", "workshop", "servers", "zb"];
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
    if (e.key === "Escape" && !typing && (picked.size || selected || modePlan || settingsOpen)) {
      if (modePlan) modePlan = null;
      else if (settingsOpen) settingsOpen = false;
      else if (picked.size) picked = new Set();
      else selected = null;
      return;
    }
    if (typing || !(e.ctrlKey || e.metaKey) || tab !== "mods") return;
    const k = e.key.toLowerCase();
    if (k === "z" && !e.shiftKey) {
      e.preventDefault();
      undo();
    } else if (k === "y" || (k === "z" && e.shiftKey)) {
      e.preventDefault();
      redo();
    }
  }

  function toggle(id: string) {
    commit(rows.map((r) => (r.kind === "mod" && r.id === id ? { ...r, on: !r.on } : r)));
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
    await installAndSettle(`Installing ${sources.length === 1 ? sources[0] : `${sources.length} mods`}`, () => api.install(sources));
  }

  /** A new separator above the selected mod (or at the top), ready to rename. */
  async function addSeparator() {
    const at = Math.max(0, selected ? rows.findIndex((r) => r.kind === "mod" && r.id === selected) : 0);
    const taken = new Set(separators);
    let name = "New section";
    for (let n = 2; taken.has(name); n++) name = `New section ${n}`;
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
    else if (name.trim() !== r.name && separators.includes(name.trim())) say(`There's already a section called "${name.trim()}". Pick another name.`, true);
    else commit(rows.map((x, j) => (j === i ? { kind: "sep", name: name.trim() } : x)));
  }

  async function createProfile(e: Event) {
    e.preventDefault();
    const name = newProfile?.trim();
    newProfile = null;
    if (name) await run(`Creating profile "${name}"`, () => api.profileNew(name));
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
    commit(rows.map((r) => (r.kind === "mod" && picked.has(r.id) ? { ...r, on } : r)));
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
    if (!ids.length) return say("None of the selected mods were installed by PZMM; unsubscribe Steam mods in Steam.", true);
    const skipped = picked.size - ids.length;
    if (confirm(`Remove ${ids.length} mod${ids.length === 1 ? "" : "s"}? Their files are deleted from your Zomboid mods folder.${skipped ? ` (${skipped} not installed by PZMM will be left alone.)` : ""}`)) {
      run(`Removing ${ids.length} mods`, () => api.remove(ids)).then(() => (picked = new Set()));
    }
  }

  const separators = $derived(rows.flatMap((r) => (r.kind === "sep" && r.name ? [r.name] : [])));

  function remove(id: string) {
    if (confirm(`Remove ${id}? Its files are deleted from your Zomboid mods folder.`)) run(`Removing ${id}`, () => api.remove([id]));
  }

  function deleteProfile() {
    const name = view?.profile;
    const other = modeProfiles.find((p) => p !== name); // stay in this game: switching game goes through the mode plan
    if (!name || !other) return;
    if (confirm(`Delete the profile "${name}"? Your mods stay installed. PZMM will switch to "${other}".`)) {
      run(`Deleting profile "${name}"`, async () => {
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
    const unlisten = listen<string>("log", (e) => say(e.payload, e.payload.startsWith("error:")));
    run("Loading", api.load).then(() => {
      checkUpdates();
      refreshStats();
    });
    refreshSave();
    getVersion().then((v) => (appVersion = v));
    checkAppUpdate();
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={globalKey} onfocus={refreshOnFocus} />

<div class="app">
  <header class="top" data-tauri-drag-region>
    <WindowControls />
    <img class="brand" src="/brand/pzmm.png" alt="PZMM" width="44" height="44" />

    <div class="profile">
      {#if newProfile !== null}
        <form onsubmit={createProfile} class="inline">
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" bind:value={newProfile} placeholder="Name, e.g. Friends server" autofocus
            onkeydown={(e) => e.key === "Escape" && (newProfile = null)} />
          <button type="submit" disabled={!newProfile.trim()}><Icon name="plus" size={14} />Create</button>
        </form>
      {:else}
        <ProfilePicker {view} {busy} onUse={(p) => run("Switching profile", () => api.profileUse(p))}
          onSwitchGame={planMode} onNew={() => (newProfile = "")} />
        <button class="quiet" onclick={shareProfile} disabled={busy} title="Copy a code others can import"><Icon name="share" size={14} />Share</button>
        <button class="quiet" onclick={() => (importText = "")} disabled={busy} title="Import a profile from a share code"><Icon name="upload" size={14} />Import</button>
        {#if modeProfiles.length > 1}
          <button class="quiet" onclick={deleteProfile} disabled={busy}><Icon name="trash" size={14} />Delete</button>
        {/if}
      {/if}
    </div>

    <form class="install" onsubmit={install}>
      <label for="install" class="visually-hidden">Install a mod</label>
      <input id="install" type="text" bind:value={installText} autocomplete="off" spellcheck="false"
        placeholder="Paste a Workshop link, collection or ID, a GitHub repo (owner/repo), or a .zip path" />
      <button class="primary" type="submit" disabled={busy || !installText.trim()}><Icon name="download" size={14} />Install</button>
    </form>

  </header>
  {#snippet actions()}
    <div class="right">
      {#if updates.size}
        <button class="primary" onclick={() => updateMods([...updates])} disabled={busy}><Icon name="refresh" size={14} />Update {updates.size} {updates.size === 1 ? "mod" : "mods"}</button>
      {:else}
        <button onclick={checkUpdatesNow} disabled={busy || checking} title="Ask the Workshop whether any mod has a newer version">
          <Icon name="refresh" size={14} />{checking ? "Checking…" : "Check for updates"}</button>
      {/if}
      <button onclick={() => api.openUrl("steam://rungameid/108600")}><Icon name="play" size={14} />Launch game</button>
      <button class="quiet" aria-expanded={settingsOpen} onclick={() => (settingsOpen = !settingsOpen)}><Icon name="settings" size={14} />Settings</button>
    </div>
  {/snippet}

  {#if importText !== null}
    <form class="mode-plan" onsubmit={importProfile}>
      <label for="import-code"><strong>Import a profile</strong>: paste the share code someone sent you. Missing Workshop mods are installed.</label>
      <div class="row">
        <!-- svelte-ignore a11y_autofocus -->
        <input id="import-code" type="text" bind:value={importText} onkeydown={(e) => e.key === "Escape" && (importText = null)} placeholder="PZMM1:…" autofocus spellcheck="false" style="flex: 1" />
        <button class="primary" type="submit" disabled={!importText.trim()}><Icon name="upload" size={14} />Import</button>
        <button class="quiet" type="button" onclick={() => (importText = null)}><Icon name="x" size={14} />Cancel</button>
      </div>
    </form>
  {/if}

  {#if modePlan}
    <div class="mode-plan" role="region" aria-label="Switch game">
      {#if modePlan.error}
        <p class="error">{modePlan.error}</p>
        <button class="quiet" onclick={() => (modePlan = null)}><Icon name="x" size={14} />Close</button>
      {:else}
        <p>
          <strong>{modePlan.target === "viewpoint" ? "Switch to Project Viewpoint (3D)?" : "Switch back to Project Zomboid?"}</strong>
          {#if modePlan.target === "viewpoint"}
            Viewpoint is early in development. It loads through ZombieBuddy, which asks you once, when the game starts, to approve its Java code.
          {/if}
        </p>
        {#if modePlan.lines.length}
          <ul class="mono">{#each modePlan.lines as l}<li>{l}</li>{/each}</ul>
          <div class="row">
            <button class="primary" onclick={switchMode} disabled={busy}><Icon name="swap" size={14} />Switch</button>
            <button class="quiet" onclick={() => (modePlan = null)}><Icon name="x" size={14} />Cancel</button>
          </div>
        {:else}
          <p class="hint-line">Checking what's needed…</p>
        {/if}
      {/if}
    </div>
  {/if}

  {#if appUpdate && !appUpdateLater}
    <div class="app-update" role="status">
      {#if appProgress === null}
        <span><strong>PZMM {appUpdate.version} is available.</strong> You have {appVersion}.</span>
        <button class="quiet" onclick={() => api.openUrl(`https://github.com/SEVNC3/pz-mod-manager/releases/tag/v${appUpdate?.version}`)}>What's new</button>
        <button class="quiet" onclick={() => (appUpdateLater = true)}>Later</button>
        <button class="primary" onclick={installAppUpdate} disabled={busy} title={busy ? "Wait for the current job to finish" : ""}><Icon name="update" size={14} />Update and restart</button>
      {:else}
        <span>Downloading PZMM {appUpdate.version}… {appProgress}%</span>
        <progress max="100" value={appProgress}></progress>
      {/if}
    </div>
  {/if}

  <SettingsPanel open={settingsOpen} {view} {busy} {appVersion} onBuild={(b) => run("Saving", () => api.setBuild(b))}
    onRedownload={redownloadAll} onCheckUpdate={() => checkAppUpdate(true)} />

  <div class="tabbar">
  <div class="tabs" role="tablist" aria-label="Sections" tabindex="-1" onkeydown={(e) => arrowPick(e, TAB_IDS, tab, (v) => (tab = v as typeof tab))}>
    {#each [["mods", "My mods"], ["workshop", "Browse Workshop"], ["servers", "Servers & saves"], ["zb", "ZombieBuddy"]] as [id, label]}
      <button role="tab" aria-selected={tab === id} class:active={tab === id} onclick={() => (tab = id as typeof tab)}>
        {label}{#if id === "zb" && zbNeeded}<span class="alert" aria-label="needs setup"></span>{/if}
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
  {:else if tab === "servers"}
    <ServersSaves {busy} profile={view?.profile ?? ""} enabledIds={[...mods.values()].filter((m) => m.on).map((m) => m.id)}
      onImport={(name) => run(`Importing ${name}.ini`, () => api.serverImport(name), { showLog: true })}
      onUseSave={(mode, name) => run(`Loading the mods of ${name}`, () => api.saveUse(mode, name))} />
  {:else if tab === "zb"}
    <ZombieBuddy status={zb} profile={view?.profile ?? ""} {busy}
      onSetup={async () => { await run("Setting up ZombieBuddy", api.zbInstall, { showLog: true }); }}
      onRemove={async () => { await run("Removing ZombieBuddy", api.zbUninstall, { showLog: true }); }} />
  {:else}
  <section class="mods">
  {#if saveDiff && latestSave && !saveNoticeDismissed}
    <div class="banner save" role="status">
      <Icon name="alert" size={16} />
      <span>
        Your latest save, <strong>{latestSave.name}</strong>, was played with a different mod list:
        {[saveDiff.missing.length && `${saveDiff.missing.length} of its mods aren't enabled`, saveDiff.extra.length && `${saveDiff.extra.length} enabled mods weren't in it`].filter(Boolean).join(", ")}.
        Continuing it with this list can break it.
      </span>
      <button onclick={() => latestSave && run(`Loading the mods of ${latestSave.name}`, () => api.saveUse(latestSave!.mode, latestSave!.name))}><Icon name="swap" size={14} />Use this save's mods</button>
      <button class="quiet" onclick={() => (saveNoticeDismissed = true)}>Dismiss</button>
    </div>
  {/if}
  {#if zbNeeded}
    <div class="banner" role="status">
      <Icon name="alert" size={16} />
      <span>{zb?.required_by.length} enabled {zb?.required_by.length === 1 ? "mod needs" : "mods need"} ZombieBuddy, which isn't set up{zb?.files_current || zb?.launcher_patched ? " completely (a game update may have reset it)" : ""}.</span>
      <button onclick={() => (tab = "zb")}><Icon name="settings" size={14} />Set up ZombieBuddy</button>
    </div>
  {/if}
  <div class="filters">
    <input id="mod-search" type="search" bind:value={search} placeholder="Search mods  ( / )" aria-label="Search mods" />
    <div class="seg" role="radiogroup" aria-label="Show" tabindex="-1" onkeydown={(e) => arrowPick(e, FILTER_IDS, filter, (v) => (filter = v as typeof filter))}>
      {#each [["all", "All"], ["on", "Enabled"], ["off", "Disabled"], ["problems", `Problems${problemCount ? ` (${problemCount})` : ""}`]] as [v, label]}
        <button role="radio" aria-checked={filter === v} class:active={filter === v} onclick={() => (filter = v as typeof filter)}>{label}</button>
      {/each}
    </div>
    <label class="sort">
      <select bind:value={sortBy} aria-label="Sort the list" title="Sort the list">
        <option value="order">Load order</option>
        <option value="name">Name</option>
        <option value="size">Size</option>
        <option value="updated">Last changed</option>
        <option value="status">Problems first</option>
      </select>
    </label>
    <span class="count">{enabledCount} enabled · {mods.size} mods</span>
    {#if missingReqs.size}
      <button class="primary" onclick={installMissing} disabled={busy}
        title={`Find and install from the Workshop: ${[...missingReqs].join(", ")}`}><Icon name="download" size={14} />Install missing mods ({missingReqs.size})</button>
    {/if}
    {#if fixable}
      <button class="primary" onclick={fixLoadOrder} disabled={busy}
        title="Enable required mods and order mods by their require / loadModAfter / loadModBefore rules"><Icon name="wand" size={14} />Fix load order</button>
    {/if}
    {#if picked.size >= 2}
      <span class="bulk" role="group" aria-label="Selected mods">
        <strong>{picked.size} selected</strong>
        <button onclick={() => bulkSet(true)} disabled={busy}><Icon name="check" size={14} />Enable</button>
        <button onclick={() => bulkSet(false)} disabled={busy}><Icon name="x" size={14} />Disable</button>
        <select aria-label="Move selected mods" disabled={busy || filtering}
          onchange={(e) => { const v = e.currentTarget.value; e.currentTarget.value = ""; if (v) bulkMove(v); }}>
          <option value="">Move to…</option>
          <option value="top">Top of the list</option>
          {#each separators as s}<option value={s}>Under "{s}"</option>{/each}
          <option value="bottom">Bottom of the list</option>
        </select>
        <button class="danger" onclick={bulkRemove} disabled={busy}><Icon name="trash" size={14} />Remove</button>
        <button class="quiet" onclick={() => (picked = new Set())}><Icon name="x" size={14} />Clear</button>
      </span>
    {:else if !missingReqs.size && !fixable}
      <span class="hint">
        {filtering ? "Clear the search and filter, and sort by load order, to reorder." : "Drag or Alt+↑/↓ to reorder · Space toggles · Ctrl/Shift+click selects"}
      </span>
    {:else}
      <span class="spacer"></span>
    {/if}
    <span class="tools">
    <button class="quiet" onclick={checkSetup} disabled={busy} title="Check the profile and server configs for problems"><Icon name="pulse" size={14} />Check setup</button>
    <button class="quiet icon" onclick={undo} disabled={busy || !undoStack.length} title="Undo the last list change (Ctrl+Z)" aria-label="Undo"><Icon name="undo" size={16} /></button>
    <button class="quiet icon" onclick={redo} disabled={busy || !redoStack.length} title="Redo (Ctrl+Y)" aria-label="Redo"><Icon name="redo" size={16} /></button>
    <button class="quiet" onclick={addSeparator} disabled={busy || filtering} title="Add a separator above the selected mod"><Icon name="plus" size={14} />Separator</button>
    </span>
  </div>

  <Findings {findings} {gameErrors} {bisect} {busy} onClose={() => (findings = null)} onSelect={(id) => (selected = id)}
    onBisectStart={bisectStart} onBisectAnswer={bisectAnswer} onBisectStop={bisectStop} />
  <main class:with-details={selected} class:with-panel={!selected && rows.length}>
    <div class="list blueprint">
      {#if !view}
        <p class="loading">Loading your mods…</p>
      {:else if rows.length === 0}
        <div class="empty">
          <p class="marker">Nothing in the stockpile yet.</p>
          <p>Paste a Steam Workshop link above to install a mod, or subscribe to mods in Steam: subscriptions show up here automatically.</p>
        </div>
      {:else}
        <ModTable {rows} {visible} {sectionOf} {filtering} {busy} build={view?.build ?? null} {stats} {overlaps} {problems} {updates}
          {collapsed} {picked} {selected} bind:dragFrom bind:editingSep onRowClick={rowClick} onRowKey={rowKey} onToggle={toggle}
          onToggleSection={toggleSection} onDrop={dropOn} onRename={renameSep} onRemoveSep={removeSep} />
      {/if}
    </div>

    {#if selected}
      <Details id={selected} {mods} {busy} hasUpdate={updates.has(selected)} onUpdate={(id) => updateMods([id])} onInstallWorkshop={installWorkshop}
        onToggle={toggle} onSelect={(id) => (selected = id)}
        onRemove={remove} onRollback={(id) => run(`Restoring ${id}`, () => api.rollback(id))} onClose={() => (selected = null)} />
    {:else if rows.length}
      <LoadOrder {rows} {problems} {fixable} {overlaps} {busy} onAutosort={() => autosortList()} onFix={fixLoadOrder} />
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
  .mods {
    display: grid;
    grid-template-rows: auto auto 1fr;
  }

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
    min-height: 0;
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
  }
</style>
