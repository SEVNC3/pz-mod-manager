<script lang="ts">
  import type { WorkshopItem } from "./api";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";

  let { item, busy, installing, onInstall, onUninstall, all }: {
    item: WorkshopItem;
    busy: boolean;
    /** The item being installed, if any: one install at a time. */
    installing: string | null;
    onInstall: (id: string) => void;
    /** Installed: take it out again (PZMM's copy removed, or Steam's subscription ended). */
    onUninstall?: (id: string) => void;
    /** The label for a collection (default "Install all"). */
    all?: string;
  } = $props();
</script>

{#if item.have}
  <span class="installed">
    <span class="have"><Icon name="check" size={14} /> {t("common.installed")}</span>
    {#if onUninstall && !item.collection}
      <button class="quiet" disabled={busy || installing !== null} onclick={() => onUninstall(item.id)} title={t("installButton.uninstallTip")}><Icon name="trash" size={14} />{t("installButton.uninstall")}</button>
    {/if}
  </span>
{:else}
  <button class="primary" disabled={busy || installing !== null} onclick={() => onInstall(item.id)}>
    <Icon name="download" size={14} />{installing === item.id ? t("common.installing") : item.collection ? (all ?? t("installButton.installAll")) : t("common.install")}
  </button>
{/if}

<style>
  .installed {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .have {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--accent-text);
    font-weight: 700;
  }
</style>
