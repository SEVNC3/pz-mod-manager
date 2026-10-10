<script lang="ts">
  import { api } from "./api";

  // An empty url shows the icon as a disabled "coming soon" stub.
  const LINKS = {
    github: "https://github.com/SEVNC3/pz-mod-manager",
    discord: "https://discord.gg/TfS9NwNHrH",
    kofi: "https://ko-fi.com/sevnc3",
  };

  // Each brand's own logo (static/brand/*.png, downloaded once from logo.dev).
  const ICONS = [
    { key: "github", label: "PZMM on GitHub: source, issues and releases" },
    { key: "discord", label: "Join the PZMM Discord" },
    { key: "kofi", label: "Support PZMM on Ko-fi" },
  ] as const;
</script>

<nav class="links" aria-label="PZMM links">
  {#each ICONS as icon (icon.key)}
    {@const url = LINKS[icon.key]}
    {@const label = url ? icon.label : `${icon.label} (coming soon)`}
    <button class="quiet" title={label} aria-label={label}
      disabled={!url} onclick={() => api.openUrl(url)}>
      <img src={`/brand/${icon.key}.png`} alt="" width="20" height="20" />
    </button>
  {/each}
</nav>

<style>
  .links {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: 8px;
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
  button:hover:not(:disabled) img {
    opacity: 1;
  }
  button:disabled img {
    filter: grayscale(1);
    opacity: 0.35;
  }
</style>
