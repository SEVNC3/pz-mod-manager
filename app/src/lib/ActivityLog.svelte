<script lang="ts">
  import { api } from "./api";
  import { fmtNum, fmtTime } from "./format";
  import { t } from "./i18n.svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  let { log, busy, busyLabel, progress, open = $bindable(), onCancel, onCopy }: {
    log: { text: string; en: string; error: boolean; time: number; url?: string }[];
    busy: boolean;
    busyLabel: string;
    /** Workshop items downloaded, of how many (while a download runs). */
    progress: [number, number] | null;
    open: boolean;
    /** Stop the download: what finished still installs. */
    onCancel: () => void;
    onCopy: () => void;
  } = $props();

  /** The icon for an activity line, from what it says (in English: `en`, so it works in every language). */
  function logIcon(text: string, error: boolean): IconName {
    const t = text.toLowerCase();
    if (error || t.startsWith("error")) return "error"; // i18n-ignore
    if (t.startsWith("installed") || t.includes("downloaded") || t.startsWith("downloading")) return "download"; // i18n-ignore
    if (t.includes("restored")) return "rollback"; // i18n-ignore
    if (t.includes("update")) return "refresh"; // i18n-ignore
    if (t.startsWith("delete") || t.startsWith("remove") || t.includes("removed")) return "trash"; // i18n-ignore
    if (t.startsWith("switched")) return "swap"; // i18n-ignore
    if (t.includes("requires") || t.includes("found on the workshop") || t.startsWith("collection")) return "layers"; // i18n-ignore
    if (t.includes("copied a share code")) return "share"; // i18n-ignore
    if (t.startsWith("started")) return "server"; // i18n-ignore
    if (t.includes("up to date") || t.includes("set up") || t.startsWith("copied") || t.startsWith("imported") || t.startsWith("picked up")) return "check"; // i18n-ignore
    if (t.includes("couldn't") || t.includes("isn't") || t.includes("not ")) return "alert"; // i18n-ignore
    return "info";
  }
</script>

<footer class="activity" class:open>
  <div class="head">
    <button class="quiet bar" aria-expanded={open} onclick={() => (open = !open)}>
      <span class="dot" class:busy aria-hidden="true"></span>
      <span class="label">{busy ? busyLabel + "…" : t("activity.title")}</span>
      {#if busy && progress}<span class="count">{t("activity.progress", { done: fmtNum(progress[0]), total: fmtNum(progress[1]) })}</span>{/if}
      {#if log.length}<Icon name={logIcon(log.at(-1)!.en, log.at(-1)!.error)} size={12} />{/if}
      <!-- read out while the log is closed (open, the list below is) -->
      <span class="last" aria-live={open ? "off" : "polite"}>{log.at(-1)?.text ?? ""}</span>
    </button>
    {#if busy && progress}
      <button class="quiet" onclick={onCancel} title={t("activity.cancelTip")}><Icon name="x" size={14} />{t("common.cancel")}</button>
    {/if}
    {#if open && log.length}
      <button class="quiet" onclick={onCopy} title={t("activity.copyTip")}><Icon name="copy" size={14} />{t("activity.copy")}</button>
    {/if}
  </div>
  {#if open}
    <ol class="log mono" aria-live="polite">
      {#each log as l (l)}
        <li class:error={l.error}><span class="time">{fmtTime(l.time)}</span><Icon name={logIcon(l.en, l.error)} size={12} /><span>{l.text}
          {#if l.url}<button class="quiet link" onclick={() => api.openUrl(l.url!)}>{t("activity.whatChanged")}</button>{/if}</span></li>
      {:else}
        <li class="quiet-line">{t("activity.empty")}</li>
      {/each}
    </ol>
  {/if}
</footer>

<style>
  .activity {
    border-top: 1px solid var(--line);
    background: var(--panel);
  }
  .head {
    display: flex;
    align-items: center;
    padding-right: 10px;
  }
  .bar {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 10px;
    padding: 7px 16px;
    border-radius: 0;
    text-align: left;
  }
  .label {
    font-weight: 700;
  }
  .count {
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .time {
    flex: none;
    color: var(--muted);
  }
  .link {
    margin-left: 6px;
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
