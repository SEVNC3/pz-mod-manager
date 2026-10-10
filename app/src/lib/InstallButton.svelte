<script lang="ts">
  import type { WorkshopItem } from "./api";
  import Icon from "./Icon.svelte";

  let { item, busy, installing, onInstall, all = "Install all" }: {
    item: WorkshopItem;
    busy: boolean;
    /** The item being installed, if any: one install at a time. */
    installing: string | null;
    onInstall: (id: string) => void;
    /** The label for a collection. */
    all?: string;
  } = $props();
</script>

{#if item.have}
  <span class="have"><Icon name="check" size={14} /> Installed</span>
{:else}
  <button class="primary" disabled={busy || installing !== null} onclick={() => onInstall(item.id)}>
    <Icon name="download" size={14} />{installing === item.id ? "Installing…" : item.collection ? all : "Install"}
  </button>
{/if}

<style>
  .have {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--accent-text);
    font-weight: 700;
  }
</style>
