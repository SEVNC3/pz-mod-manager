<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getVersion } from "@tauri-apps/api/app";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { api, describeSource, zbReady, type Mode, type ModRow, type Row, type View, type ZbStatus } from "$lib/api";
  import Details from "$lib/Details.svelte";
  import Workshop from "$lib/Workshop.svelte";
  import ZombieBuddy from "$lib/ZombieBuddy.svelte";
  import ServersSaves from "$lib/ServersSaves.svelte";
  import LayerIcon from "$lib/LayerIcon.svelte";
  import Links from "$lib/Links.svelte";
  import { findProblems, sortLoadOrder } from "$lib/order";
  import type { Overlap } from "$lib/api";

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
  let accent = $state(pref("pzmm.accent", "#d39b3a"));
  $effect(() => {
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
    root.style.setProperty("--accent", accent);
    try {
      localStorage.setItem("pzmm.theme", theme);
      localStorage.setItem("pzmm.accent", accent);
    } catch {}
  });

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
  const filtering = $derived(search.trim() !== "" || filter !== "all");
  const visible = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return rows
      .map((r, i) => ({ r, i }))
      .filter(({ r }) => {
        if (r.kind === "sep") return !filtering;
        if (filter === "on" && !r.on) return false;
        if (filter === "off" && r.on) return false;
        if (filter === "problems" && !problems.has(r.id) && !r.missing) return false;
        return !q || r.id.toLowerCase().includes(q) || r.name.toLowerCase().includes(q);
      });
  });

  // --- actions ---
  function say(text: string, error = false) {
    log.push({ text, error });
    if (error) logOpen = true;
  }

  /** Run a backend job that returns the new view. Errors go to the activity log; returns success. */
  async function run(label: string, job: () => Promise<View>, keepHistory = false): Promise<boolean> {
    // undo covers list edits only: anything else (installs, profile switches) resets it
    if (!keepHistory) undoStack = redoStack = [];
    busy = true;
    busyLabel = label;
    try {
      const v = await job();
      view = v;
      rows = v.rows;
      if (v.adopted) say(`Picked up changes made in the game's mod menu (profile "${v.profile}").`);
      if (selected && !v.rows.some((r) => r.kind === "mod" && r.id === selected)) selected = null;
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
    }
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
    const ok = await run(`Installing workshop item ${id}`, () => api.install([id]));
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

  function globalKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
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
    await run(`Installing ${sources.length === 1 ? sources[0] : `${sources.length} items`}`, () => api.install(sources));
  }

  async function addSeparator() {
    await commit([{ kind: "sep", name: "New section" }, ...rows]);
    editingSep = 0;
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

  function remove(id: string) {
    if (confirm(`Delete ${id} from your Zomboid mods folder?`)) run(`Removing ${id}`, () => api.remove([id]));
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
    } else if (e.key === "Enter" && r.kind === "sep") {
      editingSep = i;
    }
  }

  onMount(() => {
    const unlisten = listen<string>("log", (e) => say(e.payload, e.payload.startsWith("error:")));
    run("Loading", api.load).then(checkUpdates);
    getVersion().then((v) => (appVersion = v));
    checkAppUpdate();
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={globalKey} />

<div class="app">
  <header class="top">
    <div class="brand" aria-label="PZMM">PZMM</div>

    <div class="modes" role="radiogroup" aria-label="Game">
      {#each MODES as [m, label]}
        <button role="radio" aria-checked={view?.mode === m} class:active={(view?.mode ?? "zomboid") === m}
          disabled={busy} onclick={() => planMode(m)}>{label}</button>
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
        <button class="quiet" onclick={() => (newProfile = "")} disabled={busy} title="New profile from the current one">New</button>
        {#if modeProfiles.length > 1}
          <button class="quiet" onclick={deleteProfile} disabled={busy}>Delete</button>
        {/if}
      {/if}
    </div>

    <form class="install" onsubmit={install}>
      <label for="install" class="visually-hidden">Install a mod</label>
      <input id="install" type="text" bind:value={installText} autocomplete="off" spellcheck="false"
        placeholder="Paste a Workshop link, collection or ID, a GitHub repo (owner/repo), or a .zip path" />
      <button class="primary" type="submit" disabled={busy || !installText.trim()}>Install</button>
    </form>

    <div class="right">
      {#if updates.size}
        <button class="primary" onclick={() => updateMods([...updates])} disabled={busy}>Update {updates.size} {updates.size === 1 ? "mod" : "mods"}</button>
      {:else}
        <button onclick={() => { logOpen = true; run("Updating mods", api.updateAll).then(() => (updates = new Set())); }} disabled={busy}
          title="Re-download every mod PZMM installed">Update all</button>
      {/if}
      <button onclick={() => api.openUrl("steam://rungameid/108600")}>Launch game</button>
      <button class="quiet" aria-expanded={settingsOpen} onclick={() => (settingsOpen = !settingsOpen)}>Settings</button>
    </div>
  </header>

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
      <span class="version">PZMM {appVersion}
        <button class="quiet" onclick={() => checkAppUpdate(true)}>Check for updates</button></span>
      <div class="paths">
        <span>Zomboid folder <button class="quiet link mono" onclick={() => view && api.openPath(view.zomboid)}>{view?.zomboid}</button></span>
        <span>Steam Workshop folder <span class="mono">{view?.workshop ?? "not found"}</span></span>
      </div>
    </div>
  {/if}

  <div class="tabbar">
  <div class="tabs" role="tablist" aria-label="Sections">
    {#each [["mods", "My mods"], ["workshop", "Browse Workshop"], ["servers", "Servers & saves"], ["zb", "ZombieBuddy"]] as [id, label]}
      <button role="tab" aria-selected={tab === id} class:active={tab === id} onclick={() => (tab = id as typeof tab)}>
        {label}{#if id === "zb" && zbNeeded}<span class="alert" aria-label="needs setup"></span>{/if}
      </button>
    {/each}
  </div>
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
  {#if zbNeeded}
    <div class="banner" role="status">
      <span>{zb?.required_by.length} enabled {zb?.required_by.length === 1 ? "mod needs" : "mods need"} ZombieBuddy, which isn't set up{zb?.files_current || zb?.launcher_patched ? " completely (a game update may have reset it)" : ""}.</span>
      <button onclick={() => (tab = "zb")}>Set up ZombieBuddy</button>
    </div>
  {/if}
  <div class="filters">
    <input type="search" bind:value={search} placeholder="Search mods" aria-label="Search mods" />
    <div class="seg" role="radiogroup" aria-label="Show">
      {#each [["all", "All"], ["on", "Enabled"], ["off", "Disabled"], ["problems", `Problems${problemCount ? ` (${problemCount})` : ""}`]] as [v, label]}
        <button role="radio" aria-checked={filter === v} class:active={filter === v} onclick={() => (filter = v as typeof filter)}>{label}</button>
      {/each}
    </div>
    <span class="count">{enabledCount} enabled · {mods.size} mods</span>
    {#if missingReqs.size}
      <button class="primary" onclick={installMissing} disabled={busy}
        title={`Find and install from the Workshop: ${[...missingReqs].join(", ")}`}>Install missing mods ({missingReqs.size})</button>
    {/if}
    {#if fixable}
      <button class="primary" onclick={fixLoadOrder} disabled={busy}
        title="Enable required mods and order mods by their require / loadModAfter / loadModBefore rules">Fix load order</button>
    {/if}
    {#if !missingReqs.size && !fixable}
      <span class="hint">
        {filtering ? "Clear the search and filter to change load order." : "Drag to reorder · Alt+↑/↓ · Space toggles"}
      </span>
    {:else}
      <span class="spacer"></span>
    {/if}
    <button class="quiet" onclick={undo} disabled={busy || !undoStack.length} title="Undo the last list change (Ctrl+Z)">Undo</button>
    <button class="quiet" onclick={redo} disabled={busy || !redoStack.length} title="Redo (Ctrl+Y)">Redo</button>
    <button class="quiet" onclick={addSeparator} disabled={busy || filtering}>Add separator</button>
  </div>

  <main class:with-details={selected}>
    <div class="list">
      {#if view && rows.length === 0}
        <div class="empty">
          <p class="marker">Nothing in the stockpile yet.</p>
          <p>Paste a Steam Workshop link above to install a mod, or subscribe to mods in Steam: subscriptions show up here automatically.</p>
        </div>
      {:else}
        <table>
          <thead class="visually-hidden">
            <tr><th>Move</th><th>Enabled</th><th>Load order</th><th>Mod</th><th>Source</th></tr>
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
                  <td colspan="4">
                    {#if editingSep === i}
                      <!-- svelte-ignore a11y_autofocus -->
                      <input class="tape-edit" type="text" value={r.name} autofocus aria-label="Separator name (empty removes it)"
                        onblur={(e) => renameSep(i, e.currentTarget.value)}
                        onkeydown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") editingSep = null; }} />
                    {:else}
                      <span class="tape" title="Double-click to rename">{r.name}</span>
                    {/if}
                  </td>
                </tr>
              {:else}
                {@const src = describeSource(r.source)}
                <tr class="mod" class:off={!r.on} class:selected={selected === r.id} class:drop={dropAt === i} class:dragging={dragFrom === i}
                  data-index={i} tabindex="0" draggable={!filtering}
                  aria-selected={selected === r.id}
                  onclick={() => (selected = r.id)} onkeydown={(e) => rowKey(e, i, r)}
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
                    <span class="title">{r.name || r.id}</span>
                    {#if r.name && r.name !== r.id}<span class="id mono">{r.id}</span>{/if}
                  </td>
                  <td class="src">
                    {#if r.on && overlaps[r.id]}
                      {@const o = overlaps[r.id]}
                      <LayerIcon wins={o.wins_over.length > 0} loses={o.loses_to.length > 0} label={overlapLabel(o)} />
                    {/if}
                    {#if r.missing}
                      <span class="missing">Not installed</span>
                    {:else if problems.has(r.id)}
                      <span class="missing" title={problems.get(r.id)?.text}>⚠ {problems.get(r.id)?.text}</span>
                    {:else if updates.has(r.id)}
                      <span class="update">Update available</span>
                    {:else}
                      <span class="badge" data-kind={r.source.split(":")[0]}>{src.label}</span>
                    {/if}
                  </td>
                </tr>
              {/if}
            {/each}
            {#if !filtering}
              <tr class="tail" class:drop={dropAt === rows.length} aria-hidden="true"
                ondragover={(e) => { e.preventDefault(); dropAt = rows.length; }}
                ondrop={(e) => { e.preventDefault(); if (dragFrom !== null) move(dragFrom, rows.length); dragFrom = dropAt = null; }}>
                <td colspan="5"></td>
              </tr>
            {/if}
          </tbody>
        </table>
      {/if}
    </div>

    {#if selected}
      <Details id={selected} {mods} {busy} hasUpdate={updates.has(selected)} onUpdate={(id) => updateMods([id])} onInstallWorkshop={installWorkshop}
        onToggle={toggle} onSelect={(id) => (selected = id)}
        onRemove={remove} onRollback={(id) => run(`Restoring ${id}`, () => api.rollback(id))} />
    {/if}
  </main>
  </section>
  {/if}

  <footer class="activity" class:open={logOpen}>
    <button class="quiet bar" aria-expanded={logOpen} onclick={() => (logOpen = !logOpen)}>
      <span class="dot" class:busy aria-hidden="true"></span>
      <span class="label">{busy ? busyLabel + "…" : "Activity"}</span>
      <span class="last">{log.at(-1)?.text ?? ""}</span>
    </button>
    {#if logOpen}
      <ol class="log mono" aria-live="polite">
        {#each log as l}
          <li class:error={l.error}>{l.text}</li>
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
    color: var(--text);
    font-weight: 700;
    border-bottom-color: var(--accent);
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
    padding: 5px 10px;
    color: var(--muted);
  }
  .modes button.active {
    background: var(--accent-soft);
    color: var(--accent-text);
    font-weight: 700;
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
  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 16px;
    background: color-mix(in oklab, var(--danger) 12%, var(--panel));
    border-bottom: 1px solid var(--line);
  }
  .banner span {
    flex: 1;
  }

  /* --- top bar --- */
  .top {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .brand {
    font-family: var(--font-mono);
    font-weight: 400;
    font-size: 15px;
    letter-spacing: 0.18em;
    padding: 3px 8px;
    border: 1.5px solid var(--accent);
    color: var(--accent-text);
    border-radius: 4px;
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
    gap: 12px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .filters input[type="search"] {
    width: 220px;
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
  }
  .seg button.active {
    background: var(--accent-soft);
    color: var(--accent-text);
    font-weight: 700;
  }
  .count {
    color: var(--muted);
  }
  .hint {
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
  main.with-details {
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
  tr.mod.selected {
    background: var(--accent-soft);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  tr.off .title,
  tr.off .num {
    color: var(--muted);
  }
  tr.dragging {
    opacity: 0.4;
  }
  tr.drop td {
    box-shadow: inset 0 2px 0 var(--accent);
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
  .name {
    overflow: hidden;
  }
  .name .title {
    font-weight: 700;
  }
  .name .id {
    margin-left: 10px;
    color: var(--muted);
  }
  .src :global(.layers) {
    margin-right: 6px;
  }
  .src {
    width: 250px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
    color: var(--accent-text);
    font-weight: 700;
    font-size: 12px;
  }
  .missing {
    color: var(--danger);
    font-weight: 700;
    font-size: 12px;
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
    background: var(--accent);
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
    padding: 1px 0;
    overflow-wrap: anywhere;
  }
  .log .error {
    color: var(--danger);
  }
  .quiet-line {
    color: var(--muted);
  }

  @media (max-width: 900px) {
    main.with-details {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr 45%;
    }
    .hint {
      display: none;
    }
  }
</style>
