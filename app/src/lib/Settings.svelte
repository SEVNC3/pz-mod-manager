<script module lang="ts">
  const SECTIONS = ["folders", "downloads", "keys", "backups", "appearance", "launch", "performance", "about"] as const;
  type Section = (typeof SECTIONS)[number];
  // kept at module level: what was open stays open when the tab is left and come back to
  let section = $state<Section>("folders");
  /** Open Settings on a section (other tabs' links: the Steam key, save backups, the game build). */
  export function showInSettings(s: Section) {
    section = s;
  }
</script>

<script lang="ts">
  import { track } from "./activity.svelte";
  import { onMount, tick } from "svelte";
  import { api, type Folder, type NotifyKind, type ProfileLaunch, type SettingsView, type View } from "./api";
  import { fmtSize } from "./format";
  import { LANGS, systemLang, keyedError, t, th, type Vars } from "./i18n.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import { confirmed } from "./ask.svelte";
  import Links from "./Links.svelte";
  import Performance from "./Performance.svelte";
  import { prefs } from "./prefs.svelte";
  import { arrowPick, wholeNumbers } from "./ui";

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

  const ICONS: Record<Section, IconName> = { folders: "folder", downloads: "download", keys: "lock", backups: "rollback", appearance: "eye", launch: "play", performance: "pulse", about: "info" };
  const NAMES: Record<Section, string> = { folders: "settings.folders.title", downloads: "settings.downloads.title", keys: "settings.keys.title", backups: "settings.removing.title",
    appearance: "settings.appearance.title", launch: "settings.launch.section", performance: "perf.title", about: "settings.about.title" };
  const LEDES: Record<Section, string> = { folders: "settings.folders.lede", downloads: "settings.desc.downloads", keys: "settings.desc.keys", backups: "settings.desc.backups",
    appearance: "settings.desc.appearance", launch: "settings.desc.launch", performance: "perf.lede", about: "settings.desc.about" };
  /** The sections that show settings.json, so wait for it. */
  const NEEDS_CFG: Section[] = ["folders", "downloads", "keys", "backups", "launch"];
  /** Narrow windows: the rail is a drawer the bar above the main area opens. */
  let railOpen = $state(false);
  let rail = $state<HTMLElement>();
  let toggle = $state<HTMLButtonElement>();
  function go(s: Section) {
    section = s;
    if (railOpen) closeRail();
  }
  /** The drawer opens on the open section's button; closing it puts focus back on the bar's button. */
  async function openRail() {
    railOpen = true;
    await tick();
    rail?.querySelector<HTMLElement>("button.active")?.focus();
  }
  function closeRail(refocus = true) {
    railOpen = false;
    if (refocus) toggle?.focus();
  }

  /** The backend names folders and their problems in English: shown in the UI language by key (English text it doesn't know as is). */
  const WARNINGS: [RegExp, string][] = [[/^a drive root/, "driveRoot"], [/in the game folder/, "inGame"], [/OneDrive/, "oneDrive"], [/no permission/, "noWrite"], [/^only ([\d.]+) GB free/, "lowSpace"]];
  function warning(w: string) {
    for (const [re, key] of WARNINGS) {
      const m = w.match(re);
      if (m) return t(`settings.folders.warn.${key}`, { size: m[1] ? fmtSize(Number(m[1]) * 2 ** 30) : "" });
    }
    return w;
  }
  const folderName = (f: Folder) => t(`settings.folders.name.${f.key}`);

  const hint = (f: Folder) =>
    f.key === "data" ? t("settings.folders.hintData") : f.key === "zomboid" ? t("settings.folders.hintZomboid") : f.key === "saves" ? t("settings.folders.hintSaves") : "";

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
      say(...keyedError(e), true);
    } finally {
      working = false;
    }
  }

  async function browse(f: Folder) {
    const picked = await api.pickFolder(f.path ?? "").catch(() => null);
    if (picked) setFolder(f, picked);
  }

  /** The active profile's launch options, read again when the profile changes. */
  let launch = $state<ProfileLaunch | null>(null);
  $effect(() => {
    if (view?.profile !== undefined) api.profileLaunch().then((l) => (launch = l)).catch(() => (launch = null));
  });
  async function setLaunch(args: string, own: boolean) {
    try {
      launch = await api.profileLaunchSet(args, own);
    } catch (e) {
      say(...keyedError(e), true);
    }
  }

  // read by the app at start; read again here when that failed (or Try again)
  let readError = $state("");
  const read = () => api.settingsGet().then((c) => ((cfg = c), (readError = ""))).catch((e) => (readError = t(...keyedError(e))));
  onMount(() => {
    if (!cfg) read();
    // the server browser has its own countries switch: shown again, Settings shows the one on disk
    else api.settingsGet().then((c) => cfg && (cfg.settings.countries = c.settings.countries)).catch(() => {});
  });

  const NOTIFY: NotifyKind[] = ["crash", "restart", "downloads", "favourites"];
  const save = () => cfg && api.settingsSet($state.snapshot(cfg.settings)).catch((e) => say("settings.saveFailed", { error: t(...keyedError(e)) }, true));

  let token = $state("");
  async function saveToken(value: string) {
    try {
      await api.githubTokenSet(value);
      token = "";
      if (cfg) cfg.token_saved = !!value.trim();
      say(value.trim() ? "settings.tokenSaved" : "settings.tokenForgot");
    } catch (e) {
      say(...keyedError(e), true);
    }
  }

  let steamKey = $state("");
  async function saveSteamKey(value: string) {
    try {
      await api.steamKeySet(value);
      steamKey = "";
      if (cfg) cfg.steam_key_saved = !!value.trim();
      say(value.trim() ? "settings.steamKey.saved" : "settings.steamKey.forgot");
    } catch (e) {
      say(...keyedError(e), true);
    }
  }

  async function clearBackups() {
    if (!cfg || !(await confirmed(t("settings.confirmClearBackups", { size: fmtSize(cfg.backups_size) }), { label: t("common.delete"), icon: "trash" }))) return;
    try {
      cfg.backups_size = await api.backupsClear();
    } catch (e) {
      say(...keyedError(e), true);
    }
  }

  /** Server countries go through their own command: settings_set keeps the one on disk. */
  async function setCountries(on: boolean) {
    try {
      await api.serverCountriesSet(on);
      if (cfg) cfg.settings.countries = on;
    } catch (e) {
      say(...keyedError(e), true);
    }
  }

  async function copyDiagnostics() {
    try {
      await navigator.clipboard.writeText(await api.diagnostics());
      say("settings.diagnosticsCopied");
    } catch (e) {
      say("settings.diagnosticsFailed", { error: t(...keyedError(e)) }, true);
    }
  }

  // the activity bar moves while this tab works
  $effect(() => {
    if (working) return track();
  });
