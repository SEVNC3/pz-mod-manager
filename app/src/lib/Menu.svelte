<script lang="ts" module>
  import type { IconName } from "./Icon.svelte";
  /** A menu entry; null draws a separator. */
  export type MenuItem = { label: string; icon: IconName; run: () => void; disabled?: boolean; danger?: boolean } | null;
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "./Icon.svelte";

  // A popup menu at a point (right-click menus, Browse…), styled like the profile menu.
  // Arrows move, Enter picks, Esc / Tab / a click elsewhere close; focus goes back where it was.
  let { x, y, label, items, onClose }: { x: number; y: number; label: string; items: MenuItem[]; onClose: () => void } = $props();

  let menu = $state<HTMLElement>();
  // svelte-ignore state_referenced_locally
  let at = $state({ left: x, top: y });

  onMount(() => {
    const back = document.activeElement as HTMLElement | null;
    // kept inside the window
    const r = menu!.getBoundingClientRect();
    at = { left: Math.max(4, Math.min(x, innerWidth - r.width - 4)), top: Math.max(4, Math.min(y, innerHeight - r.height - 4)) };
    tick().then(() => menu?.querySelector<HTMLElement>("button:not(:disabled)")?.focus());
    return () => back?.focus?.();
  });

  function key(e: KeyboardEvent) {
    const all = [...(menu?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? [])];
    const i = all.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      all[(i + (e.key === "ArrowDown" ? 1 : all.length - 1)) % all.length]?.focus();
    }
  }
</script>

<svelte:window onpointerdown={(e) => !menu?.contains(e.target as Node) && onClose()} onblur={onClose} onresize={onClose} />

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div class="menu" role="menu" aria-label={label} bind:this={menu} style:left="{at.left}px" style:top="{at.top}px" onkeydown={key}
  oncontextmenu={(e) => e.preventDefault()}>
  {#each items as item}
    {#if item}
      <button role="menuitem" class="item" class:danger={item.danger} disabled={item.disabled} onclick={() => { onClose(); item.run(); }}>
        <Icon name={item.icon} size={14} />{item.label}
      </button>
    {:else}
      <div class="sep" role="separator"></div>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 40;
    min-width: 220px;
    padding: 4px;
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .item {
    justify-content: flex-start;
    gap: 8px;
    padding: 5px 10px;
    border-radius: 4px;
    text-align: left;
  }
  .item.danger {
    color: var(--danger);
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
  }
</style>
