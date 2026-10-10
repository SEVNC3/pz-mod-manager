<script lang="ts" module>
  import type { Overlap } from "./api";
  import { t } from "./i18n.svelte";

  /** What an overlap means, for the icon's label. */
  export const overlapLabel = (o: Overlap) =>
    [
      o.wins_over.length && t("layerIcon.overrides", { mods: o.wins_over.join(", ") }),
      o.loses_to.length && t("layerIcon.overriddenBy", { mods: o.loses_to.join(", ") }),
    ]
      .filter(Boolean)
      .join(". ");
</script>

<script lang="ts">
  /** Two stacked layers, like files stacked by load order: the top layer is filled when
      this mod's copies win, the bottom one when another mod's copies win. */
  let { wins = false, loses = false, label }: { wins?: boolean; loses?: boolean; label: string } = $props();
</script>

<span class="layers" class:wins class:loses title={label}>
  <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
    <path class="bottom" d="M8 7.5 14.5 11 8 14.5 1.5 11Z" />
    <path class="top" d="M8 1.5 14.5 5 8 8.5 1.5 5Z" />
  </svg>
  <span class="visually-hidden">{label}</span>
</span>

<style>
  .layers {
    display: inline-flex;
    vertical-align: middle;
    color: var(--muted);
  }
  path {
    fill: var(--panel);
    stroke: currentColor;
    stroke-width: 1.3;
    stroke-linejoin: round;
  }
  /* blood red; the slightly lighter edge keeps its shape readable on charcoal */
  .wins .top {
    fill: var(--accent);
    stroke: color-mix(in oklab, var(--accent) 70%, white);
  }
  /* being overridden is normal: neutral, so red stays for real problems */
  .loses .bottom {
    fill: var(--muted);
    stroke: var(--muted);
  }
</style>