</script>

<svelte:window onpointerdown={(e) => railOpen && !rail?.contains(e.target as Node) && !toggle?.contains(e.target as Node) && closeRail(false)} />

<section class="settings" aria-label={t("settings.label")}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions: the arrow keys move between its buttons, Esc closes the drawer -->
  <nav class="rail" class:open={railOpen} aria-label={t("settings.sections")} bind:this={rail}
    onkeydown={(e) => (railOpen && e.key === "Escape" ? closeRail() : arrowPick(e, [...SECTIONS], section, (v) => (section = v as Section)))}>
    {#each SECTIONS as s}
      <button class:active={section === s} aria-current={section === s ? "page" : undefined} onclick={() => go(s)}><Icon name={ICONS[s]} size={14} />{t(NAMES[s])}</button>
    {/each}
  </nav>

  <div class="main">
    <div class="mbar">
      <button bind:this={toggle} onclick={() => (railOpen ? closeRail(false) : openRail())} aria-expanded={railOpen}><Icon name={ICONS[section]} size={14} />{t(NAMES[section])}<Icon name={railOpen ? "chevron-up" : "chevron-down"} size={14} /></button>
    </div>

    {#key section}
    <div class="page blueprint">
    <div class="inner" use:wholeNumbers>
    <header>
      <h2>{t(NAMES[section])}</h2>
      <p>{t(LEDES[section])}</p>
    </header>

    {#if NEEDS_CFG.includes(section) && !cfg}
      <div class="card">
        {#if readError}
          <p class="error">{t("settings.readFailed", { error: readError })}</p>
          <button onclick={read}><Icon name="refresh" size={14} />{t("common.tryAgain")}</button>
        {:else}
          <p class="hint">{t("settings.folders.reading")}</p>
        {/if}
      </div>
    {:else if section === "folders" && cfg}
      {#each cfg.folders as f (f.key)}
        <div class="card">
          <div class="fhead">
            <h3>{folderName(f)}</h3>
            <span class="pill {f.status}">{t(`settings.folders.status.${f.status}`)}</span>
            {#if f.overridden}<span class="hint">{@html th("settings.folders.overriddenBy", { env: f.env ?? "" })}</span>{:else if !f.custom && f.path}<span class="hint">{t("settings.folders.detected")}</span>{/if}
          </div>
          {#if hint(f)}<p class="desc">{hint(f)}</p>{/if}
          <div class="row">
            <input type="text" class="mono path" value={f.path ?? ""} placeholder={t("settings.folders.status.missing")} spellcheck="false" aria-label={folderName(f)}
              disabled={working} onchange={(e) => setFolder(f, e.currentTarget.value)} />
            <button disabled={working} onclick={() => browse(f)}><Icon name="folder" size={14} />{t("settings.folders.browse")}</button>
            <button disabled={working} onclick={() => setFolder(f, "")} title={t("settings.folders.detectTip")}><Icon name="search" size={14} />{t("settings.folders.detect")}</button>
            <button disabled={f.status === "missing" || !f.path} onclick={() => f.path && api.openPath(f.path)}><Icon name="external" size={14} />{t("settings.folders.open")}</button>
          </div>
          {#each f.warnings as w}<p class="warn"><Icon name="alert" size={12} />{warning(w)}</p>{/each}
          {#if f.key === "data" && moving}
            <div class="confirm" role="group" aria-label={t("settings.folders.moveLabel")}>
              <p>{@html th("settings.folders.moveConfirm", { from: f.path ?? "", to: moving.to })}</p>
              <button class="primary" disabled={busy} onclick={() => moving && setFolder(f, moving.path, true)}><Icon name="swap" size={14} />{t("settings.folders.moveData")}</button>
              <button disabled={busy} onclick={() => moving && setFolder(f, moving.path, false)}>{t("settings.folders.switchWithoutMoving")}</button>
              <button class="quiet" onclick={() => (moving = null)}><Icon name="x" size={14} />{t("common.cancel")}</button>
            </div>
          {/if}
        </div>
      {/each}
    {:else if section === "downloads" && cfg}
      <div class="card">
        <h3>{t("settings.downloads.updates")}</h3>
        <p class="desc">{t("settings.downloads.updatesHint")}</p>
        <div class="set">
          <label for="upd">{t("settings.downloads.check")}</label>
          <select id="upd" bind:value={cfg.settings.update_check} onchange={save}>
            <option value="off">{t("settings.downloads.checkOff")}</option>
            <option value="start">{t("settings.downloads.checkStart")}</option>
            <option value="1h">{t("settings.downloads.check1h")}</option>
            <option value="3h">{t("settings.downloads.check3h")}</option>
          </select>
        </div>
        <div class="set">
          <label for="act">{t("settings.downloads.whenFound")}</label>
          <select id="act" bind:value={cfg.settings.update_action} onchange={save} disabled={cfg.settings.update_check === "off"}>
            <option value="notify">{t("settings.downloads.notify")}</option>
            <option value="install">{t("settings.downloads.install")}</option>
          </select>
        </div>
        <label class="set"><span>{t("settings.downloads.appUpdates")}</span><input type="checkbox" bind:checked={cfg.settings.app_updates} onchange={save} /></label>
      </div>
      <div class="card">
        <h3>{t("settings.downloads.workshop")}</h3>
        <p class="desc">{t("settings.downloads.workshopHint")}</p>
        <div class="set">
          <label for="links">{t("settings.downloads.openIn")}</label>
          <select id="links" bind:value={cfg.settings.workshop_links} onchange={save}>
            <option value="steam">{t("settings.downloads.inSteam")}</option>
            <option value="browser">{t("settings.downloads.inBrowser")}</option>
          </select>
        </div>
        <div class="set">
          <label for="wsview">{t("settings.appearance.workshopView")}</label>
          <select id="wsview" bind:value={prefs.workshopView}>
            <option value="browse">{t("settings.appearance.browse")}</option>
            <option value="home">{t("settings.appearance.home")}</option>
          </select>
        </div>
        <div class="set">
          <label for="perpage">{t("settings.appearance.perPage")}</label>
          <select id="perpage" bind:value={prefs.perPage}>
            <option value={30}>30</option>
            <option value={50}>50</option>
          </select>
        </div>
      </div>
      <div class="card">
        <div class="set head">
          <div><h3>{t("settings.repair.title")}</h3><p class="desc">{t("settings.repair.redownloadTip")}</p></div>
          <button onclick={onRedownload} disabled={busy}><Icon name="download" size={14} />{t("settings.repair.redownload")}</button>
        </div>
      </div>
    {:else if section === "keys" && cfg}
      <div class="card">
        <h3>{t("settings.token.title")}</h3>
        <p class="desc">{t("settings.token.hint")}
          {#if cfg.token_env}<strong>{t("settings.token.envSet")}</strong>{/if}</p>
        <form class="row" onsubmit={(e) => { e.preventDefault(); saveToken(token); }}>
          <input type="password" bind:value={token} autocomplete="off" spellcheck="false" aria-label={t("settings.token.title")} placeholder={cfg.token_saved ? t("settings.token.savedPlaceholder") : "ghp_…"} />
          <button type="submit" disabled={!token.trim()}><Icon name="check" size={14} />{t("settings.token.save")}</button>
          {#if cfg.token_saved}<button type="button" onclick={() => saveToken("")}><Icon name="x" size={14} />{t("settings.token.forget")}</button>{/if}
        </form>
      </div>
      <div class="card">
        <h3>{t("settings.steamKey.title")}</h3>
        <p class="desc">{t("settings.steamKey.hint")}
          <button class="link" type="button" onclick={() => api.openUrl("https://steamcommunity.com/dev/apikey")}>{t("settings.steamKey.get")}</button></p>
        <form class="row" onsubmit={(e) => { e.preventDefault(); saveSteamKey(steamKey); }}>
          <input type="password" bind:value={steamKey} autocomplete="off" spellcheck="false" aria-label={t("settings.steamKey.title")} placeholder={cfg.steam_key_saved ? t("settings.steamKey.savedPlaceholder") : ""} />
          <button type="submit" disabled={!steamKey.trim()}><Icon name="check" size={14} />{t("settings.steamKey.save")}</button>
          {#if cfg.steam_key_saved}<button type="button" onclick={() => saveSteamKey("")}><Icon name="x" size={14} />{t("settings.token.forget")}</button>{/if}
        </form>
      </div>
      <div class="card">
        <h3>{t("settings.countries.title")}</h3>
        <p class="desc">{t("launcher.countriesHint")}</p>
        <label class="set"><span>{t("settings.countries.show")}</span><input type="checkbox" checked={cfg.settings.countries} onchange={(e) => setCountries(e.currentTarget.checked)} /></label>
      </div>
    {:else if section === "backups" && cfg}
      <div class="card">
        <h3>{t("settings.removing.mods")}</h3>
        <p class="desc">{t("settings.removing.modsHint")}</p>
        <div class="set">
          <label for="rm">{t("settings.removing.removed")}</label>
          <select id="rm" bind:value={cfg.settings.recycle} onchange={save}>
            <option value={true}>{t("settings.removing.recycle")}</option>
            <option value={false}>{t("settings.removing.deleted")}</option>
          </select>
        </div>
        <div class="set">
          <label for="req">{t("settings.removing.removeRequirements")}</label>
          <select id="req" bind:value={cfg.settings.remove_requirements} onchange={save}>
            <option value="ask">{t("settings.removing.ask")}</option>
            <option value="always">{t("settings.removing.always")}</option>
            <option value="never">{t("settings.removing.never")}</option>
          </select>
        </div>
      </div>
      <div class="card">
        <h3>{t("settings.removing.backups")}</h3>
        <p class="desc">{t("settings.removing.backupsHint")}</p>
        <div class="set">
          <div><label for="cap">{t("settings.removing.backupCap")}</label><span class="hint">{t("settings.removing.backupCapHint")}</span></div>
          <span class="unit"><input id="cap" type="number" min="0" step="100" bind:value={cfg.settings.backup_cap_mb} onchange={save} />{t("settings.removing.mb")}</span>
        </div>
        <div class="set">
          <span>{@html th("settings.removing.backupsUse", { size: fmtSize(cfg.backups_size) })}</span>
          <button class="danger" disabled={busy || !cfg.backups_size} onclick={clearBackups}><Icon name="trash" size={14} />{t("settings.removing.clearBackups")}</button>
        </div>
      </div>
      <div class="card">
        <h3>{t("settings.saveBackups.title")}</h3>
        <p class="desc">{t("settings.saveBackups.hint")}</p>
        <label class="set"><span>{t("settings.saveBackups.onLaunch")}</span><input type="checkbox" bind:checked={cfg.settings.save_backup_launch} onchange={save} /></label>
        <label class="set"><span>{t("settings.saveBackups.onExit")}</span><input type="checkbox" bind:checked={cfg.settings.save_backup_exit} onchange={save} /></label>
        <div class="set">
          <div><label for="keep">{t("settings.saveBackups.keep")}</label><span class="hint">{t("settings.saveBackups.keepHint")}</span></div>
          <input id="keep" type="number" min="0" max="100" bind:value={cfg.settings.save_backup_keep} onchange={save} />
        </div>
        <div class="set">
          <div><label for="every">{t("settings.saveBackups.every")}</label><span class="hint">{t("settings.saveBackups.everyHint")}</span></div>
          <input id="every" type="number" min="0" max="240" bind:value={cfg.settings.save_backup_every} onchange={save} />
        </div>
      </div>
    {:else if section === "appearance"}
      <div class="card">
        <h3>{t("settings.appearance.look")}</h3>
        <p class="desc">{t("settings.appearance.lookHint")}</p>
        <div class="set">
          <label for="lang">{t("settings.language")}</label>
          <select id="lang" bind:value={prefs.lang}>
            <option value="">{t("settings.languageSystem", { name: LANGS[systemLang()] })}</option>
            {#each Object.entries(LANGS) as [code, name]}<option value={code}>{name}</option>{/each}
          </select>
        </div>
        <div class="set">
          <label for="theme">{t("settings.appearance.theme")}</label>
          <select id="theme" bind:value={prefs.theme}>
            <option value="system">{t("settings.appearance.themeSystem")}</option>
            <option value="dark">{t("settings.appearance.dark")}</option>
            <option value="light">{t("settings.appearance.light")}</option>
          </select>
        </div>
        <div class="set">
          <label for="accent">{t("settings.appearance.accent")}</label>
          <input id="accent" type="color" bind:value={prefs.accent} />
        </div>
        <label class="set"><span>{t("settings.appearance.compact")}</span><input type="checkbox" bind:checked={prefs.compact} /></label>
      </div>
      <div class="card">
        <h3>{t("settings.appearance.lists")}</h3>
        <p class="desc">{t("settings.appearance.listsHint")}</p>
        <div class="set">
          <label for="build">{t("settings.appearance.build")}</label>
          <select id="build" value={String(view?.forced_build ?? "")} disabled={busy}
            onchange={(e) => { const v = e.currentTarget.value; onBuild(v ? (Number(v) as 41 | 42) : null); }}>
            <option value="">{t("settings.appearance.buildDetect")}</option>
            <option value="42">{t("settings.appearance.build42")}</option>
            <option value="41">{t("settings.appearance.build41")}</option>
          </select>
        </div>
        {#if view?.game_version || view?.steam_branch}
          <p class="desc">{t("settings.appearance.gameVersion", { version: view?.game_version ?? "?", branch: view?.steam_branch ?? "?" })}</p>
        {/if}
        <label class="set"><span>{t("settings.appearance.showIds")}</span><input type="checkbox" bind:checked={prefs.showIds} /></label>
        <label class="set"><span>{t("settings.appearance.favFirst")}</span><input type="checkbox" bind:checked={prefs.favFirst} /></label>
      </div>
    {:else if section === "launch" && cfg}
      <div class="card">
        <h3>{t("settings.launch.title")}</h3>
        <p class="desc">{t("settings.launch.gameHint")}</p>
        <div class="set">
          <div><label for="args">{t("settings.launch.args")}</label><span class="hint">{t("settings.launch.argsHint")}</span></div>
          <!-- i18n-ignore: game command-line flags -->
          <input id="args" type="text" class="mono" bind:value={cfg.settings.launch_args} onchange={save} spellcheck="false" placeholder="-debug -nosteam" />
        </div>
      </div>
      {#if launch}
        <div class="card">
          <h3>{t("settings.launch.profileTitle", { profile: launch.profile })}</h3>
          <p class="desc">{t("settings.launch.profileHint")}</p>
          <div class="set">
            <label for="pargs">{t("settings.launch.profileArgs")}</label>
            <!-- i18n-ignore: game command-line flags -->
            <input id="pargs" type="text" class="mono" value={launch.launch.args} onchange={(e) => launch && setLaunch(e.currentTarget.value, !!launch.launch.cache_dir)} spellcheck="false" placeholder="-debug" />
          </div>
          <div class="set">
            <label class="what" for="own">{t("settings.launch.ownFolder")}<span class="hint">{t("settings.launch.ownFolderHint")}
              {#if launch.launch.cache_dir}<span class="mono">{launch.launch.cache_dir}</span> <button class="link" onclick={() => launch?.launch.cache_dir && api.openPath(launch.launch.cache_dir)}>{t("settings.folders.open")}</button>{/if}</span></label>
            <input id="own" type="checkbox" checked={!!launch.launch.cache_dir} onchange={(e) => launch && setLaunch(launch.launch.args, e.currentTarget.checked)} />
          </div>
        </div>
      {/if}
      <div class="card">
        <h3>{t("settings.launch.profiles")}</h3>
        <p class="desc">{t("settings.launch.profilesHint")}</p>
        <label class="set"><span>{t("settings.launch.autosort")}</span><input type="checkbox" bind:checked={cfg.settings.autosort_new} onchange={save} /></label>
        <label class="set"><span>{t("settings.launch.keepMarks")}</span><input type="checkbox" bind:checked={cfg.settings.keep_marks} onchange={save} /></label>
      </div>
      <div class="card">
        <h3>{t("settings.launch.trayTitle")}</h3>
        <p class="desc">{t("settings.launch.trayHint")}</p>
        <!-- ask, or what Settings say: tray (it keeps the tray icon) or quit -->
        <div class="set">
          <label for="onclose">{t("settings.launch.onClose")}</label>
          <select id="onclose" value={(cfg.settings.close_ask ?? !cfg.settings.tray) ? "ask" : cfg.settings.tray ? "tray" : "quit"}
            onchange={(e) => { if (!cfg) return; const v = e.currentTarget.value; cfg.settings.close_ask = v === "ask"; if (v !== "ask") cfg.settings.tray = v === "tray"; save(); }}>
            <option value="ask">{t("settings.launch.onCloseAsk")}</option>
            <option value="tray">{t("close.tray")}</option>
            <option value="quit">{t("close.quit")}</option>
          </select>
        </div>
        <label class="set"><span>{t("settings.launch.autostart")}</span><input type="checkbox" bind:checked={cfg.settings.autostart} onchange={save} /></label>
      </div>
      <div class="card">
        <h3>{t("settings.notify.title")}</h3>
        <p class="desc">{t("settings.notify.when")}</p>
        {#each NOTIFY as kind (kind)}
          <label class="set"><span>{t(`settings.notify.${kind}`)}</span><input type="checkbox" checked={cfg.settings.notify.includes(kind)}
            onchange={(e) => cfg && ((cfg.settings.notify = e.currentTarget.checked ? [...cfg.settings.notify, kind] : cfg.settings.notify.filter((k) => k !== kind)), save())} /></label>
        {/each}
      </div>
    {:else if section === "performance"}
      <Performance />
    {:else if section === "about"}
      <div class="card">
        <div class="set head">
          <div><h3>{t("settings.about.version", { version: appVersion })}</h3>
            <p class="desc">{t("settings.about.official")} <button class="link" onclick={() => api.openUrl("https://github.com/SEVNC3/pz-mod-manager/releases")}>{"github.com/SEVNC3/pz-mod-manager"}</button></p></div>
          <div class="acts"><button onclick={onCheckUpdate}><Icon name="update" size={14} />{t("settings.about.checkUpdates")}</button><Links /></div>
        </div>
      </div>
      <div class="card">
        <h3>{t("settings.about.help")}</h3>
        <p class="desc">{t("settings.about.helpHint")}</p>
        <div class="row">
          <button disabled={!view} onclick={() => view && api.openPath(`${view.zomboid}/Logs`)}><Icon name="folder" size={14} />{t("settings.about.openLogs")}</button>
          <button onclick={copyDiagnostics} title={t("settings.about.diagnosticsTip")}><Icon name="copy" size={14} />{t("settings.about.diagnostics")}</button>
        </div>
        <p class="keys">{@html th("settings.about.keys")}</p>
      </div>
    {/if}
    </div>
    </div>
    {/key}
  </div>
</section>

<style>
  /* a Workspace like Servers: the sections down a rail on the left, the open one's cards on the
     blueprint; in narrow windows the rail is a drawer the bar above the main area opens */
  .settings {
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
    padding: 12px 0;
    background: var(--panel);
    border-right: 1px solid var(--line);
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
  /* hover: the fill and its edge; the open section adds the bar */
  .rail button:hover:not(:disabled) {
    color: var(--on-selected);
    background: var(--hover);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .rail button.active {
    color: var(--on-selected);
    background: var(--accent-soft);
    border-left-color: var(--on-selected);
    box-shadow: inset 0 0 0 1px var(--hover-line);
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
  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .inner {
    max-width: 860px;
    margin: 0 auto;
    padding: 24px 16px 40px;
    display: grid;
    gap: 24px;
  }
  header h2 {
    margin: 0 0 4px;
    font-size: 22px;
  }
  header p {
    margin: 0;
    color: var(--muted);
  }
  /* Performance's cards too */
  .inner :global(.card) {
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
    padding: 22px 26px 24px;
    min-width: 0;
  }
  .inner :global(.card h3) {
    margin: 0;
    font-size: 16px;
  }
  .inner :global(.card .desc) {
    margin: 4px 0 6px;
    color: var(--muted);
  }
  /* a setting: what it is on the left, its control on the right; a line between them */
  .set {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 8px 24px;
    padding: 12px 0;
  }
  .set + .set {
    border-top: 1px solid var(--line);
  }
  .set:last-child {
    padding-bottom: 0;
  }
  .set > :first-child {
    flex: 1 1 280px;
    min-width: 0;
  }
  .set.head {
    padding-top: 0;
  }
  .set .desc {
    margin-bottom: 0;
  }
  .set .hint {
    display: block;
    margin-top: 2px;
  }
  .set select,
  .set input[type="text"] {
    width: 260px;
  }
  .set input[type="checkbox"] {
    flex: none;
    margin: 0;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  label.set {
    cursor: pointer;
  }
  .unit {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 12px;
  }
  input[type="number"] {
    width: 100px;
  }
  input[type="password"] {
    flex: 0 1 320px;
  }
  /* the native picker as the app's other fields: a raised box, the colour as a rounded swatch in it */
  input[type="color"] {
    width: 52px;
    height: 34px;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--raised);
    cursor: pointer;
  }
  input[type="color"]:hover {
    border-color: var(--hover-line);
  }
  input[type="color"]::-webkit-color-swatch-wrapper {
    padding: 0;
  }
  input[type="color"]::-webkit-color-swatch {
    border: 0;
    border-radius: 3px;
  }
  .fhead {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 10px;
  }
  .path {
    flex: 1 1 300px;
  }
  .pill {
    --tone: var(--ok);
    padding: 1px 8px;
    border: 1px solid color-mix(in oklab, var(--tone) 45%, transparent);
    border-radius: 999px;
    color: var(--tone);
    font-size: 12px;
    font-weight: 700;
  }
  .pill.missing,
  .pill.not_pz {
    --tone: var(--danger);
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--danger);
  }
  .confirm {
    margin-top: 12px;
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
    margin: 16px 0 0;
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
</style>
