<script lang="ts" module>
  import type { IconName } from "./Icon.svelte";
  /** A menu entry; null draws a separator, `heading` a group's title (`hint`: a line of help, as written). `checked` makes it a tick-box
      entry (its tick shows), and `keep` leaves the menu open after a pick (filters set several at once). */
  export type MenuItem =
    | { label: string; icon?: IconName; run: () => void; disabled?: boolean; danger?: boolean; checked?: boolean; keep?: boolean }
    | { heading: string; hint?: boolean }
    | null;
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
    tick().then(() => menu?.querySelector<HTMLElement>("[role^=menuitem]:not(:disabled)")?.focus());
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
    {#if item && "heading" in item}
      <div class="heading" class:hint={item.hint} role="presentation">{item.heading}</div>
    {:else if item}
      <button role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"} aria-checked={item.checked} class="item" class:danger={item.danger} disabled={item.disabled}
        onclick={() => { if (!item.keep) onClose(); item.run(); }}>
        <span class="tick">{#if item.checked !== undefined ? item.checked : item.icon}<Icon name={item.checked !== undefined ? "check" : item.icon!} size={14} />{/if}</span>{item.label}
      </button>
    {:else}
      <div class="sep" role="separator"></div>
    {/if}
  {/each}
</div>

<style>
  .popup-menu {
    position: fixed;
    max-height: calc(100vh - 16px);
    overflow-y: auto;
  }
  .heading {
    padding: 8px 10px 3px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .heading.hint {
    font-weight: 400;
    letter-spacing: normal;
    text-transform: none;
  }
</style>
