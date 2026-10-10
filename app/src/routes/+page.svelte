<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getVersion } from "@tauri-apps/api/app";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { decodeShare, encodeShare } from "$lib/api";
  import { api, describeSource, zbReady, type Mode, type ModRow, type Row, type View, type ZbStatus } from "$lib/api";
  import Details from "$lib/Details.svelte";
  import Workshop from "$lib/Workshop.svelte";
  import ZombieBuddy from "$lib/ZombieBuddy.svelte";
  import ServersSaves from "$lib/ServersSaves.svelte";
  import LayerIcon from "$lib/LayerIcon.svelte";
  import Links from "$lib/Links.svelte";
  import Icon, { type IconName } from "$lib/Icon.svelte";
  import { autosort, bisectRound, findProblems, placeInSections, sortLoadOrder } from "$lib/order";
  import LoadOrder from "$lib/LoadOrder.svelte";
  import WindowControls from "$lib/WindowControls.svelte";
  import type { Finding, ModErrors, Overlap, Save, Stats } from "$lib/api";

  let tab = $state<"mods" | "workshop" | "servers" | "zb">("mods");
  let updates = $state<Set<string>>(new Set());

  // shared files between enabled mods: who overrides whom (refreshed after every change)
  let overlaps = $state<Record<string, Overlap>>({});
  const refreshOverlaps = () => api.conflictSummary().then((o) => (overlaps = o)).catch(() => {});
  const overlapLabel = (o: Overlap) =>
    [
      o.wins_over.length && `Overrides files from ${o.wins_over.join(", ")}`,
      o.loses_to.length && `Some files are overridden by ${o.loses_to.join(", ")}`,
    ]
      .filter(Boolean)
      .join(". ");

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
  let zb = $state<ZbStatus | null>(null);
  const zbNeeded = $derived(!!zb && zb.required_by.length > 0 && !zbReady(zb));
  const refreshZb = () => api.zbStatus().then((s) => (zb = s)).catch(() => {});

  let view = $state<View | null>(null);
  let rows = $state<Row[]>([]);

  // --- Project Zomboid / Project Viewpoint ---
  const MODES: [Mode, string][] = [["zomboid", "Project Zomboid"], ["viewpoint", "Project Viewpoint"]];
  let modePlan = $state<{ target: Mode; lines: string[]; error: string } | null>(null);
  const modeProfiles = $derived(
    (view?.profiles ?? []).filter((p) => p === view?.profile || (view?.kinds[p] ?? "zomboid") === (view?.mode ?? "zomboid")),
  );

  async function planMode(target: Mode) {
    if (!view || target === view.mode) return;
    modePlan = { target, lines: [], error: "" };
    try {
      modePlan.lines = await api.modePreview(target);
    } catch (e) {
      modePlan.error = String(e);
    }
  }

  async function switchMode() {
    if (!modePlan) return;
    const target = modePlan.target;
    modePlan = null;
    logOpen = true;
    const label = target === "viewpoint" ? "Setting up Project Viewpoint" : "Switching to Project Zomboid";
    if (await run(label, () => api.modeSwitch(target))) await settleOrder();
  }
  let selected = $state<string | null>(null);
  let busy = $state(false);
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
  let dropAt = $state<number | null>(null);

  // --- per-viewer preferences (theme, accent) ---
  const pref = (k: string, d: string) => {
    try {
      return localStorage.getItem(k) ?? d;
    } catch {
      return d;
    }
  };
  let theme = $state(pref("pzmm.theme", "system"));
  const DEFAULT_ACCENT = "#6e080c"; // the blood on the separator tape (static/blood.svg)
  // people still on an earlier default (amber, then pink) move to the current one
  let accent = $state([pref("pzmm.accent", DEFAULT_ACCENT)].map((a) => (["#d39b3a", "#fc2684"].includes(a) ? DEFAULT_ACCENT : a))[0]);
  /** Text colour on an accent fill: dark on light accents, white on dark ones. */
  const onAccent = (hex: string) => {
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.45 ? "#1b1a17" : "#ffffff";
  };
  $effect(() => {
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
    root.style.setProperty("--accent", accent);
    root.style.setProperty("--on-accent", onAccent(accent));
    try {
      localStorage.setItem("pzmm.theme", theme);
      localStorage.setItem("pzmm.accent", accent);
    } catch {}
  });

  const SOURCE_HINTS: Record<string, string> = {
    steam: "Subscribed in Steam; Steam keeps it updated",
    workshop: "Downloaded from the Workshop by PZMM",
    github: "Installed from GitHub by PZMM",
    local: "Installed from a file by PZMM",
  };

  // --- derived ---
  const mods = $derived(new Map(rows.filter((r): r is ModRow => r.kind === "mod").map((r) => [r.id, r])));
  const enabledCount = $derived([...mods.values()].filter((m) => m.on).length);
  const order = $derived.by(() => {
    const o = new Map<number, number>();
    let n = 0;
    rows.forEach((r, i) => r.kind === "mod" && r.on && o.set(i, ++n));
    return o;
  });
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
  const fmtSize = (b: number) =>
    b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} GB` : b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`;
  const fmtDate = (s: number) => (s ? new Date(s * 1000).toLocaleDateString(undefined, { year: "2-digit", month: "short", day: "numeric" }) : "");

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
  /** Mods / enabled mods under each separator. */
  const sectionCounts = $derived.by(() => {
    const out = new Map<string, { total: number; on: number }>();
    rows.forEach((r, i) => {
      if (r.kind !== "mod") return;
      const c = out.get(sectionOf[i]) ?? { total: 0, on: 0 };
      c.total++;
      if (r.on) c.on++;
      out.set(sectionOf[i], c);
    });
    return out;
  });

  const visible = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return rows
      .map((r, i) => ({ r, i }))
      .filter(({ r, i }) => {
        if (r.kind === "sep") return !filtering;
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

  /** The icon for an activity line, from what it says. */
  function logIcon(text: string, error: boolean): IconName {
    const t = text.toLowerCase();
    if (error || t.startsWith("error")) return "error";
    if (t.startsWith("installed") || t.includes("downloaded") || t.startsWith("downloading")) return "download";
    if (t.includes("restored")) return "rollback";
    if (t.includes("update")) return "refresh";
    if (t.startsWith("delete") || t.startsWith("remove") || t.includes("removed")) return "trash";
    if (t.startsWith("switched")) return "swap";
    if (t.includes("requires") || t.includes("found on the workshop") || t.startsWith("collection")) return "layers";
    if (t.includes("copied a share code")) return "share";
    if (t.startsWith("started")) return "server";
    if (t.includes("up to date") || t.includes("set up") || t.startsWith("copied") || t.startsWith("imported") || t.startsWith("picked up")) return "check";
    if (t.includes("couldn't") || t.includes("isn't") || t.includes("not ")) return "alert";
    return "info";
  }

  // --- actions ---
  function say(text: string, error = false) {
    log.push({ text, error });
    if (error) logOpen = true;
  }

  /** Run a backend job that returns the new view. Errors go to the activity log; returns success. */
  async function run(label: string, job: () => Promise<View>, keepHistory = false): Promise<boolean> {
    // undo covers list edits only: anything else (installs, profile switches) resets it
    if (!keepHistory) undoStack = redoStack = [];
    const before = new Set(rows.flatMap((r) => (r.kind === "mod" ? [r.id] : [])));
    const knownProfiles = view?.profiles;
    busy = true;
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
      busy = false;
      busyLabel = "";
      refreshZb();
      refreshOverlaps();
      if (!keepHistory) refreshStats(); // files may have changed (installs, updates, removals)
    }
  }

  /** Autosort: needed mods on, grouped by category, every load rule satisfied. `record` keeps it undoable. */
  async function autosortList(record = true) {
    let c: Awaited<ReturnType<typeof api.categories>>;
    const wasBusy = busy;
    busy = true;
    busyLabel = "Reading Workshop tags";
    try {
      c = await api.categories();
    } catch (e) {
      say(String(e), true);
      return;
    } finally {
      busy = wasBusy;
    }
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
    logOpen = true;
    if (await run(`Updating ${ids.length === 1 ? ids[0] : `${ids.length} mods`}`, () => api.updateMods(ids))) {
      updates = new Set([...updates].filter((u) => !ids.includes(u)));
    }
  }

  /** After installing requirements, move them above the mods that need them. */
  async function settleOrder() {
    await tick();
    if (fixable) fixLoadOrder();
  }

  async function installWorkshop(id: string) {
    logOpen = true;
    const ok = await run(`Installing Workshop mod ${id}`, () => api.install([id]));
    if (ok) await settleOrder();
    return ok;
  }

  async function installMissing() {
    logOpen = true;
    let unresolved: string[] = [];
    const ok = await run(`Finding ${missingReqs.size} missing ${missingReqs.size === 1 ? "mod" : "mods"} on the Workshop`, async () => {
      const [v, left] = await api.installMissing();
      unresolved = left;
      return v;
    });
    if (ok) {
      await settleOrder();
      if (unresolved.length) say(`Still missing: ${unresolved.join(", ")}. Select a mod that needs one and use Find to pick a match.`, true);
    }
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
      logOpen = true;
      await run(`Importing "${shared.name}"`, () => api.profileImport(shared));
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
    bisectApply({ original: $state.snapshot(rows), suspects, testing: [], round: 1 });
  }

  function bisectAnswer(stillBroken: boolean) {
    if (!bisect) return;
    const suspects = stillBroken ? bisect.testing : bisect.suspects.filter((s) => !bisect!.testing.includes(s));
    if (suspects.length <= 1) {
      const original = bisect.original;
      bisect = null;
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
    return run("Saving", () => api.save(next), true);
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

  function move(from: number, to: number) {
    if (from === to || to < 0 || to > rows.length) return;
    const next = [...rows];
    const [item] = next.splice(from, 1);
    next.splice(to > from ? to - 1 : to, 0, item);
    commit(next);
  }

  async function install(e: Event) {
    e.preventDefault();
    const sources = installText.split(/\s+/).filter(Boolean);
    if (!sources.length) return;
    installText = "";
    logOpen = true;
    await run(`Installing ${sources.length === 1 ? sources[0] : `${sources.length} mods`}`, () => api.install(sources));
  }

  /** A new separator above the selected mod (or at the top), ready to rename. */
  async function addSeparator() {
    const at = Math.max(0, selected ? rows.findIndex((r) => r.kind === "mod" && r.id === selected) : 0);
    await commit([...rows.slice(0, at), { kind: "sep", name: "New section" }, ...rows.slice(at)]);
    editingSep = at;
  }

  /** Drop a separator; its mods join the section above. */
  function removeSep(i: number) {
    commit(rows.filter((_, j) => j !== i));
  }

  function renameSep(i: number, name: string) {
    editingSep = null;
    const r = rows[i];
    if (r?.kind !== "sep") return;
    commit(name.trim() ? rows.map((x, j) => (j === i ? { kind: "sep", name: name.trim() } : x)) : rows.filter((_, j) => j !== i));
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

  const separators = $derived(rows.filter((r) => r.kind === "sep").map((r) => (r as { name: string }).name));

  function remove(id: string) {
    if (confirm(`Remove ${id}? Its files are deleted from your Zomboid mods folder.`)) run(`Removing ${id}`, () => api.remove([id]));
  }

  function deleteProfile() {
    const name = view?.profile;
    const other = view?.profiles.find((p) => p !== name);
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

<svelte:window onkeydown={globalKey} />

<div class="app">
  <header class="top" data-tauri-drag-region>
    <WindowControls />
    <img class="brand" src="/brand/pzmm.png" alt="PZMM" width="44" height="44" />

    <div class="modes" role="radiogroup" aria-label="Game">
      {#each MODES as [m, label]}
        <button role="radio" aria-checked={view?.mode === m} class:active={(view?.mode ?? "zomboid") === m}
          disabled={busy} onclick={() => planMode(m)} title={label}>
          <img src={m === "zomboid" ? "/brand/project-zomboid.png" : "/brand/project-viewpoint.png"} alt={label} height="26" />
        </button>
      {/each}
    </div>

    <div class="profile">
      <label for="profile">Profile</label>
      {#if newProfile !== null}
        <form onsubmit={createProfile} class="inline">
          <!-- svelte-ignore a11y_autofocus -->
          <input type="text" bind:value={newProfile} placeholder="Name, e.g. Friends server" autofocus
            onkeydown={(e) => e.key === "Escape" && (newProfile = null)} />
          <button type="submit" disabled={!newProfile.trim()}>Create</button>
        </form>
      {:else}
        <select id="profile" value={view?.profile} disabled={busy}
          onchange={(e) => run("Switching profile", () => api.profileUse(e.currentTarget.value))}>
          {#each modeProfiles as p}
            <option value={p}>{p}</option>
          {/each}
        </select>
        <button class="quiet" onclick={() => (newProfile = "")} disabled={busy} title="New profile from the current one"><Icon name="plus" size={14} />New</button>
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
      <button class="primary" type="submit" disabled={busy || !installText.trim()}><Icon name="download" size={15} />Install</button>
    </form>

  </header>
  {#snippet actions()}
    <div class="right">
      {#if updates.size}
        <button class="primary" onclick={() => updateMods([...updates])} disabled={busy}><Icon name="refresh" size={14} />Update {updates.size} {updates.size === 1 ? "mod" : "mods"}</button>
      {:else}
        <button onclick={() => { logOpen = true; run("Updating mods", api.updateAll).then(() => (updates = new Set())); }} disabled={busy}
          title="Re-download every mod PZMM installed"><Icon name="refresh" size={14} />Update all</button>
      {/if}
      <button onclick={() => api.openUrl("steam://rungameid/108600")}><Icon name="play" size={14} />Launch game</button>
      <button class="quiet" aria-expanded={settingsOpen} onclick={() => (settingsOpen = !settingsOpen)}><Icon name="settings" size={15} />Settings</button>
    </div>
  {/snippet}

  {#if importText !== null}
    <form class="mode-plan" onsubmit={importProfile}>
      <label for="import-code"><strong>Import a profile</strong>: paste the share code someone sent you. Missing Workshop mods are installed.</label>
      <div class="row">
        <!-- svelte-ignore a11y_autofocus -->
        <input id="import-code" type="text" bind:value={importText} onkeydown={(e) => e.key === "Escape" && (importText = null)} placeholder="PZMM1:…" autofocus spellcheck="false" style="flex: 1" />
        <button class="primary" type="submit" disabled={!importText.trim()}>Import</button>
        <button class="quiet" type="button" onclick={() => (importText = null)}>Cancel</button>
      </div>
    </form>
  {/if}

  {#if modePlan}
    <div class="mode-plan" role="region" aria-label="Switch game">
      {#if modePlan.error}
        <p class="error">{modePlan.error}</p>
        <button class="quiet" onclick={() => (modePlan = null)}>Close</button>
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
            <button class="primary" onclick={switchMode} disabled={busy}>Switch</button>
            <button class="quiet" onclick={() => (modePlan = null)}>Cancel</button>
          </div>
        {:else}
          <p class="hint-line">Checking what's needed…</p>
        {/if}
      {/if}
    </div>
  {/if}

  {#if appUpdate}
    <div class="app-update" role="status">
      {#if appProgress === null}
        <span><strong>PZMM {appUpdate.version} is available.</strong> You have {appVersion}.</span>
        <button class="quiet" onclick={() => api.openUrl(`https://github.com/SEVNC3/pz-mod-manager/releases/tag/v${appUpdate?.version}`)}>What's new</button>
        <button class="primary" onclick={installAppUpdate}>Update and restart</button>
      {:else}
        <span>Downloading PZMM {appUpdate.version}… {appProgress}%</span>
        <progress max="100" value={appProgress}></progress>
      {/if}
    </div>
  {/if}

  {#if settingsOpen}
    <div class="settings" role="region" aria-label="Settings">
      <label>Theme
        <select bind:value={theme}>
          <option value="system">Match system</option>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>
      <label>Accent <input type="color" bind:value={accent} /></label>
      <label>Game build
        <select value={String(view?.build ?? "")} disabled={busy}
          onchange={(e) => { const v = e.currentTarget.value; run("Saving", () => api.setBuild(v ? (Number(v) as 41 | 42) : null)); }}>
          <option value="">Detect from default.txt</option>
          <option value="42">Build 42</option>
          <option value="41">Build 41</option>
        </select>
      </label>
      <span class="keys">
        Keyboard: <kbd>/</kbd> search · <kbd>↑</kbd><kbd>↓</kbd> move between rows · <kbd>Enter</kbd> details ·
        <kbd>Space</kbd> enable/disable · <kbd>Alt</kbd>+<kbd>↑</kbd><kbd>↓</kbd> reorder · <kbd>Ctrl</kbd>+<kbd>Z</kbd>/<kbd>Y</kbd> undo/redo ·
        <kbd>Ctrl</kbd>/<kbd>Shift</kbd>+click select several · <kbd>Esc</kbd> clear selection
      </span>
      <span class="version">PZMM {appVersion}
        <button class="quiet" onclick={() => checkAppUpdate(true)}>Check for updates</button></span>
      <div class="paths">
        <span>Zomboid folder <button class="quiet link mono" onclick={() => view && api.openPath(view.zomboid)}>{view?.zomboid}</button></span>
        <span>Steam Workshop folder <span class="mono">{view?.workshop ?? "not found"}</span></span>
      </div>
    </div>
  {/if}

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

  {#if tab === "workshop"}
    <Workshop {busy} defaultBuild={view?.build ?? null} onInstall={installWorkshop} viewpoint={view?.mode === "viewpoint"} />
  {:else if tab === "servers"}
    <ServersSaves {busy} profile={view?.profile ?? ""} enabledIds={[...mods.values()].filter((m) => m.on).map((m) => m.id)}
      onImport={(name) => { logOpen = true; return run(`Importing ${name}.ini`, () => api.serverImport(name)); }}
      onUseSave={(mode, name) => run(`Loading the mods of ${name}`, () => api.saveUse(mode, name))} />
  {:else if tab === "zb"}
    <ZombieBuddy status={zb} profile={view?.profile ?? ""} {busy}
      onSetup={async () => { logOpen = true; await run("Setting up ZombieBuddy", api.zbInstall); }}
      onRemove={async () => { logOpen = true; await run("Removing ZombieBuddy", api.zbUninstall); }} />
  {:else}
  <section class="mods">
  {#if saveDiff && latestSave && !saveNoticeDismissed}
    <div class="banner save" role="status">
      <span>
        Your latest save, <strong>{latestSave.name}</strong>, was played with a different mod list:
        {[saveDiff.missing.length && `${saveDiff.missing.length} of its mods aren't enabled`, saveDiff.extra.length && `${saveDiff.extra.length} enabled mods weren't in it`].filter(Boolean).join(", ")}.
        Continuing it with this list can break it.
      </span>
      <button onclick={() => latestSave && run(`Loading the mods of ${latestSave.name}`, () => api.saveUse(latestSave!.mode, latestSave!.name))}>Use this save's mods</button>
      <button class="quiet" onclick={() => (saveNoticeDismissed = true)}>Dismiss</button>
    </div>
  {/if}
  {#if zbNeeded}
    <div class="banner" role="status">
      <span>{zb?.required_by.length} enabled {zb?.required_by.length === 1 ? "mod needs" : "mods need"} ZombieBuddy, which isn't set up{zb?.files_current || zb?.launcher_patched ? " completely (a game update may have reset it)" : ""}.</span>
      <button onclick={() => (tab = "zb")}>Set up ZombieBuddy</button>
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
    {#if picked.size >= 1}
      <span class="bulk" role="group" aria-label="Selected mods">
        <strong>{picked.size} selected</strong>
        <button onclick={() => bulkSet(true)} disabled={busy}>Enable</button>
        <button onclick={() => bulkSet(false)} disabled={busy}>Disable</button>
        <select aria-label="Move selected mods" disabled={busy || filtering}
          onchange={(e) => { const v = e.currentTarget.value; e.currentTarget.value = ""; if (v) bulkMove(v); }}>
          <option value="">Move to…</option>
          <option value="top">Top of the list</option>
          {#each separators as s}<option value={s}>Under "{s}"</option>{/each}
          <option value="bottom">Bottom of the list</option>
        </select>
        <button class="danger" onclick={bulkRemove} disabled={busy}>Remove</button>
        <button class="quiet" onclick={() => (picked = new Set())}>Clear</button>
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
    <button class="quiet icon" onclick={undo} disabled={busy || !undoStack.length} title="Undo the last list change (Ctrl+Z)" aria-label="Undo"><Icon name="undo" size={15} /></button>
    <button class="quiet icon" onclick={redo} disabled={busy || !redoStack.length} title="Redo (Ctrl+Y)" aria-label="Redo"><Icon name="redo" size={15} /></button>
    <button class="quiet" onclick={addSeparator} disabled={busy || filtering} title="Add a separator above the selected mod"><Icon name="plus" size={14} />Separator</button>
    </span>
  </div>

  {#if bisect}
    <div class="findings" role="region" aria-label="Find the broken mod">
      <p>
        <strong>Finding the broken mod, round {bisect.round}:</strong>
        {bisect.suspects.length} suspects left, {bisect.testing.length} enabled for this test (plus what they need).
        Start the game, check whether the problem happens, then come back here.
      </p>
      <div class="row">
        <button class="primary" onclick={() => bisectAnswer(true)} disabled={busy}>The problem still happens</button>
        <button class="primary" onclick={() => bisectAnswer(false)} disabled={busy}>The problem is gone</button>
        <button class="quiet" onclick={bisectStop} disabled={busy}>Stop and restore my list</button>
      </div>
    </div>
  {/if}

  {#if findings}
    <div class="findings" role="region" aria-label="Setup check">
      <div class="findings-head">
        <strong>
          {findings.length === 0 ? "No problems found." : `${findings.filter((f) => f.level === "error").length} errors, ${findings.filter((f) => f.level === "warning").length} warnings`}
        </strong>
        <button class="quiet" onclick={() => (findings = null)}>Close</button>
      </div>
      {#if findings.length}
        <ul>
          {#each findings as f}
            <li class={f.level}><span class="mono">{f.subject}</span> {f.text}</li>
          {/each}
        </ul>
      {/if}
      {#if gameErrors.length}
        <p class="findings-sub"><strong>Lua errors in your last game session</strong> (from console.txt):</p>
        <ul>
          {#each gameErrors as e}
            <li class="warning">
              <button class="quiet link" disabled={!e.id} onclick={() => e.id && (selected = e.id)}>{e.name}</button>:
              {e.count} {e.count === 1 ? "error" : "errors"}, first at <span class="mono">{e.example}</span>
            </li>
          {/each}
        </ul>
      {/if}
      <p class="findings-sub">
        Something breaks but nothing above explains it?
        <button class="quiet link" onclick={bisectStart} disabled={busy}>Find the broken mod</button>:
        PZMM halves your enabled mods each time you test the game.
      </p>
    </div>
  {/if}
  <main class:with-details={selected} class:with-panel={!selected && rows.length}>
    <div class="list">
      {#if !view}
        <p class="loading">Loading your mods…</p>
      {:else if rows.length === 0}
        <div class="empty">
          <p class="marker">Nothing in the stockpile yet.</p>
          <p>Paste a Steam Workshop link above to install a mod, or subscribe to mods in Steam: subscriptions show up here automatically.</p>
        </div>
      {:else}
        <table>
          <thead class="visually-hidden">
            <tr><th>Move</th><th>Enabled</th><th>Load order</th><th>Mod</th><th>Size</th><th>Last changed</th><th>Shared files</th><th>Source</th></tr>
          </thead>
          <tbody>
            {#each visible as { r, i } (r.kind === "mod" ? `m:${r.id}` : `s:${i}:${r.name}`)}
              {#if r.kind === "sep"}
                <tr class="sep" class:drop={dropAt === i} data-index={i} tabindex="0" draggable={!filtering && editingSep !== i}
                  ondragstart={() => (dragFrom = i)} ondragend={() => { dragFrom = dropAt = null; }}
                  ondragover={(e) => { e.preventDefault(); dropAt = i; }}
                  ondrop={(e) => { e.preventDefault(); if (dragFrom !== null) move(dragFrom, i); dragFrom = dropAt = null; }}
                  ondblclick={() => (editingSep = i)} onkeydown={(e) => editingSep !== i && rowKey(e, i, r)}>
                  <td class="grip" aria-hidden="true">⠿</td>
                  <td colspan="7">
                    {#if editingSep === i}
                      <!-- svelte-ignore a11y_autofocus -->
                      <input class="tape-edit" type="text" value={r.name} autofocus aria-label="Separator name (empty removes it)"
                        onblur={(e) => renameSep(i, e.currentTarget.value)}
                        onkeydown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") editingSep = null; }} />
                    {:else}
                      <button class="quiet fold" aria-expanded={!collapsed.has(r.name)} aria-label={`${collapsed.has(r.name) ? "Expand" : "Collapse"} ${r.name}`}
                        onclick={(e) => { e.stopPropagation(); toggleSection(r.name); }}>{collapsed.has(r.name) ? "▸" : "▾"}</button>
                      <span class="tape" title="Double-click to rename">{r.name}</span>
                      {#if sectionCounts.get(r.name)}
                        {@const c = sectionCounts.get(r.name)!}
                        <span class="sec-count">{c.on} of {c.total} enabled</span>
                      {/if}
                      <button class="quiet icon sep-remove" disabled={busy || filtering} title="Remove this separator (Delete). Its mods stay where they are."
                        aria-label={`Remove separator ${r.name}`} onclick={(e) => { e.stopPropagation(); removeSep(i); }}><Icon name="trash" size={14} /></button>
                    {/if}
                  </td>
                </tr>
              {:else}
                {@const src = describeSource(r.source)}
                <tr class="mod" class:off={!r.on} class:selected={selected === r.id} class:picked={picked.size >= 1 && picked.has(r.id)} class:drop={dropAt === i} class:dragging={dragFrom === i}
                  data-index={i} tabindex="0" draggable={!filtering}
                  aria-selected={selected === r.id}
                  onclick={(e) => rowClick(e, r.id)} onkeydown={(e) => rowKey(e, i, r)}
                  ondragstart={() => (dragFrom = i)} ondragend={() => { dragFrom = dropAt = null; }}
                  ondragover={(e) => { e.preventDefault(); dropAt = i; }}
                  ondrop={(e) => { e.preventDefault(); if (dragFrom !== null) move(dragFrom, i); dragFrom = dropAt = null; }}>
                  <td class="grip" aria-hidden="true">{filtering ? "" : "⠿"}</td>
                  <td class="check">
                    <input type="checkbox" checked={r.on} disabled={busy} aria-label={`Enable ${r.name || r.id}`}
                      onclick={(e) => e.stopPropagation()} onchange={() => toggle(r.id)} />
                  </td>
                  <td class="num mono">{order.get(i) ?? ""}</td>
                  <td class="name">
                    <span class="title" title={r.name || r.id}>{r.name || r.id}</span>
                    {#if r.name && r.name !== r.id}<span class="id mono">{r.id}</span>{/if}
                    {#if r.only41 && (view?.build ?? 42) !== 41}
                      <span class="build-badge" title="Built for Build 41 only (no 42/ or common/ folder): on Build 42 it may error or do nothing">B41 only</span>
                    {:else if r.only42 && view?.build === 41}
                      <span class="build-badge" title="Built for Build 42 only">B42 only</span>
                    {/if}
                  </td>
                  <td class="meta mono">{stats[r.id] ? fmtSize(stats[r.id].size) : ""}</td>
                  <td class="meta mono">{stats[r.id] ? fmtDate(stats[r.id].modified) : ""}</td>
                  <td class="overlap">
                    {#if r.on && overlaps[r.id]}
                      {@const o = overlaps[r.id]}
                      <LayerIcon wins={o.wins_over.length > 0} loses={o.loses_to.length > 0} label={overlapLabel(o)} />
                    {/if}
                  </td>
                  <td class="src">
                    {#if r.missing}
                      <span class="missing">Not installed</span>
                    {:else if problems.has(r.id)}
                      <span class="missing" title={problems.get(r.id)?.text}><Icon name="alert" size={13} />{problems.get(r.id)?.text}</span>
                    {:else if updates.has(r.id)}
                      <span class="update"><Icon name="update" size={13} />Update available</span>
                    {:else}
                      <span class="badge" data-kind={r.source.split(":")[0]} title={SOURCE_HINTS[r.source.split(":")[0]] ?? "Added to the mods folder outside PZMM"}>{src.label}</span>
                    {/if}
                  </td>
                </tr>
              {/if}
            {/each}
            {#if !filtering}
              <tr class="tail" class:drop={dropAt === rows.length} aria-hidden="true"
                ondragover={(e) => { e.preventDefault(); dropAt = rows.length; }}
                ondrop={(e) => { e.preventDefault(); if (dragFrom !== null) move(dragFrom, rows.length); dragFrom = dropAt = null; }}>
                <td colspan="8"></td>
              </tr>
            {/if}
          </tbody>
        </table>
      {/if}
    </div>

    {#if selected}
      <Details id={selected} {mods} {busy} hasUpdate={updates.has(selected)} onUpdate={(id) => updateMods([id])} onInstallWorkshop={installWorkshop}
        onToggle={toggle} onSelect={(id) => (selected = id)}
        onRemove={remove} onRollback={(id) => run(`Restoring ${id}`, () => api.rollback(id))} onClose={() => (selected = null)} />
    {:else if rows.length}
      <LoadOrder {rows} {problems} {overlaps} {busy} onAutosort={() => autosortList()} onFix={fixLoadOrder} />
    {/if}
  </main>
  </section>
  {/if}

  <footer class="activity" class:open={logOpen}>
    <button class="quiet bar" aria-expanded={logOpen} onclick={() => (logOpen = !logOpen)}>
      <span class="dot" class:busy aria-hidden="true"></span>
      <span class="label">{busy ? busyLabel + "…" : "Activity"}</span>
      {#if log.length}<Icon name={logIcon(log.at(-1)!.text, log.at(-1)!.error)} size={13} />{/if}
      <span class="last">{log.at(-1)?.text ?? ""}</span>
    </button>
    {#if logOpen}
      <ol class="log mono" aria-live="polite">
        {#each log as l}
          <li class:error={l.error}><Icon name={logIcon(l.text, l.error)} size={13} /><span>{l.text}</span></li>
        {:else}
          <li class="quiet-line">Installs, updates and errors show up here.</li>
        {/each}
      </ol>
    {/if}
  </footer>
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
  .tabs button.active {
    color: #fff;
    background: var(--accent-soft);
    border-bottom-color: #fff;
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
  .modes {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .modes button {
    border: 0;
    border-radius: 0;
    background: transparent;
    padding: 4px 10px;
    color: var(--muted);
  }
  .modes img {
    display: block;
    height: 26px;
    width: auto;
    opacity: 0.45;
    transition: opacity 0.15s;
  }
  .modes button:hover img {
    opacity: 0.75;
  }
  .modes button.active {
    background: var(--accent-soft);
  }
  .modes button.active img {
    opacity: 1;
  }
  /* the wordmarks are white: darken them on the light theme */
  :global(:root[data-theme="light"]) .modes img {
    filter: invert(1);
  }
  @media (prefers-color-scheme: light) {
    :global(:root:not([data-theme="dark"])) .modes img {
      filter: invert(1);
    }
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
  .mode-plan .error {
    color: var(--danger);
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
    width: 220px;
    accent-color: var(--accent);
  }
  .version {
    color: var(--muted);
  }
  .findings {
    padding: 8px 16px 10px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
    max-height: 30vh;
    overflow-y: auto;
  }
  .findings-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .findings-sub {
    margin: 8px 0 0;
    color: var(--muted);
  }
  .findings .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .findings ul {
    margin: 6px 0 0;
    padding-left: 18px;
  }
  .findings li.error {
    color: var(--danger);
  }
  .findings li.warning {
    color: var(--text);
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
  .inline,
  .right {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .profile label {
    color: var(--muted);
  }
  .install {
    display: flex;
    gap: 6px;
    flex: 1 1 200px;
  }
  .install input {
    flex: 1;
  }
  .right {
    margin-left: auto;
  }

  .settings {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 18px;
    padding: 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .settings label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }
  .settings input[type="color"] {
    width: 34px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: none;
  }
  .paths {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--muted);
    margin-left: auto;
  }
  .link {
    padding: 0 4px;
    text-decoration: underline;
    text-underline-offset: 2px;
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
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    border: 0;
    border-radius: 0;
    background: transparent;
    padding: 5px 12px;
    color: var(--muted);
  }
  .seg button:hover:not(:disabled),
  .tabs button:hover {
    color: var(--text);
  }
  .seg button.active {
    background: var(--accent-soft);
    color: #fff;
    font-weight: 700;
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
    color: var(--muted);
    font-size: 12px;
    margin-left: auto;
  }
  .spacer {
    margin-left: auto;
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
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  td {
    padding: 0 8px;
    height: 40px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  tr.mod {
    cursor: default;
  }
  tr.mod:hover {
    background: color-mix(in oklab, var(--raised) 70%, transparent);
  }
  .fold {
    padding: 0 6px;
    margin-right: 4px;
    color: var(--muted);
  }
  .sec-count {
    margin-left: 12px;
    color: var(--muted);
    font-size: 12px;
  }
  tr.mod.picked {
    background: color-mix(in oklab, var(--accent) 22%, transparent);
    box-shadow: inset 3px 0 0 var(--accent-text);
  }
  .bulk {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    flex-wrap: wrap;
  }
  tr.mod.selected {
    background: var(--accent-soft);
    box-shadow: inset 3px 0 0 var(--accent-text);
  }
  tr.off .title,
  tr.off .num {
    color: var(--muted);
  }
  tr.dragging {
    opacity: 0.4;
  }
  tr.drop td {
    box-shadow: inset 0 3px 0 var(--accent-text);
  }
  tr:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .grip {
    width: 22px;
    color: var(--muted);
    cursor: grab;
    text-align: center;
    padding-right: 0;
  }
  .check {
    width: 28px;
  }
  .check input {
    accent-color: var(--accent);
    width: 16px;
    height: 16px;
  }
  .num {
    width: 40px;
    text-align: right;
    color: var(--accent-text);
  }
  /* max-width: 0 lets an auto-layout cell shrink, so long names ellipsize instead of wrapping */
  .name {
    max-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .name .title {
    font-weight: 700;
  }
  .name .id {
    margin-left: 10px;
    color: var(--muted);
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
  .keys {
    flex-basis: 100%;
    color: var(--muted);
    font-size: 12px;
  }
  kbd {
    font-family: var(--font-mono);
    font-size: 11px;
    border: 1px solid var(--line);
    border-bottom-width: 2px;
    border-radius: 3px;
    padding: 0 4px;
    background: var(--raised);
  }
  .build-badge {
    margin-left: 8px;
    font-size: 11px;
    font-weight: 700;
    color: var(--danger);
    border: 1px solid color-mix(in oklab, var(--danger) 50%, transparent);
    border-radius: 4px;
    padding: 0 5px;
  }
  .meta {
    width: 78px;
    text-align: right;
    color: var(--muted);
    white-space: nowrap;
  }
  .sort {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }
  /* its own column so the icons line up whatever the badge beside them says */
  .overlap {
    width: 28px;
    padding-left: 8px;
    padding-right: 0;
    text-align: center;
  }
  .src {
    width: 1%;
    white-space: nowrap;
    text-align: right;
  }
  .badge {
    font-size: 12px;
    color: var(--muted);
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 1px 8px;
  }
  .badge[data-kind="github"],
  .badge[data-kind="workshop"],
  .badge[data-kind="local"] {
    color: var(--text);
  }
  .update {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--accent-text);
    font-weight: 700;
    font-size: 12px;
  }
  .missing {
    display: inline-block;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: middle;
    color: var(--danger);
    font-weight: 700;
    font-size: 12px;
  }
  .missing :global(.icon) {
    margin-right: 4px;
  }
  tr.tail td {
    height: 48px;
    border: 0;
  }

  /* signature: separators are masking-tape labels, like crates in a safehouse */
  tr.sep td {
    height: 46px;
    border-bottom: 0;
  }
  .tape,
  .tape-edit {
    display: inline-block;
    font-family: var(--font-marker);
    font-size: 16px;
    letter-spacing: 0.02em;
    color: var(--tape-ink);
    background: var(--tape);
    padding: 2px 16px 0;
    transform: rotate(-0.8deg);
    clip-path: polygon(0 8%, 3% 0, 6% 10%, 9% 1%, 100% 0, 98% 30%, 100% 55%, 97% 80%, 100% 100%, 4% 100%, 1% 88%, 3% 60%, 0 40%);
    box-shadow: 0 1px 0 rgb(0 0 0 / 0.15);
  }
  .tape {
    padding-right: 72px;
    background: url(/blood.svg) right -10px center / auto 140% no-repeat, var(--tape);
  }
  .sep-remove {
    float: right;
    margin-top: 6px;
    color: var(--muted);
    opacity: 0;
  }
  tr.sep:hover .sep-remove,
  tr.sep:focus-within .sep-remove {
    opacity: 1;
  }
  .sep-remove:hover:not(:disabled) {
    color: var(--danger);
  }
  .tape-edit {
    border: 0;
    border-radius: 0;
    outline-offset: 3px;
    min-width: 220px;
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

  /* --- activity --- */
  .activity {
    border-top: 1px solid var(--line);
    background: var(--panel);
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 10px;
    width: 100%;
    padding: 7px 16px;
    border-radius: 0;
    text-align: left;
  }
  .label {
    font-weight: 700;
  }
  .last {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--line);
  }
  .dot.busy {
    background: var(--accent-text);
    animation: pulse 1s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.3;
    }
  }
  .log {
    list-style: none;
    margin: 0;
    padding: 4px 16px 10px;
    max-height: 180px;
    overflow-y: auto;
  }
  .log li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 2px 0;
    overflow-wrap: anywhere;
  }
  .log li :global(.icon) {
    margin-top: 3px;
    color: var(--muted);
  }
  .log li.error :global(.icon) {
    color: var(--danger);
  }
  .log .error {
    color: var(--danger);
  }
  .quiet-line {
    color: var(--muted);
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
