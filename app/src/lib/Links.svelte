<script lang="ts">
  import { api } from "./api";
  import { t } from "./i18n.svelte";

  // An empty url shows the icon as a disabled "coming soon" stub.
  const LINKS = {
    github: "https://github.com/SEVNC3/pz-mod-manager",
    discord: "https://discord.com/invite/6URUy6mNzu",
    kofi: "https://ko-fi.com/sevnc3",
  };

  // Each brand's own logo (static/brand/*.png, downloaded once from logo.dev).
  const ICONS = [
    { key: "github", label: "links.github", glyph: true },
    { key: "discord", label: "links.discord", glyph: true },
    { key: "kofi", label: "links.kofi", glyph: false },
  ] as const;
</script>

<nav class="links" aria-label={t("links.label")}>
  {#each ICONS as icon (icon.key)}
    {@const url = LINKS[icon.key]}
    {@const label = url ? t(icon.label) : t("links.comingSoon", { label: t(icon.label) })}
    <button class="quiet" title={label} aria-label={label}
      disabled={!url} onclick={() => api.openUrl(url)}>
      <img src={`/brand/${icon.key}.png`} alt="" width="20" height="20" class:glyph={icon.glyph} />
    </button>
  {/each}
</nav>

<style>
  .links {
    display: flex;
    align-items: center;
    gap: 4px;
    /* the bar's right end, however wide the window */
    margin-left: auto;
    padding-left: 8px;
  }
  button {
    display: grid;
    place-items: center;
    padding: 5px;
  }
  img {
    display: block;
    border-radius: 5px;
    opacity: 0.85;
    transition: opacity 0.15s;
  }
  img.glyph {
    filter: var(--glyph);
  }
  button:hover:not(:disabled) img {
    opacity: 1;
  }
  button:disabled img {
    filter: grayscale(1);
    opacity: 0.35;
  }
</style>
