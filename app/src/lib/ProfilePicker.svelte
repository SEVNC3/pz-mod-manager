<script lang="ts">
  import { tick } from "svelte";
  import type { Mode, View } from "./api";
  import Icon from "./Icon.svelte";

  // One picker for "which mod list am I playing": every profile, grouped under its game.
  // Picking a profile from the other game goes through the mode switch (onSwitchGame).
  let { view, busy, onUse, onSwitchGame, onNew }: {
    view: View | null;
    busy: boolean;
    onUse: (name: string) => void;
    onSwitchGame: (target: Mode, profile?: string) => void;
    onNew: () => void;
  } = $props();

  const GAMES: [Mode, string, string][] = [
    ["zomboid", "Project Zomboid", "/brand/project-zomboid.png"],
    ["viewpoint", "Project Viewpoint", "/brand/project-viewpoint.png"],
  ];
  let open = $state(false);
  let root = $state<HTMLElement>();
  let trigger = $state<HTMLButtonElement>();

  const mode = $derived<Mode>(view?.mode ?? "zomboid");
  const kindOf = (p: string): Mode => view?.kinds[p] ?? "zomboid";
  const groups = $derived(GAMES.map(([m, label, img]) => ({ m, label, img, profiles: (view?.profiles ?? []).filter((p) => kindOf(p) === m) })));
  const hasViewpoint = $derived(groups[1].profiles.length > 0);
  const current = $derived(GAMES.find(([m]) => m === mode)!);

  async function toggle() {
    open = !open;
    if (open) {
      await tick();
      (root?.querySelector('[aria-checked="true"]') as HTMLElement | null)?.focus();
    }
  }
  function close(refocus = true) {
    open = false;
    if (refocus) trigger?.focus();
  }
  function pick(p: string) {
    close();
    if (p === view?.profile) return;
    if (kindOf(p) === mode) onUse(p);
    else onSwitchGame(kindOf(p), p);
  }
  function menuKey(e: KeyboardEvent) {
    const items = [...(root?.querySelectorAll<HTMLElement>("[role^=menuitem]") ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      items[(i + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
    } else if (e.key === "Tab") close(false);
  }
</script>

<svelte:window onpointerdown={(e) => open && !root?.contains(e.target as Node) && close(false)} />

<div class="picker" bind:this={root}>
  <button bind:this={trigger} class="trigger" onclick={toggle} disabled={busy || !view} aria-haspopup="menu" aria-expanded={open}
    title="Profile: the mod list you're playing">
    <img src={current[2]} alt={current[1]} height="20" />
    <span class="dot" aria-hidden="true">·</span>
    <span class="name">{view?.profile ?? "…"}</span>
    <svg class="caret" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><polyline points="6 9 12 15 18 9" /></svg>
  </button>

  {#if open}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="menu" role="menu" aria-label="Profiles" tabindex="-1" onkeydown={menuKey}>
      {#each groups as g}
        {#if g.profiles.length}
          <div class="group" role="presentation">
            <img src={g.img} alt={g.label} height="16" />
          </div>
          {#each g.profiles as p}
            <button role="menuitemradio" aria-checked={p === view?.profile} class="item" class:on={p === view?.profile} onclick={() => pick(p)}>
              <span class="tick">{#if p === view?.profile}<Icon name="check" size={14} />{/if}</span>{p}
            </button>
          {/each}
        {/if}
      {/each}
      <div class="sep" role="separator"></div>
      <button role="menuitem" class="item" onclick={() => { close(false); onNew(); }}>
        <span class="tick"><Icon name="plus" size={14} /></span>New profile…
      </button>
      {#if !hasViewpoint}
        <button role="menuitem" class="item" onclick={() => { close(); onSwitchGame("viewpoint"); }}>
          <span class="tick"><Icon name="swap" size={14} /></span>Set up Project Viewpoint…
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  .trigger {
    gap: 8px;
    padding: 5px 10px;
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 6px;
    max-width: 340px;
  }
  .trigger img {
    display: block;
    height: 20px;
    width: auto;
  }
  .dot {
    color: var(--muted);
  }
  .name {
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .caret {
    fill: none;
    stroke: var(--muted);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 30;
    min-width: 260px;
    padding: 4px;
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .group {
    padding: 8px 10px 4px;
  }
  .group img {
    display: block;
    height: 16px;
    width: auto;
    opacity: 0.8;
  }
  .item {
    justify-content: flex-start;
    gap: 6px;
    padding: 5px 10px;
    border-radius: 4px;
    text-align: left;
  }
  .item.on {
    font-weight: 700;
  }
  .tick {
    width: 16px;
    display: inline-flex;
    color: inherit;
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
  }
  /* the wordmarks are white: darken them on the light theme */
  :global(:root[data-theme="light"]) .picker img {
    filter: invert(1);
  }
  @media (prefers-color-scheme: light) {
    :global(:root:not([data-theme="dark"])) .picker img {
      filter: invert(1);
    }
  }
</style>
