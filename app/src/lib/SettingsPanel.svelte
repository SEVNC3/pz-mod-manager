<script lang="ts">
  import { api, type View } from "./api";
  import Icon from "./Icon.svelte";

  // Always mounted (the theme applies while the panel is closed); `open` shows the panel.
  let { open, view, busy, appVersion, onBuild, onRedownload, onCheckUpdate }: {
    open: boolean;
    view: View | null;
    busy: boolean;
    appVersion: string;
    onBuild: (build: 41 | 42 | null) => void;
    onRedownload: () => void;
    onCheckUpdate: () => void;
  } = $props();

  // --- per-viewer preferences (theme, accent) ---
  const pref = (k: string, d: string) => {
    try {
      return localStorage.getItem(k) ?? d;
    } catch {
      return d;
    }
  };
  let theme = $state(pref("pzmm.theme", "system"));
  const DEFAULT_ACCENT = "#6e080c"; // the blood on the separator tape (static/blood.svg)
  // people still on an earlier default (amber, then pink) move to the current one
  let accent = $state([pref("pzmm.accent", DEFAULT_ACCENT)].map((a) => (["#d39b3a", "#fc2684"].includes(a) ? DEFAULT_ACCENT : a))[0]);
  /** Text colour on an accent fill: dark on light accents, white on dark ones. */
  const onAccent = (hex: string) => {
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.45 ? "#1b1a17" : "#ffffff";
  };
  $effect(() => {
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
    root.style.setProperty("--accent", accent);
    root.style.setProperty("--on-accent", onAccent(accent));
    try {
      localStorage.setItem("pzmm.theme", theme);
      localStorage.setItem("pzmm.accent", accent);
    } catch {}
  });
</script>

{#if open}
  <div class="settings" role="region" aria-label="Settings">
    <label>Theme
      <select bind:value={theme}>
        <option value="system">Match system</option>
        <option value="dark">Dark</option>
        <option value="light">Light</option>
      </select>
    </label>
    <label>Accent <input type="color" bind:value={accent} /></label>
    <label>Game build
      <select value={String(view?.build ?? "")} disabled={busy}
        onchange={(e) => { const v = e.currentTarget.value; onBuild(v ? (Number(v) as 41 | 42) : null); }}>
        <option value="">Detect from default.txt</option>
        <option value="42">Build 42</option>
        <option value="41">Build 41</option>
      </select>
    </label>
    <button onclick={onRedownload} disabled={busy} title="Repairs damaged copies; each keeps its current version as a backup"><Icon name="download" size={14} />Re-download mods PZMM installed</button>
    <span class="keys">
      Keyboard: <kbd>/</kbd> search · <kbd>↑</kbd><kbd>↓</kbd> move between rows · <kbd>Enter</kbd> details ·
      <kbd>Space</kbd> enable/disable · <kbd>Alt</kbd>+<kbd>↑</kbd><kbd>↓</kbd> reorder · <kbd>Ctrl</kbd>+<kbd>Z</kbd>/<kbd>Y</kbd> undo/redo ·
      <kbd>Ctrl</kbd>/<kbd>Shift</kbd>+click select several · <kbd>Esc</kbd> clear selection
    </span>
    <span class="version">PZMM {appVersion}
      <button class="quiet" onclick={onCheckUpdate}>Check for updates</button></span>
    <div class="paths">
      <span>Zomboid folder <button class="quiet link mono" onclick={() => view && api.openPath(view.zomboid)}>{view?.zomboid}</button></span>
      <span>Steam Workshop folder <span class="mono">{view?.workshop ?? "not found"}</span></span>
    </div>
  </div>
{/if}

<style>
  /* a card above the list, like the other strips there */
  .settings {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    align-items: center;
    gap: 12px 18px;
    padding: 14px 18px;
    margin: 10px 16px 0;
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  .settings label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }
  .settings input[type="color"] {
    width: 34px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: none;
  }
  .version {
    color: var(--muted);
  }
  .paths {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--muted);
    margin-left: auto;
  }
  .link {
    padding: 0 4px;
  }
  .keys {
    grid-column: 1 / -1;
    color: var(--muted);
    font-size: 12px;
  }
  kbd {
    font-family: var(--font-mono);
    font-size: 11px;
    border: 1px solid var(--line);
    border-bottom-width: 2px;
    border-radius: 3px;
    padding: 0 4px;
    background: var(--raised);
  }
</style>
