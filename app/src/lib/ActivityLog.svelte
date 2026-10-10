<script lang="ts">
  import Icon, { type IconName } from "./Icon.svelte";

  let { log, busy, busyLabel, open = $bindable() }: {
    log: { text: string; error: boolean }[];
    busy: boolean;
    busyLabel: string;
    open: boolean;
  } = $props();

  /** The icon for an activity line, from what it says. */
  function logIcon(text: string, error: boolean): IconName {
    const t = text.toLowerCase();
    if (error || t.startsWith("error")) return "error";
    if (t.startsWith("installed") || t.includes("downloaded") || t.startsWith("downloading")) return "download";
    if (t.includes("restored")) return "rollback";
    if (t.includes("update")) return "refresh";
    if (t.startsWith("delete") || t.startsWith("remove") || t.includes("removed")) return "trash";
    if (t.startsWith("switched")) return "swap";
    if (t.includes("requires") || t.includes("found on the workshop") || t.startsWith("collection")) return "layers";
    if (t.includes("copied a share code")) return "share";
    if (t.startsWith("started")) return "server";
    if (t.includes("up to date") || t.includes("set up") || t.startsWith("copied") || t.startsWith("imported") || t.startsWith("picked up")) return "check";
    if (t.includes("couldn't") || t.includes("isn't") || t.includes("not ")) return "alert";
    return "info";
  }
</script>

<footer class="activity" class:open>
  <button class="quiet bar" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="dot" class:busy aria-hidden="true"></span>
    <span class="label">{busy ? busyLabel + "…" : "Activity"}</span>
    {#if log.length}<Icon name={logIcon(log.at(-1)!.text, log.at(-1)!.error)} size={12} />{/if}
    <span class="last">{log.at(-1)?.text ?? ""}</span>
  </button>
  {#if open}
    <ol class="log mono" aria-live="polite">
      {#each log as l}
        <li class:error={l.error}><Icon name={logIcon(l.text, l.error)} size={12} /><span>{l.text}</span></li>
      {:else}
        <li class="quiet-line">Installs, updates and errors show up here.</li>
      {/each}
    </ol>
  {/if}
</footer>

<style>
  .activity {
    border-top: 1px solid var(--line);
    background: var(--panel);
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 10px;
    width: 100%;
    padding: 7px 16px;
    border-radius: 0;
    text-align: left;
  }
  .label {
    font-weight: 700;
  }
  .last {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
    opacity: 0.6;
  }
  .dot.busy {
    opacity: 1;
    background: var(--accent-text);
    animation: pulse 1s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.3;
    }
  }
  .log {
    list-style: none;
    margin: 0;
    padding: 4px 16px 10px;
    max-height: 180px;
    overflow-y: auto;
  }
  .log li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 2px 0;
    overflow-wrap: anywhere;
  }
  .log li :global(.icon) {
    margin-top: 3px;
    color: var(--muted);
  }
  .log li.error :global(.icon) {
    color: var(--danger);
  }
  .quiet-line {
    color: var(--muted);
  }
</style>
