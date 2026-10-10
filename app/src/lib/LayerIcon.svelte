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
  .wins .top {
    fill: var(--accent);
    stroke: var(--accent);
  }
  .loses .bottom {
    fill: var(--danger);
    stroke: var(--danger);
  }
</style>
