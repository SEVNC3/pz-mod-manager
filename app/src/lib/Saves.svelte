<script lang="ts">
  import { api, type Save, type ServersAndSaves } from "./api";
  import { fmtDateTime } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import SaveTools from "./SaveTools.svelte";
  import { showInSettings } from "./Settings.svelte";

  let { busy, lock = "", profile, enabledIds, installedIds, onUseSave, onSettings, ask }: {
    busy: boolean;
    /** Why switching profile is blocked now ("" when it isn't): Use these mods waits. */
    lock?: string;
    profile: string;
    enabledIds: string[];
    /** Installed mod ids: a save's mods not among them are missing. */
    installedIds: string[];
    /** Switch to a save's mods (after a preview). */
    onUseSave: (save: Save) => Promise<boolean>;
    onSettings: () => void;
    /** A preview to confirm: resolves to the choice picked, -1 for Cancel. */
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
  } = $props();

  let data = $state<ServersAndSaves | null>(null);
  let error = $state("");
  const refresh = () =>
    api
      .serversAndSaves()
      .then((d) => ((data = d), (error = "")))
      .catch((e) => (error = t(...keyedError(e))));
  /** Game save folders' mode names the game itself translates; others show as they are. */
  const MODES = ["Apocalypse", "Survivor", "Builder", "Sandbox", "Multiplayer", "Beginner"];
  const modeName = (m: string) => (MODES.includes(m) ? t(`saves.mode.${m}`) : m);

  function diff(mods: string[]) {
    const mine = new Set(enabledIds);
    const theirs = new Set(mods);
    const extra = mods.filter((m) => !mine.has(m)).length;
    const lacking = enabledIds.filter((m) => !theirs.has(m)).length;
    if (!extra && !lacking) return t("servers.sameMods");
    return new Intl.ListFormat(intlLang(), { type: "unit" }).format([extra ? t("servers.notInProfile", { count: extra }) : "", lacking ? t("servers.notInTheirs", { count: lacking }) : ""].filter(Boolean));
  }

  const when = (time: number) => fmtDateTime(time);
  const installed = $derived(new Set(installedIds));

  /** The save whose tools are open: "mode/name". */
  let open = $state("");
  const toggle = (key: string) => (open = open === key ? "" : key);

  // again once a launch, install or switch is over (it may have changed the saves or their mods)
  $effect(() => {
    if (!busy) refresh();
  });
</script>

<section class="ss blueprint" aria-label={t("servers.saves.title")}>
  <div class="inner">
  <div class="card">
    <div class="head">
      <h2>{t("servers.saves.title")}</h2>
      <button class="quiet" onclick={refresh} disabled={busy}><Icon name="refresh" size={14} />{t("common.refresh")}</button>
    </div>
    <p class="lede">{t("servers.saves.lede")}</p>
    {#if error}<p class="error">{error}</p>{/if}
    {#if data?.own_folder}<p class="hint">{t("servers.saves.ownFolder", { dir: data.own_folder })}</p>{/if}
    {#if data && data.saves.length === 0}
      <p class="hint">{t("servers.saves.none")}</p>
    {:else if data}
      <ul class="list">
        {#each data.saves as s (s.mode + "/" + s.name)}
          {@const key = s.mode + "/" + s.name}
          {@const missing = s.mods.filter((id) => !installed.has(id))}
          <li>
            <span class="name">
              <strong>{s.name}</strong>
              <span class="hint">{t("servers.saves.info", { mode: modeName(s.mode), count: s.mods.length, when: when(s.modified), diff: diff(s.mods) })}{#if s.maps.length}{" · "}{s.maps.join(", ")}{/if}{#if missing.length}{" · "}<span class="error" title={t("saves.notInstalledTip", { mods: missing.join(", ") })}>{t("saves.notInstalled", { count: missing.length })}</span>{/if}</span>
            </span>
            <span class="btns">
              <button class="quiet" onclick={() => data && api.openPath(`${data.saves_dir}/${s.mode}/${s.name}`)} title={t("saves.openFolder")} aria-label={t("saves.openFolder")}><Icon name="folder" size={14} /></button>
              <button disabled={busy || !!lock} title={lock} onclick={() => onUseSave(s)}><Icon name="swap" size={14} />{t("servers.saves.useMods")}</button>
              <button aria-expanded={open === key} onclick={() => toggle(key)}><Icon name="settings" size={14} />{t("saves.tools")}</button>
            </span>
          </li>
          {#if open === key}
            <li class="open"><SaveTools mode={s.mode} name={s.name} mods={s.mods} {enabledIds} {profile} {ask} onChanged={refresh} /></li>
          {/if}
        {/each}
      </ul>
    {/if}
    {#if data?.deleted.length}
      <h3>{t("saves.deletedTitle")}</h3>
      <p class="hint">{t("saves.deletedHint")}</p>
      <ul class="list">
        {#each data.deleted as [mode, name] (mode + "/" + name)}
          {@const key = mode + "/" + name}
          <li>
            <span class="name"><strong>{name}</strong> <span class="hint">{modeName(mode)}</span></span>
            <button aria-expanded={open === key} onclick={() => toggle(key)}><Icon name="rollback" size={14} />{t("saves.backupsButton")}</button>
          </li>
          {#if open === key}
            <li class="open"><SaveTools {mode} {name} mods={null} {enabledIds} {profile} {ask} onChanged={refresh} /></li>
          {/if}
        {/each}
      </ul>
    {/if}
    <p class="hint auto">{t("saves.autoHint")} <button class="link" onclick={() => (showInSettings("backups"), onSettings())}>{t("saves.autoSettings")}</button></p>
  </div>
  </div>
</section>

<style>
  .ss {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    max-width: 1180px;
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
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
  }
  h3 {
    margin: 24px 0 4px;
    font-size: 15px;
  }
  .lede {
    margin: 0 0 12px;
  }
  .list {
    list-style: none;
    padding: 0;
    margin: 8px 0 0;
    border-top: 1px solid var(--line);
  }
  .list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 8px;
    margin: 0 -8px;
    border-radius: 4px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .list li:not(.open):hover {
    background: var(--hover);
    color: var(--on-selected);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .list li:not(.open):hover .hint {
    color: color-mix(in oklab, var(--on-selected) 72%, transparent);
  }
  .list li.open {
    display: block;
  }
  .auto {
    margin-top: 10px;
  }
  .name {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  .hint {
    display: block;
  }
</style>
