<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "./i18n.svelte";

  // The window has no system frame (decorations: false): these replace its buttons.
  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const sync = async () => (maximized = await win.isMaximized());
    sync();
    const off = win.onResized(sync);
    return () => {
      off.then((f) => f());
    };
  });
</script>

<div class="winctl">
  <button onclick={() => win.minimize()} aria-label={t("window.minimise")} title={t("window.minimise")}>
    <svg viewBox="0 0 10 10" aria-hidden="true"><line x1="0" y1="5.5" x2="10" y2="5.5" /></svg>
  </button>
  <button onclick={() => win.toggleMaximize()} aria-label={maximized ? t("window.restore") : t("window.maximise")} title={maximized ? t("window.restore") : t("window.maximise")}>
    {#if maximized}
      <svg viewBox="0 0 10 10" aria-hidden="true"><rect x="0.5" y="2.5" width="7" height="7" /><polyline points="2.5 2.5 2.5 0.5 9.5 0.5 9.5 7.5 7.5 7.5" /></svg>
    {:else}
      <svg viewBox="0 0 10 10" aria-hidden="true"><rect x="0.5" y="0.5" width="9" height="9" /></svg>
    {/if}
  </button>
  <button class="close" onclick={() => win.close()} aria-label={t("common.close")} title={t("common.close")}>
    <svg viewBox="0 0 10 10" aria-hidden="true"><line x1="0" y1="0" x2="10" y2="10" /><line x1="10" y1="0" x2="0" y2="10" /></svg>
  </button>
</div>

<style>
  .winctl {
    position: absolute;
    top: 0;
    right: 0;
    display: flex;
    z-index: 5;
  }
  button {
    width: 46px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--muted);
  }
  button:hover:not(:disabled) {
    background: var(--raised);
    color: var(--text);
  }
  button.close:hover:not(:disabled) {
    background: #c42b1c;
    color: #fff;
  }
  svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
</style>
