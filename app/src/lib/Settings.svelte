<script lang="ts">
  import { api, type Folder, type SettingsView, type View } from "./api";
  import { fmtSize } from "./format";
  import Icon from "./Icon.svelte";
  import Links from "./Links.svelte";
  import { prefs } from "./prefs.svelte";

  let { cfg = $bindable(), view, busy, appVersion, say, onBuild, onRedownload, onCheckUpdate, onFoldersChanged }: {
    cfg: SettingsView | null;
    view: View | null;
    busy: boolean;
    appVersion: string;
    say: (text: string, error?: boolean) => void;
    onBuild: (build: 41 | 42 | null) => void;
    onRedownload: () => void;
    onCheckUpdate: () => void;
    onFoldersChanged: () => void;
  } = $props();

  const STATUS = { found: "Found", missing: "Not found", not_pz: "Not a PZ folder" };
  const hint = (f: Folder) => (f.key === "data" ? "profiles, installed mods, backups, SteamCMD" : f.key === "zomboid" ? "mods, saves, server configs, logs" : "");

  /** A PZMM data change waiting for "move the data or not". `path` "" is the default folder. */
  let moving = $state<{ path: string; to: string } | null>(null);
  let working = $state(false);

  async function setFolder(f: Folder, path: string, move = false) {
    if (f.key === "data" && !moving && !f.overridden && (path.trim() || f.custom)) {
      const to = path.trim() || f.detected || "";
      if (to !== f.path) return (moving = { path: path.trim(), to });
    }
    moving = null;
    working = true;
    try {
      cfg = await api.folderSet(f.key, path, move);
      onFoldersChanged();
    } catch (e) {
      say(String(e), true);
    } finally {
      working = false;
    }
  }

  async function browse(f: Folder) {
    const picked = await api.pickFolder(f.path ?? "").catch(() => null);
    if (picked) setFolder(f, picked);
  }

  const save = () => cfg && api.settingsSet($state.snapshot(cfg.settings)).catch((e) => say(`Couldn't save the settings: ${e}`, true));

  let token = $state("");
  async function saveToken(value: string) {
    try {
      await api.githubTokenSet(value);
      token = "";
      if (cfg) cfg.token_saved = !!value.trim();
      say(value.trim() ? "Saved the GitHub token in the system's credential store." : "Forgot the GitHub token.");
    } catch (e) {
      say(String(e), true);
    }
  }

  async function clearBackups() {
    if (!cfg || !confirm(`Delete all backups (${fmtSize(cfg.backups_size)})? Mods can't be restored to their previous version afterwards.`)) return;
    try {
      cfg.backups_size = await api.backupsClear();
    } catch (e) {
      say(String(e), true);
    }
  }

  async function copyDiagnostics() {
    try {
      await navigator.clipboard.writeText(await api.diagnostics());
      say("Copied the diagnostics (your user name is masked). Paste them into a bug report.");
    } catch (e) {
      say(`Couldn't copy the diagnostics: ${e}`, true);
    }
  }
</script>

