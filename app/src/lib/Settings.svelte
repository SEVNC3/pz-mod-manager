<script lang="ts">
  import { api, type Folder, type SettingsView, type View } from "./api";
  import { fmtSize } from "./format";
  import { LANGS, systemLang, t, th, type Vars } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import Links from "./Links.svelte";
  import { prefs } from "./prefs.svelte";

  let { cfg = $bindable(), view, busy, appVersion, say, onBuild, onRedownload, onCheckUpdate, onFoldersChanged }: {
    cfg: SettingsView | null;
    view: View | null;
    busy: boolean;
    appVersion: string;
    say: (key: string, vars?: Vars, error?: boolean) => void;
    onBuild: (build: 41 | 42 | null) => void;
    onRedownload: () => void;
    onCheckUpdate: () => void;
    onFoldersChanged: () => void;
  } = $props();

  const hint = (f: Folder) => (f.key === "data" ? t("settings.folders.hintData") : f.key === "zomboid" ? t("settings.folders.hintZomboid") : "");

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
      say(String(e), {}, true);
    } finally {
      working = false;
    }
  }

  async function browse(f: Folder) {
    const picked = await api.pickFolder(f.path ?? "").catch(() => null);
    if (picked) setFolder(f, picked);
  }

  const save = () => cfg && api.settingsSet($state.snapshot(cfg.settings)).catch((e) => say("settings.saveFailed", { error: String(e) }, true));

  let token = $state("");
  async function saveToken(value: string) {
    try {
      await api.githubTokenSet(value);
      token = "";
      if (cfg) cfg.token_saved = !!value.trim();
      say(value.trim() ? "settings.tokenSaved" : "settings.tokenForgot");
    } catch (e) {
      say(String(e), {}, true);
    }
  }

  async function clearBackups() {
    if (!cfg || !confirm(t("settings.confirmClearBackups", { size: fmtSize(cfg.backups_size) }))) return;
    try {
      cfg.backups_size = await api.backupsClear();
    } catch (e) {
      say(String(e), {}, true);
    }
  }

  async function copyDiagnostics() {
    try {
      await navigator.clipboard.writeText(await api.diagnostics());
      say("settings.diagnosticsCopied");
    } catch (e) {
      say("settings.diagnosticsFailed", { error: String(e) }, true);
    }
  }
</script>

