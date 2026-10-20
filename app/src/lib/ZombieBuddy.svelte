<script lang="ts">
  import { api, modeOf, VIEWPOINT, zbReady, type JavaMod, type View, type ZbStatus } from "./api";
  import { workshopUrl } from "./format";
  import { intlLang, keyedError, t, th } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import { showInSettings } from "./Settings.svelte";

  let { status, view, updates, busy, lock, onSetup, onRemove, onSwitch, onInstall, onUpdate, onSettings, onShowMod }: {
    status: ZbStatus | null;
    view: View | null;
    /** Mods with a Workshop update waiting. */
    updates: Set<string>;
    busy: boolean;
    /** Why the profile can't change now ("" when it can). */
    lock: string;
    onSetup: (viewpoint: boolean) => Promise<void>;
    onRemove: () => Promise<void>;
    /** Show what switching to Viewpoint does (downloads it the first time). */
    onSwitch: () => void;
    /** Download Viewpoint (a Viewpoint profile whose copy is gone). */
    onInstall: () => void;
    onUpdate: () => void;
    onSettings: () => void;
    /** A Java mod in My mods. */
    onShowMod: (id: string) => void;
  } = $props();

  const profile = $derived(view?.profile ?? "");
  /** `vp`: asked for from the Viewpoint card, so the changes show there. */
  let plan = $state<{ remove: boolean; lines: string[]; vp: boolean } | null>(null);
  /** The enabled Java mods, read again when the setup changes. */
  let java = $state<JavaMod[]>([]);
  /** Why they couldn't be read ("" when they were). */
  let javaError = $state("");
  $effect(() => {
    void status;
    api
      .javaMods()
      .then((j) => ((java = j), (javaError = "")))
      .catch((e) => ((java = []), (javaError = t(...keyedError(e)))));
  });
  /** The hash just copied (its button shows it worked). */
  let copied = $state("");
  const copyHash = (sha: string) =>
    navigator.clipboard.writeText(sha).then(
      () => (copied = sha),
      (e) => (javaError = t(...keyedError(e))),
    );
  /** Loader names are proper names. */
  const LOADERS = { leaf: "Leaf", avrix: "Avrix" };
  const approval = (j: JavaMod) => t(j.approved === true ? "java.allowed" : j.approved === false ? "java.blocked" : "java.willAsk");
  let error = $state("");
  /** The error came from the Viewpoint card. */
  let errorVp = $state(false);

  async function preview(remove: boolean, vp = false) {
    error = "";
    errorVp = vp;
    try {
      plan = { remove, lines: await api.zbPreview(remove, vp), vp };
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function apply() {
    const remove = plan?.remove;
    const vp = !!plan?.vp;
    plan = null;
    await (remove ? onRemove() : onSetup(vp));
  }

  const ready = $derived(zbReady(status));
  const installedAny = $derived(!!status && (status.files_current || status.launcher_patched));
  const checks = $derived(
    status
      ? [
          [status.have_mod, t("zb.checks.mod"), t("zb.checks.modHint")],
          [status.files_current, t("zb.checks.agent"), status.game_dir ?? t("zb.checks.gameNotFound")],
          [status.launcher_patched, t("zb.checks.flag"), t("zb.checks.flagHint")],
          [status.enabled, t("zb.checks.enabled", { profile }), t("zb.checks.enabledHint")],
        ] as [boolean, string, string][]
      : [],
  );

  // --- Project Viewpoint ---
  /** done, label, hint, and whether it stops Viewpoint (shown as a warning) */
  type Check = [boolean, string, string, boolean?];
  const vpRow = $derived(view?.rows.find((r) => r.kind === "mod" && r.id === VIEWPOINT.mod));
  const vpInstalled = $derived(vpRow?.kind === "mod" && !vpRow.missing);
  const vpActive = $derived(view?.mode === "viewpoint");
  const vpProfiles = $derived((view?.profiles ?? []).filter((p) => modeOf(view?.kinds[p]) === "viewpoint"));
  /** Its mod.info: version and the game versions it's made for. */
  let vpInfo = $state<Record<string, string>>({});
  $effect(() => {
    if (vpInstalled) api.details(VIEWPOINT.mod).then((d) => (vpInfo = d?.info ?? {})).catch(() => (vpInfo = {}));
    else vpInfo = {};
  });
  const b41 = $derived(view?.build === 41);
  /** Switching enables ZombieBuddy in the Viewpoint profile, so outside it only the game folder counts. */
  const zbForVp = $derived(vpActive ? ready : !!status && status.have_mod && status.files_current && status.launcher_patched);
  const gameRange = $derived(
    vpInfo.versionMin && vpInfo.versionMax && vpInfo.versionMin !== vpInfo.versionMax
      ? `${vpInfo.versionMin}–${vpInfo.versionMax}`
      : (vpInfo.versionMin ?? vpInfo.versionMax ?? ""),
  );
  /** Not installed: the install hint covers ZombieBuddy and the profile, so those rows wait. */
  const vpChecks = $derived(
    (
      [
        [vpInstalled, vpInstalled ? t("vp.installed", { version: vpInfo.modversion ?? "?" }) : t("vp.notInstalled"),
          updates.has(VIEWPOINT.mod) ? t("vp.updateReady") : vpInstalled ? "" : t("vp.notInstalledHint")],
        (vpInstalled || vpActive) && [zbForVp, t(zbForVp ? "vp.zbReady" : "vp.zbNotReady"), zbForVp ? "" : t("vp.zbNotReadyHint")],
        [!b41, t("vp.build"), b41 ? t("vp.build41") : gameRange ? t("vp.gameVersion", { version: gameRange }) : "", b41],
        (vpInstalled || vpProfiles.length > 0) && [vpProfiles.length > 0, vpProfiles.length ? t("vp.profiles", { names: new Intl.ListFormat(intlLang(), { type: "unit" }).format(vpProfiles) }) : t("vp.noProfile"),
          vpActive ? t("vp.active", { profile }) : vpProfiles.length ? "" : t("vp.noProfileHint")],
      ] as (Check | false)[]
    ).filter((c): c is Check => !!c),
  );
</script>

{#snippet checklist(items: Check[])}
  <ul class="checks">
    {#each items as [ok, label, hint, bad]}
      <li class:ok class:bad>
        <span class="mark" aria-hidden="true">{#if ok}<Icon name="check" size={12} />{:else if bad}<Icon name="alert" size={12} />{/if}</span>
        <span><strong>{label}</strong>{#if hint}<span class="hint">{hint}</span>{/if}</span>
        <span class="visually-hidden">{ok ? t("zb.done") : t("zb.notDone")}</span>
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet changes()}
  {#if plan}
    <div class="plan" role="region" aria-label={t("zb.changesLabel")}>
      <p><strong>{plan.remove ? t("zb.confirmRemove") : t("zb.willDo")}</strong></p>
      {#if plan.remove && status?.required_by.length}
        <p class="needed">{t("zb.removeNeeded", { count: status.required_by.length })}</p>
      {/if}
      {#if plan.lines.length}
        <ul class="mono">{#each plan.lines as l}<li>{t(...keyedError(l))}</li>{/each}</ul>
      {:else}
        <p>{t("zb.nothing")}</p>
      {/if}
      <div class="row">
        <button class="primary" onclick={apply} disabled={busy || !plan.lines.length}>
          <Icon name={plan.remove ? "trash" : "check"} size={14} />{plan.remove ? t("common.remove") : t("zb.apply")}
        </button>
        <button class="quiet" onclick={() => (plan = null)}><Icon name="x" size={14} />{t("common.cancel")}</button>
      </div>
    </div>
  {/if}
{/snippet}

<section class="zb blueprint" aria-label={t("tabs.zb")}>
  <div class="pair">
  <div class="inner">
    <h2 class="zb-title"><img src="/brand/zombiebuddy.png" alt="" width="46" height="46" />{t("zb.title")}</h2>
    <p class="lede">
      {t("zb.lede")}
    </p>

    <p class="caution">
      {@html th("zb.caution")}
    </p>

    {#if status}
      {@render checklist(checks)}

      {#each status.loaders as l}
        <p class="loader">
          <strong>{t("java.loaderFound", { name: LOADERS[l.name], version: l.version })}</strong>
          {l.started_by === "steam" ? t("java.loaderSteam") : l.started_by ? t("java.loaderBy", { file: l.started_by }) : t("java.loaderNotStarted")}
        </p>
      {/each}

      {#if status.required_by.length}
        <p class="needed">
          {@html th("zb.neededBy", { count: status.required_by.length, ids: new Intl.ListFormat(intlLang(), { type: "unit" }).format(status.required_by) })}
        </p>
      {/if}

      {#if plan && !plan.vp}
        {@render changes()}
      {:else}
        <div class="row">
          {#if !ready}
            <!-- without a game folder there's nothing to set up yet: setting it is the next step -->
            <button class:primary={!!status.game_dir} onclick={() => preview(false)} disabled={busy || !status.game_dir}>
              <Icon name="settings" size={14} />{status.have_mod && status.files_current && status.launcher_patched ? t("zb.enableHere") : installedAny ? t("zb.repair") : t("zb.setUp")}
            </button>
            {#if !status.game_dir}
              <button class="primary" onclick={() => (showInSettings("folders"), onSettings())}><Icon name="folder" size={14} />{t("zb.setGameFolder")}</button>
            {/if}
          {/if}
          {#if installedAny}
            <button class="danger" onclick={() => preview(true)} disabled={busy}><Icon name="trash" size={14} />{t("zb.removeFromGame")}</button>
          {/if}
          <button class="quiet" onclick={() => api.openUrl(workshopUrl("3619862853"))}><img src="/brand/steam.png" alt="" width="14" height="14" class="brand-img" />{t("details.workshopPage")}</button>
          <button class="quiet" onclick={() => api.openUrl("https://github.com/zed-0xff/ZombieBuddy")}><Icon name="external" size={14} />{t("zb.about")}</button>
        </div>
      {/if}
      {#if error && !errorVp}<p class="error">{error}</p>{/if}

      <p class="fine">
        {@html th("zb.fine")}
        {t("zb.useInstallerBefore")} <button class="quiet link" onclick={() => api.openUrl("https://github.com/zed-0xff/ZombieBuddy/releases/tag/windows_installer")}>{t("zb.installer")}</button> {t("zb.useInstallerAfter")}
      </p>
    {/if}
  </div>

  <div class="inner" role="region" aria-labelledby="vp-title">
    <!-- i18n-ignore: a proper name -->
    <h2 id="vp-title" class="wordmark"><img src="/brand/project-viewpoint.png" alt="Project Viewpoint" height="40" class="glyph" /></h2>
    <p class="lede">{t("vp.lede")}</p>
    {@render checklist(vpChecks)}
    {#if plan?.vp}
      {@render changes()}
    {:else}
      <div class="row">
        {#if !vpInstalled}
          <button class="primary" onclick={vpActive ? onInstall : onSwitch} disabled={busy || b41 || !!lock} title={lock}>
            <Icon name="download" size={14} />{t("vp.install")}
          </button>
        {:else if !vpActive}
          <button class="primary" onclick={onSwitch} disabled={busy || b41 || !!lock} title={lock}><Icon name="swap" size={14} />{t("vp.switch")}</button>
        {/if}
        {#if vpInstalled && updates.has(VIEWPOINT.mod)}
          <button onclick={onUpdate} disabled={busy}><Icon name="refresh" size={14} />{t("vp.update")}</button>
        {/if}
        {#if !zbForVp && status && (vpInstalled || vpActive)}
          <button onclick={() => preview(false, true)} disabled={busy || !status.game_dir}><Icon name="settings" size={14} />{t("app.zbSetup")}</button>
        {/if}
        {#if b41}
          <button onclick={() => (showInSettings("appearance"), onSettings())}><Icon name="settings" size={14} />{t("vp.changeBuild")}</button>
        {/if}
        <button class="quiet" onclick={() => api.openUrl(workshopUrl(VIEWPOINT.workshop))}>
          <img src="/brand/steam.png" alt="" width="14" height="14" class="brand-img" />{t("vp.workshopPage")}
        </button>
        <button class="quiet" onclick={() => api.openUrl(VIEWPOINT.discord)} title={t("workshop.viewpointDiscordTip")}>
          <img src="/brand/discord.png" alt="" width="14" height="14" class="brand-img glyph" />{t("workshop.viewpointDiscord")}
        </button>
      </div>
    {/if}
    {#if error && errorVp}<p class="error">{error}</p>{/if}
    <p class="fine">{t("vp.settings")}</p>
  </div>
  </div>

  <div class="inner" role="region" aria-label={t("java.title")}>
    <h2>{t("java.title")}</h2>
    <p class="lede">{t("java.lede")}</p>
    {#if java.length}
      <ul class="java">
        {#each java as j}
          <li>
            <button class="quiet link" onclick={() => onShowMod(j.id)}><strong>{j.name || j.id}</strong></button> <span class="mono hint">{j.jar}</span>
            {#if j.loader === "zombiebuddy"}
              <p class:bad={j.signature === "invalid" || j.signature === "missing"}>
                {t(`java.sig.${j.signature}`, { signer: j.signer_name || j.signer || "" })}
                {#if j.sha256}{approval(j)}{/if}
              </p>
            {:else}
              <p>{t("java.loadsWith", { name: LOADERS[j.loader] })}</p>
            {/if}
            {#if j.sha256}
              <p class="hint sha">
                <span class="mono">{`SHA-256 ${j.sha256}`}</span>
                <button class="quiet link" onclick={() => copyHash(j.sha256 ?? "")} title={t("java.copyHash")} aria-label={copied === j.sha256 ? t("java.hashCopied") : t("java.copyHash")}><Icon name={copied === j.sha256 ? "check" : "copy"} size={12} /></button>
              </p>
              {#if j.risks.length}
                <ul class="risks">
                  {#each j.risks as r}<li class={r.level}>{t(`doctor.${r.key}`, r.vars)}</li>{/each}
                </ul>
              {:else}
                <p class="hint">{t("java.clean")}</p>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {:else if !javaError}
      <p class="hint">{t("java.none")}</p>
    {/if}
    {#if javaError}<p class="error">{javaError}</p>{/if}
  </div>
</section>

<style>
  .zb {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    width: min(1180px, calc(100% - 32px));
    margin: 24px auto 0;
    padding: 22px 26px 24px;
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  .inner:last-child {
    margin-bottom: 40px;
  }
  /* ZombieBuddy and Viewpoint side by side; stacked when the window is narrow */
  .pair {
    width: min(1180px, calc(100% - 32px));
    margin: 24px auto 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(420px, 1fr));
    gap: 20px;
    align-items: stretch;
  }
  /* equal heights: the buttons and notes sit at the bottom of both */
  .pair > .inner {
    width: auto;
    margin: 0;
    display: flex;
    flex-direction: column;
  }
  /* both cards end the same way: the note, then the buttons, on one line across */
  .pair > .inner > .fine {
    order: 1;
    margin-top: auto;
    padding-top: 12px;
  }
  .pair > .inner > .row {
    order: 2;
  }
  .zb-title,
  .wordmark {
    min-height: 52px;
    margin-bottom: 10px;
  }
  .zb-title {
    gap: 12px;
  }

  .wordmark img {
    display: block;
    height: 46px;
    width: auto;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .brand-img {
    border-radius: 3px;
  }
  /* white logos: darkened on the light theme */
  .glyph {
    filter: var(--glyph);
  }
  .loader {
    margin: 0 0 8px;
  }
  .caution {
    padding: 10px 14px;
    border-left: 3px solid var(--danger);
    background: color-mix(in oklab, var(--danger) 10%, transparent);
    border-radius: 0 6px 6px 0;
  }
  .checks {
    list-style: none;
    padding: 0;
    margin: 20px 0;
    display: grid;
    gap: 10px;
  }
  .checks li {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .mark {
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--raised);
    color: var(--muted);
    border: 1px solid var(--line);
  }
  .checks li.bad .mark {
    border-color: var(--danger);
    color: var(--danger);
  }
  .checks li.ok .mark {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
    font-weight: 700;
  }
  .hint {
    display: block;
    overflow-wrap: anywhere;
  }
  .needed {
    color: var(--muted);
  }
  .java {
    list-style: none;
    padding: 0;
    margin: 10px 0;
    display: grid;
    gap: 12px;
  }
  .java > li {
    padding: 10px 14px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .java p {
    margin: 4px 0 0;
  }
  .bad,
  .checks li.bad .hint,
  .risks li.warning {
    color: var(--danger);
  }
  .sha {
    overflow-wrap: anywhere;
    font-size: 12px;
  }
  .risks {
    margin: 6px 0 0;
    padding-left: 18px;
  }
  .risks li.note {
    color: var(--muted);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 14px 0;
  }
  .plan {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px 16px;
    margin: 14px 0;
  }
  .plan ul {
    margin: 6px 0;
    padding-left: 18px;
    overflow-wrap: anywhere;
  }
  .fine {
    color: var(--muted);
    font-size: 13px;
    margin-top: 24px;
  }
  .link {
    text-underline-offset: auto;
    color: inherit;
    font-size: inherit;
  }
</style>
