<script lang="ts">
  import { tick } from "svelte";
  import { modeOf, type Mode, type View } from "./api";
  import { t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import { menuKeys } from "./ui";

  // One picker for "which mod list am I playing": every profile, grouped under its game.
  // Picking a profile from the other game goes through the mode switch (onSwitchGame).
  // Rename, Duplicate, Delete and Snapshots act on the active profile (Remove unused server profiles asks first); right-clicking a profile offers them
  // for that one. While `lock` says why (a search for a broken mod is running on this profile), only
  // Snapshots is open: the rest would leave or change it.
  let { view, busy, lock = "", onUse, onSwitchGame, onNew, onRename, onDuplicate, onDelete, onPrune, onSnapshots }: {
    view: View | null;
    busy: boolean;
    lock?: string;
    onUse: (name: string) => void;
    onSwitchGame: (target: Mode, profile?: string) => void;
    onNew: () => void;
    onRename: (name: string) => void;
    onDuplicate: (name: string) => void;
    /** Delete a profile (the active one: after switching to another of its game). */
    onDelete: (name: string) => void;
    /** Ask, then delete the server profiles you no longer use. */
    onPrune: () => void;
    onSnapshots: () => void;
  } = $props();

  const GAMES: Mode[] = ["zomboid", "viewpoint"];
  let open = $state(false);
  let root = $state<HTMLElement>();
  let trigger = $state<HTMLButtonElement>();

  const mode = $derived<Mode>(view?.mode ?? "zomboid");
  const kindOf = (p: string): Mode => modeOf(view?.kinds[p]);
  /** Profiles made for a server you joined: listed under their game, apart from the rest. */
  const isServer = (p: string) => view?.kinds[p] === "server";
  const groups = $derived(
    GAMES.map((m) => {
      const all = (view?.profiles ?? []).filter((p) => kindOf(p) === m);
      return { m, profiles: all.filter((p) => !isServer(p)), servers: all.filter(isServer) };
    }),
  );
  const hasViewpoint = $derived(groups[1].profiles.length + groups[1].servers.length > 0);

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
  /** A profile can go unless it's the active one and its game has no other to switch to. */
  const deletable = (p: string) => p !== view?.profile || (view?.profiles ?? []).some((o) => o !== p && kindOf(o) === kindOf(p));
  /** A profile's own menu (right-click), where it was asked. */
  let menuFor = $state<{ p: string; x: number; y: number } | null>(null);
  const menuItems = (p: string): MenuItem[] => [
    ...(p !== view?.profile ? [{ label: t("profile.use"), icon: "check" as const, run: () => pick(p), disabled: !!lock }, null] : []),
    { label: t("profile.rename"), icon: "pencil", run: () => (close(false), onRename(p)), disabled: !!lock },
    { label: t("profile.duplicate"), icon: "copy", run: () => (close(false), onDuplicate(p)), disabled: !!lock },
    { label: t("profile.delete"), icon: "trash", danger: true, run: () => (close(false), onDelete(p)), disabled: !!lock || !deletable(p) },
  ];

  function pick(p: string) {
    close();
    if (p === view?.profile) return;
    if (kindOf(p) === mode) onUse(p);
    else onSwitchGame(kindOf(p), p);
  }
</script>

<svelte:window onpointerdown={(e) => open && !menuFor && !root?.contains(e.target as Node) && close(false)} />

<div class="picker" bind:this={root}>
  <button bind:this={trigger} class="trigger" onclick={toggle} disabled={busy || !view} aria-haspopup="menu" aria-expanded={open}
    title={t("profile.tip")}>
    <span class="lbl">{t("profile.label")}</span>
    <span class="name">{view?.profile ?? "…"}</span>
    <svg class="caret" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><polyline points="6 9 12 15 18 9" /></svg>
  </button>

  {#if open}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="popup-menu" role="menu" aria-label={t("profile.menuLabel")} tabindex="-1" onkeydown={(e) => menuKeys(e, root, close)}>
      {#snippet item(p: string)}
        <button role="menuitemradio" aria-checked={p === view?.profile} class="item" class:on={p === view?.profile} onclick={() => pick(p)}
          oncontextmenu={(e) => { e.preventDefault(); menuFor = { p, x: e.clientX, y: e.clientY }; }}
          disabled={!!lock && p !== view?.profile} title={p === view?.profile ? "" : lock}>
          <span class="tick">{#if p === view?.profile}<Icon name="check" size={14} />{/if}</span>{p}
        </button>
      {/snippet}
      <!-- each game's profiles under its name, a line between the games -->
      {#each groups.filter((g) => g.profiles.length || g.servers.length) as g, i}
        {#if i}<div class="sep" role="separator"></div>{/if}
        <div class="group" role="presentation">{t(`profile.games.${g.m}`)}</div>
        {#each g.profiles as p}{@render item(p)}{/each}
        {#if g.servers.length}
          <div class="sub" role="presentation">{t("profile.servers")}</div>
          {#each g.servers as p}{@render item(p)}{/each}
        {/if}
      {/each}
      <div class="sep" role="separator"></div>
      <button role="menuitem" class="item" onclick={() => { close(false); onNew(); }} disabled={!!lock} title={lock}>
        <span class="tick"><Icon name="plus" size={14} /></span>{t("profile.newProfile")}
      </button>
      <button role="menuitem" class="item" onclick={() => { close(false); onRename(view?.profile ?? ""); }} disabled={!!lock} title={lock}>
        <span class="tick"><Icon name="pencil" size={14} /></span>{t("profile.rename")}
      </button>
      <button role="menuitem" class="item" onclick={() => { close(false); onDuplicate(view?.profile ?? ""); }} disabled={!!lock} title={lock}>
        <span class="tick"><Icon name="copy" size={14} /></span>{t("profile.duplicate")}
      </button>
      <button role="menuitem" class="item" onclick={() => { close(false); onDelete(view?.profile ?? ""); }} disabled={!!lock || !deletable(view?.profile ?? "")} title={lock}>
        <span class="tick"><Icon name="trash" size={14} /></span>{t("profile.delete")}
      </button>
      {#if groups.some((g) => g.servers.length)}
        <button role="menuitem" class="item" onclick={() => { close(false); onPrune(); }} disabled={!!lock} title={lock}>
          <span class="tick"><Icon name="trash" size={14} /></span>{t("profile.prune")}
        </button>
      {/if}
      <button role="menuitem" class="item" onclick={() => { close(); onSnapshots(); }}>
        <span class="tick"><Icon name="rollback" size={14} /></span>{t("profile.snapshots")}
      </button>
      {#if !hasViewpoint}
        <button role="menuitem" class="item" onclick={() => { close(); onSwitchGame("viewpoint"); }} disabled={!!lock} title={lock}>
          <span class="tick"><Icon name="swap" size={14} /></span>{t("profile.setUpViewpoint")}
        </button>
      {/if}
    </div>
  {/if}
  {#if menuFor}
    <Menu x={menuFor.x} y={menuFor.y} label={menuFor.p} items={menuItems(menuFor.p)} onClose={() => (menuFor = null)} />
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
  .lbl {
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
  /* a long list scrolls inside the window */
  .popup-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    max-height: calc(100vh - 72px);
    overflow-y: auto;
  }
  /* headings: a game's profiles, then (indented) its server profiles */
  .group,
  .sub {
    padding: 8px 10px 4px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .sub {
    padding: 6px 10px 2px 34px;
  }
  .item.on {
    font-weight: 700;
  }
</style>