<section class="settings blueprint" aria-label={t("settings.label")}>
  <div class="inner">
  <div class="card">
    <h2>{t("settings.folders.title")}</h2>
    <p class="lede">{t("settings.folders.lede")}</p>
    {#if cfg}
      <ul class="folders">
        {#each cfg.folders as f (f.key)}
          <li>
            <div class="head">
              <strong>{f.label}</strong>
              <span class="status {f.status}">{t(`settings.folders.status.${f.status}`)}</span>
              {#if f.overridden}<span class="hint">{@html th("settings.folders.overriddenBy", { env: f.env ?? "" })}</span>{:else if !f.custom && f.path}<span class="hint">{t("settings.folders.detected")}</span>{/if}
            </div>
            {#if hint(f)}<span class="hint">{hint(f)}</span>{/if}
            <div class="row">
              <input type="text" class="mono path" value={f.path ?? ""} placeholder={t("settings.folders.status.missing")} spellcheck="false" aria-label={f.label}
                disabled={working} onchange={(e) => setFolder(f, e.currentTarget.value)} />
              <button disabled={working} onclick={() => browse(f)}><Icon name="folder" size={14} />{t("settings.folders.browse")}</button>
              <button disabled={working} onclick={() => setFolder(f, "")} title={t("settings.folders.detectTip")}><Icon name="search" size={14} />{t("settings.folders.detect")}</button>
              <button disabled={f.status === "missing" || !f.path} onclick={() => f.path && api.openPath(f.path)}><Icon name="external" size={14} />{t("settings.folders.open")}</button>
            </div>
            {#each f.warnings as w}<p class="warn"><Icon name="alert" size={13} />{w}</p>{/each}
            {#if f.key === "data" && moving}
              <div class="confirm" role="group" aria-label={t("settings.folders.moveLabel")}>
                <p>{@html th("settings.folders.moveConfirm", { from: f.path ?? "", to: moving.to })}</p>
                <button class="primary" disabled={busy} onclick={() => moving && setFolder(f, moving.path, true)}><Icon name="swap" size={14} />{t("settings.folders.moveData")}</button>
                <button disabled={busy} onclick={() => moving && setFolder(f, moving.path, false)}>{t("settings.folders.switchWithoutMoving")}</button>
                <button class="quiet" onclick={() => (moving = null)}><Icon name="x" size={14} />{t("common.cancel")}</button>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p class="hint">{t("settings.folders.reading")}</p>
    {/if}
  </div>

  {#if cfg}
  <div class="card">
    <h2>{t("settings.downloads.title")}</h2>
    <div class="grid">
      <label for="upd">{t("settings.downloads.check")}</label>
      <select id="upd" bind:value={cfg.settings.update_check} onchange={save}>
        <option value="off">{t("settings.downloads.checkOff")}</option>
        <option value="start">{t("settings.downloads.checkStart")}</option>
        <option value="1h">{t("settings.downloads.check1h")}</option>
        <option value="3h">{t("settings.downloads.check3h")}</option>
      </select>
      <label for="act">{t("settings.downloads.whenFound")}</label>
      <select id="act" bind:value={cfg.settings.update_action} onchange={save} disabled={cfg.settings.update_check === "off"}>
        <option value="notify">{t("settings.downloads.notify")}</option>
        <option value="install">{t("settings.downloads.install")}</option>
      </select>
      <label for="links">{t("settings.downloads.openIn")}</label>
      <select id="links" bind:value={cfg.settings.workshop_links} onchange={save}>
        <option value="steam">{t("settings.downloads.inSteam")}</option>
        <option value="browser">{t("settings.downloads.inBrowser")}</option>
      </select>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={cfg.settings.app_updates} onchange={save} />{t("settings.downloads.appUpdates")}</label>
    </div>
    <h3>{t("settings.token.title")}</h3>
    <p class="hint">{t("settings.token.hint")}
      {#if cfg.token_env}<strong>{t("settings.token.envSet")}</strong>{/if}</p>
    <form class="row" onsubmit={(e) => { e.preventDefault(); saveToken(token); }}>
      <input type="password" bind:value={token} autocomplete="off" spellcheck="false" aria-label={t("settings.token.title")} placeholder={cfg.token_saved ? t("settings.token.savedPlaceholder") : "ghp_…"} />
      <button type="submit" disabled={!token.trim()}><Icon name="check" size={14} />{t("settings.token.save")}</button>
      {#if cfg.token_saved}<button type="button" onclick={() => saveToken("")}><Icon name="x" size={14} />{t("settings.token.forget")}</button>{/if}
    </form>
    <h3>{t("settings.repair.title")}</h3>
    <button onclick={onRedownload} disabled={busy} title={t("settings.repair.redownloadTip")}><Icon name="download" size={14} />{t("settings.repair.redownload")}</button>
  </div>

  <div class="card">
    <h2>{t("settings.removing.title")}</h2>
    <div class="grid">
      <label for="rm">{t("settings.removing.removed")}</label>
      <select id="rm" bind:value={cfg.settings.recycle} onchange={save}>
        <option value={true}>{t("settings.removing.recycle")}</option>
        <option value={false}>{t("settings.removing.deleted")}</option>
      </select>
      <label for="req">{t("settings.removing.removeRequirements")}</label>
      <select id="req" bind:value={cfg.settings.remove_requirements} onchange={save}>
        <option value="ask">{t("settings.removing.ask")}</option>
        <option value="always">{t("settings.removing.always")}</option>
        <option value="never">{t("settings.removing.never")}</option>
      </select>
      <label for="cap">{t("settings.removing.backupCap")}</label>
      <span class="row"><input id="cap" type="number" min="0" step="100" bind:value={cfg.settings.backup_cap_mb} onchange={save} /> {t("settings.removing.mb")} <span class="hint">{t("settings.removing.backupCapHint")}</span></span>
    </div>
    <p class="hint">{t("settings.removing.backupsHint")}</p>
    <div class="row">
      <span>{@html th("settings.removing.backupsUse", { size: fmtSize(cfg.backups_size) })}</span>
      <button class="danger" disabled={busy || !cfg.backups_size} onclick={clearBackups}><Icon name="trash" size={14} />{t("settings.removing.clearBackups")}</button>
    </div>
  </div>
  {/if}

  <div class="card">
    <h2>{t("settings.appearance.title")}</h2>
    <div class="grid">
      <label for="lang">{t("settings.language")}</label>
      <select id="lang" bind:value={prefs.lang}>
        <option value="">{t("settings.languageSystem", { name: LANGS[systemLang()] })}</option>
        {#each Object.entries(LANGS) as [code, name]}<option value={code}>{name}</option>{/each}
      </select>
      <label for="theme">{t("settings.appearance.theme")}</label>
      <select id="theme" bind:value={prefs.theme}>
        <option value="system">{t("settings.appearance.themeSystem")}</option>
        <option value="dark">{t("settings.appearance.dark")}</option>
        <option value="light">{t("settings.appearance.light")}</option>
      </select>
      <label for="accent">{t("settings.appearance.accent")}</label>
      <input id="accent" type="color" bind:value={prefs.accent} />
      <label for="build">{t("settings.appearance.build")}</label>
      <select id="build" value={String(view?.build ?? "")} disabled={busy}
        onchange={(e) => { const v = e.currentTarget.value; onBuild(v ? (Number(v) as 41 | 42) : null); }}>
        <option value="">{t("settings.appearance.buildDetect")}</option>
        <option value="42">{t("settings.appearance.build42")}</option>
        <option value="41">{t("settings.appearance.build41")}</option>
      </select>
      <label for="wsview">{t("settings.appearance.workshopView")}</label>
      <select id="wsview" bind:value={prefs.workshopView}>
        <option value="browse">{t("settings.appearance.browse")}</option>
        <option value="home">{t("settings.appearance.home")}</option>
      </select>
      <label for="perpage">{t("settings.appearance.perPage")}</label>
      <select id="perpage" bind:value={prefs.perPage}>
        <option value={30}>30</option>
        <option value={50}>50</option>
      </select>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={prefs.compact} />{t("settings.appearance.compact")}</label>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={prefs.showIds} />{t("settings.appearance.showIds")}</label>
    </div>
    <p class="hint">{t("settings.appearance.windowHint")}</p>
  </div>

  {#if cfg}
  <div class="card">
    <h2>{t("settings.launch.title")}</h2>
    <div class="grid">
      <label for="args">{t("settings.launch.args")}</label>
      <!-- i18n-ignore: game command-line flags -->
      <input id="args" type="text" class="mono" bind:value={cfg.settings.launch_args} onchange={save} spellcheck="false" placeholder="-debug -nosteam" />
      <span></span>
      <span class="hint">{t("settings.launch.argsHint")}</span>
      <span></span>
      <label class="check"><input type="checkbox" bind:checked={cfg.settings.autosort_new} onchange={save} />{t("settings.launch.autosort")}</label>
    </div>
  </div>
  {/if}

  <div class="card">
    <h2>{t("settings.about.title")}</h2>
    <div class="row">
      <span>{t("settings.about.version", { version: appVersion })}</span>
      <button onclick={onCheckUpdate}><Icon name="update" size={14} />{t("settings.about.checkUpdates")}</button>
      <Links />
    </div>
    <div class="row">
      <button disabled={!view} onclick={() => view && api.openPath(`${view.zomboid}/Logs`)}><Icon name="folder" size={14} />{t("settings.about.openLogs")}</button>
      <button onclick={copyDiagnostics} title={t("settings.about.diagnosticsTip")}><Icon name="copy" size={14} />{t("settings.about.diagnostics")}</button>
    </div>
    <p class="keys">
      {@html th("settings.about.keys")}
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
  .keys :global(kbd) {
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