<section class="settings blueprint" aria-label="Settings">
  <div class="inner">
  <div class="card">
    <h2>Folders</h2>
    <p class="lede">PZMM finds these by itself. Set one only if it's somewhere else. An environment variable wins over what's set here.</p>
    {#if cfg}
      <ul class="folders">
        {#each cfg.folders as f (f.key)}
          <li>
            <div class="head">
              <strong>{f.label}</strong>
              <span class="status {f.status}">{STATUS[f.status]}</span>
              {#if f.overridden}<span class="hint">overridden by <span class="mono">{f.env}</span></span>{:else if !f.custom && f.path}<span class="hint">detected</span>{/if}
            </div>
            {#if hint(f)}<span class="hint">{hint(f)}</span>{/if}
            <div class="row">
              <input type="text" class="mono path" value={f.path ?? ""} placeholder="Not found" spellcheck="false" aria-label={f.label}
                disabled={working} onchange={(e) => setFolder(f, e.currentTarget.value)} />
              <button disabled={working} onclick={() => browse(f)}><Icon name="folder" size={14} />Browse…</button>
              <button disabled={working} onclick={() => setFolder(f, "")} title="Forget the folder set here and find it again"><Icon name="search" size={14} />Detect</button>
              <button disabled={f.status === "missing" || !f.path} onclick={() => f.path && api.openPath(f.path)}><Icon name="external" size={14} />Open</button>
            </div>
            {#each f.warnings as w}<p class="warn"><Icon name="alert" size={13} />{w}</p>{/each}
            {#if f.key === "data" && moving}
              <div class="confirm" role="group" aria-label="Move PZMM data">
                <p>Move your PZMM data from <span class="mono">{f.path}</span> to <span class="mono">{moving.to}</span>? Without moving, PZMM starts empty there.</p>
                <button class="primary" disabled={busy} onclick={() => moving && setFolder(f, moving.path, true)}><Icon name="swap" size={14} />Move data</button>
                <button disabled={busy} onclick={() => moving && setFolder(f, moving.path, false)}>Switch without moving</button>
                <button class="quiet" onclick={() => (moving = null)}><Icon name="x" size={14} />Cancel</button>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p class="hint">Reading the folders…</p>
    {/if}
  </div>

  {#if cfg}
  <div class="card">
    <h2>Downloads and updates</h2>
    <div class="grid">
      <label for="upd">Check mods for updates</label>
      <select id="upd" bind:value={cfg.settings.update_check} onchange={save}>
        <option value="off">Never (use Check for updates)</option>
        <option value="start">When PZMM starts</option>
        <option value="1h">On start, then every hour</option>
        <option value="3h">On start, then every 3 hours</option>
      </select>
      <label for="act">When updates are found</label>
      <select id="act" bind:value={cfg.settings.update_action} onchange={save} disabled={cfg.settings.update_check === "off"}>
        <option value="notify">Tell me</option>
        <option value="install">Install them</option>
      </select>
      <label for="links">Open Workshop pages in</label>
      <select id="links" bind:value={cfg.settings.workshop_links} onchange={save}>
        <option value="steam">Steam (when it's running)</option>
        <option value="browser">The browser</option>
      </select>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={cfg.settings.app_updates} onchange={save} />Check for new versions of PZMM when it starts</label>
    </div>
    <h3>GitHub token</h3>
    <p class="hint">For mods installed from GitHub: a token raises GitHub's limit of 60 downloads an hour. It's kept in the system's credential store, never in a file.
      {#if cfg.token_env}<strong>GITHUB_TOKEN is set, so that one is used.</strong>{/if}</p>
    <form class="row" onsubmit={(e) => { e.preventDefault(); saveToken(token); }}>
      <input type="password" bind:value={token} autocomplete="off" spellcheck="false" aria-label="GitHub token" placeholder={cfg.token_saved ? "A token is saved" : "ghp_…"} />
      <button type="submit" disabled={!token.trim()}><Icon name="check" size={14} />Save token</button>
      {#if cfg.token_saved}<button type="button" onclick={() => saveToken("")}><Icon name="x" size={14} />Forget</button>{/if}
    </form>
    <h3>Repair</h3>
    <button onclick={onRedownload} disabled={busy} title="Repairs damaged copies; each keeps its current version as a backup"><Icon name="download" size={14} />Re-download mods PZMM installed</button>
  </div>

  <div class="card">
    <h2>Removing and backups</h2>
    <div class="grid">
      <label for="rm">Removed mods</label>
      <select id="rm" bind:value={cfg.settings.recycle} onchange={save}>
        <option value={true}>Go to the Recycle Bin</option>
        <option value={false}>Are deleted</option>
      </select>
      <label for="req">Also remove required mods nothing else uses</label>
      <select id="req" bind:value={cfg.settings.remove_requirements} onchange={save}>
        <option value="ask">Ask</option>
        <option value="always">Always</option>
        <option value="never">Never</option>
      </select>
      <label for="cap">Backups may use up to</label>
      <span class="row"><input id="cap" type="number" min="0" step="100" bind:value={cfg.settings.backup_cap_mb} onchange={save} /> MB <span class="hint">0 = no limit; the oldest go first</span></span>
    </div>
    <p class="hint">An update keeps the mod's previous version as a backup (Restore previous version in its details).</p>
    <div class="row">
      <span>Backups use <strong>{fmtSize(cfg.backups_size)}</strong></span>
      <button class="danger" disabled={busy || !cfg.backups_size} onclick={clearBackups}><Icon name="trash" size={14} />Clear backups</button>
    </div>
  </div>
  {/if}

  <div class="card">
    <h2>Appearance</h2>
    <div class="grid">
      <label for="theme">Theme</label>
      <select id="theme" bind:value={prefs.theme}>
        <option value="system">Match system</option>
        <option value="dark">Dark</option>
        <option value="light">Light</option>
      </select>
      <label for="accent">Accent</label>
      <input id="accent" type="color" bind:value={prefs.accent} />
      <label for="build">Game build</label>
      <select id="build" value={String(view?.build ?? "")} disabled={busy}
        onchange={(e) => { const v = e.currentTarget.value; onBuild(v ? (Number(v) as 41 | 42) : null); }}>
        <option value="">Detect from default.txt</option>
        <option value="42">Build 42</option>
        <option value="41">Build 41</option>
      </select>
      <label for="wsview">Workshop opens on</label>
      <select id="wsview" bind:value={prefs.workshopView}>
        <option value="browse">Browse</option>
        <option value="home">Home</option>
      </select>
      <label for="perpage">Workshop results per page</label>
      <select id="perpage" bind:value={prefs.perPage}>
        <option value={30}>30</option>
        <option value={50}>50</option>
      </select>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={prefs.compact} />Compact rows</label>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={prefs.showIds} />Show mod IDs beside names</label>
    </div>
    <p class="hint">The window remembers its size and position.</p>
  </div>

  {#if cfg}
  <div class="card">
    <h2>Launch</h2>
    <div class="grid">
      <label for="args">Extra game arguments</label>
      <input id="args" type="text" class="mono" bind:value={cfg.settings.launch_args} onchange={save} spellcheck="false" placeholder="-debug -nosteam" />
      <span></span>
      <span class="hint">Launch game passes these through Steam, which asks once to allow them.</span>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={cfg.settings.autosort_new} onchange={save} />Group a new profile's mods by category the first time it's used</label>
    </div>
  </div>
  {/if}

  <div class="card">
    <h2>About</h2>
    <div class="row">
      <span>PZMM {appVersion}</span>
      <button onclick={onCheckUpdate}><Icon name="update" size={14} />Check for updates</button>
      <Links />
    </div>
    <div class="row">
      <button disabled={!view} onclick={() => view && api.openPath(`${view.zomboid}/Logs`)}><Icon name="folder" size={14} />Open log folder</button>
      <button onclick={copyDiagnostics} title="Version, folders, profile, mod count and Check setup results, with your user name masked"><Icon name="copy" size={14} />Copy diagnostics</button>
    </div>
    <p class="keys">
      Keyboard: <kbd>/</kbd> search · <kbd>↑</kbd><kbd>↓</kbd> move between rows · <kbd>Enter</kbd> details ·
      <kbd>Space</kbd> enable/disable · <kbd>Alt</kbd>+<kbd>↑</kbd><kbd>↓</kbd> reorder · <kbd>Ctrl</kbd>+<kbd>Z</kbd>/<kbd>Y</kbd> undo/redo ·
      <kbd>Ctrl</kbd>/<kbd>Shift</kbd>+click select several · <kbd>Esc</kbd> clear selection
    </p>
  </div>
  </div>
</section>

<style>
  .settings {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    max-width: 820px;
    margin: 0 auto;
    padding: 24px 16px 40px;
    display: grid;
    gap: 16px;
  }
  .card {
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
    padding: 20px 24px 22px;
    min-width: 0;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
  }
  h3 {
    margin: 20px 0 4px;
    font-size: 15px;
  }
  .hint {
    display: block;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 8px;
  }
  .row .hint {
    display: inline;
  }
  /* label | control pairs */
  .grid {
    display: grid;
    grid-template-columns: 260px 1fr;
    align-items: center;
    gap: 10px 16px;
    margin-top: 8px;
  }
  .grid > label:not(.check) {
    color: var(--muted);
  }
  .grid > select,
  .grid > input[type="text"] {
    justify-self: start;
    min-width: 260px;
  }
  .grid .row {
    margin: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  input[type="number"],
  input[type="password"] {
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 10px;
    font: inherit;
    color: inherit;
  }
  input[type="number"] {
    width: 100px;
  }
  input[type="password"] {
    flex: 0 1 320px;
  }
  input[type="color"] {
    width: 34px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: none;
  }
  .folders {
    list-style: none;
    padding: 0;
    margin: 8px 0 0;
    border-top: 1px solid var(--line);
  }
  .folders li {
    padding: 12px 0;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .head .hint {
    display: inline;
  }
  .path {
    flex: 1 1 300px;
  }
  .status {
    font-size: 12px;
    font-weight: 700;
    color: var(--muted);
  }
  .status.missing,
  .status.not_pz {
    color: var(--danger);
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--danger);
  }
  .confirm {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: 6px;
    background: color-mix(in oklab, var(--accent) 12%, var(--panel));
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .confirm p {
    flex: 1 1 100%;
    margin: 0;
    overflow-wrap: anywhere;
  }
  .keys {
    margin: 14px 0 0;
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
  .row :global(.links) {
    margin-left: auto;
  }
</style>
