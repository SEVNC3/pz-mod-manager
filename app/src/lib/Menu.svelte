<script lang="ts" module>
  import type { IconName } from "./Icon.svelte";
  /** A menu entry; null draws a separator. */
  export type MenuItem = { label: string; icon: IconName; run: () => void; disabled?: boolean; danger?: boolean } | null;
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { menuKeys } from "./ui";

  // A popup menu at a point (right-click menus, Browse…), the profile menu's look and keys (app.css .popup-menu, ui.ts menuKeys).
  // Arrows move, Enter picks, Esc / Tab / a click elsewhere close; Esc and a pick put focus back where it was.
  let { x, y, label, items, onClose }: { x: number; y: number; label: string; items: MenuItem[]; onClose: () => void } = $props();

  let menu = $state<HTMLElement>();
  // svelte-ignore state_referenced_locally
  let at = $state({ left: x, top: y });
  let refocus = true;

  onMount(() => {
    const back = document.activeElement as HTMLElement | null;
    // kept inside the window
    const r = menu!.getBoundingClientRect();
    at = { left: Math.max(4, Math.min(x, innerWidth - r.width - 4)), top: Math.max(4, Math.min(y, innerHeight - r.height - 4)) };
    tick().then(() => menu?.querySelector<HTMLElement>("[role=menuitem]:not(:disabled)")?.focus());
    return () => refocus && back?.focus?.();
  });

  function close(back: boolean) {
    refocus = back;
    onClose();
  }
</script>

<svelte:window onpointerdown={(e) => !menu?.contains(e.target as Node) && onClose()} onblur={onClose} onresize={onClose} />

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div class="popup-menu" role="menu" aria-label={label} bind:this={menu} style:left="{at.left}px" style:top="{at.top}px" onkeydown={(e) => menuKeys(e, menu, close)}
  oncontextmenu={(e) => e.preventDefault()}>
  {#each items as item}
    {#if item}
      <button role="menuitem" class="item" class:danger={item.danger} disabled={item.disabled} onclick={() => { onClose(); item.run(); }}>
        <span class="tick"><Icon name={item.icon} size={14} /></span>{item.label}
      </button>
    {:else}
      <div class="sep" role="separator"></div>
    {/if}
  {/each}
</div>

<style>
  .popup-menu {
    position: fixed;
  }
</style>
