<script lang="ts">
  import { api, zbReady, type ZbStatus } from "./api";
  import { t, th } from "./i18n.svelte";
  import Icon from "./Icon.svelte";

  let { status, profile, busy, onSetup, onRemove }: {
    status: ZbStatus | null;
    profile: string;
    busy: boolean;
    onSetup: () => Promise<void>;
    onRemove: () => Promise<void>;
  } = $props();

  let plan = $state<{ remove: boolean; lines: string[] } | null>(null);
  let error = $state("");

  async function preview(remove: boolean) {
    error = "";
    try {
      plan = { remove, lines: await api.zbPreview(remove) };
    } catch (e) {
      error = String(e);
    }
  }

  async function apply() {
    const remove = plan?.remove;
    plan = null;
    await (remove ? onRemove() : onSetup());
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
</script>

<section class="zb blueprint" aria-label={t("zb.title")}>
  <div class="inner">
    <h2>{t("zb.title")}</h2>
    <p class="lede">
      {t("zb.lede")}
    </p>

    <p class="caution">
      {@html th("zb.caution")}
    </p>

    {#if status}
      <ul class="checks">
        {#each checks as [ok, label, hint]}
          <li class:ok>
            <span class="mark" aria-hidden="true">{#if ok}<Icon name="check" size={12} />{/if}</span>
            <span><strong>{label}</strong><span class="hint">{hint}</span></span>
            <span class="visually-hidden">{ok ? t("zb.done") : t("zb.notDone")}</span>
          </li>
        {/each}
      </ul>

      {#if status.required_by.length}
        <p class="needed">
          {@html th("zb.neededBy", { count: status.required_by.length, ids: status.required_by.join(", ") })}
        </p>
      {/if}

      {#if plan}
        <div class="plan" role="region" aria-label={t("zb.changesLabel")}>
          <p><strong>{plan.remove ? t("zb.confirmRemove") : t("zb.willDo")}</strong></p>
          {#if plan.lines.length}
            <ul class="mono">{#each plan.lines as l}<li>{l}</li>{/each}</ul>
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
      {:else}
        <div class="row">
          {#if !ready}
            <button class="primary" onclick={() => preview(false)} disabled={busy || !status.game_dir}>
              <Icon name="settings" size={14} />{installedAny ? t("zb.repair") : t("zb.setUp")}
            </button>
          {/if}
          {#if installedAny}
            <button class="danger" onclick={() => preview(true)} disabled={busy}><Icon name="trash" size={14} />{t("zb.removeFromGame")}</button>
          {/if}
          <button class="quiet" onclick={() => api.openUrl("https://github.com/zed-0xff/ZombieBuddy")}><Icon name="external" size={14} />{t("zb.about")}</button>
        </div>
      {/if}
      {#if error}<p class="error">{error}</p>{/if}

      <p class="fine">
        {@html th("zb.fine")}
        {t("zb.useInstallerBefore")} <button class="quiet link" onclick={() => api.openUrl("https://github.com/zed-0xff/ZombieBuddy/releases/tag/windows_installer")}>{t("zb.installer")}</button> {t("zb.useInstallerAfter")}
      </p>
    {/if}
  </div>
</section>

<style>
  .zb {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    width: min(760px, calc(100% - 32px));
    margin: 24px auto 40px;
    padding: 22px 26px 24px;
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
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
