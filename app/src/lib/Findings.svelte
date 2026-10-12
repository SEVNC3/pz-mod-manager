<script lang="ts">
  import type { Finding, LogReport } from "./api";
  import Icon from "./Icon.svelte";
  import { fmtNum } from "./format";
  import GameLog from "./GameLog.svelte";
  import { intlLang, t, th } from "./i18n.svelte";

  // The setup check (findings, errors in the last game and server logs) and the find-the-broken-mod rounds.
  let { findings, logs, bisect, busy, lock = false, names, onClose, onSelect, onInstall, onDisable, onDismiss, onBisectStart, onBisectAnswer, onBisectStop }: {
    findings: Finding[] | null;
    logs: LogReport[];
    bisect: { suspects: string[]; testing: string[]; round: number } | null;
    busy: boolean;
    /** The search for a broken mod owns the lists: Install and Disable wait. */
    lock?: boolean;
    /** A mod's name, by ID. */
    names: (id: string) => string;
    onClose: () => void;
    onSelect: (id: string) => void;
    /** A finding's fixes: install the missing mod `id` that `by` needs; disable mod `id`. */
    onInstall: (id: string, by: string) => void;
    onDisable: (id: string) => void;
    /** A scan's finding looked at and accepted: stop reporting the file with this SHA-256. */
    onDismiss: (sha: string) => void;
    /** Start the search, testing `first` (the mods the logs point at) before halving the rest. */
    onBisectStart: (first: string[]) => void;
    onBisectAnswer: (stillBroken: boolean) => void;
    onBisectStop: () => void;
  } = $props();

  /** Installed mods the logs point at: each first error's, then those with errors. */
  const suspects = $derived([...new Set(logs.flatMap((l) => [...(l.first?.ids ?? []), ...l.mods.flatMap((m) => (m.id ? [m.id] : []))]))]);
  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
  let reading = $state(false);
</script>

{#if bisect}
  <div class="findings" role="region" aria-label={t("findings.findBroken")}>
    <p>
      <strong>{t("findings.round", { round: bisect.round })}</strong>
      {t("findings.suspects", { count: bisect.suspects.length, testing: fmtNum(bisect.testing.length) })}
      {t("findings.testHint")}
    </p>
    <div class="row">
      <button onclick={() => onBisectAnswer(true)} disabled={busy}><Icon name="alert" size={14} />{t("findings.stillHappens")}</button>
      <button onclick={() => onBisectAnswer(false)} disabled={busy}><Icon name="check" size={14} />{t("findings.gone")}</button>
      <button class="quiet" onclick={onBisectStop} disabled={busy}><Icon name="rollback" size={14} />{t("findings.stop")}</button>
    </div>
  </div>
{/if}

{#if findings}
  <!-- one sentence for the search offer, its button where the {link} is -->
  {@const [before, after = ""] = t("findings.bisectOffer").split("{link}")}
  {@const [lead, tail = ""] = t("findings.bisectSuspects", { count: suspects.length }).split("{link}")}
  <!-- focus comes here when the check answers; Esc closes it -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions: Esc closes it -->
  <div class="findings" role="region" aria-label={t("findings.setupCheck")} tabindex="-1" {@attach (n) => n.focus()}
    onkeydown={(e) => e.key === "Escape" && !reading && (e.stopPropagation(), onClose())}>
    <div class="findings-head">
      <strong>
        {findings.length === 0 ? t("findings.noProblems") : t("findings.summary", { errors: t("findings.errors", { count: findings.filter((f) => f.level === "error").length }), warnings: t("findings.warnings", { count: findings.filter((f) => f.level === "warning").length }) })}
      </strong>
      <span>
        <button class="quiet" onclick={() => (reading = true)} title={t("gameLog.openTip")}><Icon name="message" size={14} />{t("gameLog.open")}</button>
        <button class="quiet" onclick={onClose}><Icon name="x" size={14} />{t("common.close")}</button>
      </span>
    </div>
    {#if findings.length}
      <ul>
        {#each findings as f}
          <li class={f.level}><span class="mono">{f.subject}</span> {t(`doctor.${f.key}`, f.vars)}
            {#if f.action?.install}
              {@const id = f.action.install}
              <button class="fix" onclick={() => onInstall(id, f.subject)} disabled={busy || lock}><Icon name="download" size={12} />{t("common.install")}</button>
            {:else if f.action?.disable}
              {@const id = f.action.disable}
              <button class="fix" onclick={() => onDisable(id)} disabled={busy || lock}><Icon name="x" size={12} />{t("findings.disable", { id })}</button>
            {:else if f.action?.dismiss}
              {@const sha = f.action.dismiss}
              <button class="fix" onclick={() => onDismiss(sha)} disabled={busy} title={t("findings.dismissTip")}><Icon name="check" size={12} />{t("findings.dismiss")}</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#each logs as log}
      <p class="findings-sub">{@html th("findings.logErrors", { file: fileName(log.file) })}</p>
      {#if log.first}
        <p class="first">
          {t("findings.firstError", { line: log.first.line })}
          <!-- the mods as a list in the UI's language, each a link -->
          {#if log.first.ids.length}{t("findings.pointsAt")}
            {#each new Intl.ListFormat(intlLang(), { type: "unit" }).formatToParts(log.first.ids) as p}{#if p.type === "element"}<button class="quiet link" onclick={() => onSelect(p.value)}>{p.value}</button>{:else}{p.value}{/if}{/each}{/if}
        </p>
        <pre class="mono excerpt">{log.first.text}</pre>
      {/if}
      {#if log.mods.length}
        <ul>
          {#each log.mods as e}
            <li class="warning">
              <button class="quiet link" disabled={!e.id} onclick={() => e.id && onSelect(e.id)}>{e.name}</button>:
              {@html th("findings.errorCount", { count: e.count, example: e.example })}
            </li>
          {/each}
        </ul>
      {/if}
    {/each}
    {#if suspects.length}
      <p class="findings-sub">{lead}<button class="quiet link" onclick={() => onBisectStart(suspects)} disabled={busy || !!bisect}>{t("findings.findBroken")}</button>{tail}</p>
    {:else}
      <p class="findings-sub">{before}<button class="quiet link" onclick={() => onBisectStart([])} disabled={busy || !!bisect}>{t("findings.findBroken")}</button>{after}</p>
    {/if}
  </div>
  {#if reading}<GameLog {names} {onSelect} onClose={() => (reading = false)} onBisect={busy || bisect ? undefined : onBisectStart} />{/if}
{/if}

<style>
  /* a card above the list, like the other strips there */
  .findings {
    padding: 8px 16px 10px;
    margin: 10px 16px 0;
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
    max-height: 30vh;
    overflow-y: auto;
  }
  .findings-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .findings-sub {
    margin: 8px 0 0;
    color: var(--muted);
  }
  .findings .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .findings ul {
    margin: 6px 0 0;
    padding-left: 18px;
  }
  .findings li.warning {
    color: var(--text);
  }
  /* inside a sentence the text's own spaces separate it */
  .link {
    padding: 0;
  }
  .findings:focus {
    outline: none;
  }
  .first {
    margin: 4px 0 0;
  }
  .excerpt {
    margin: 4px 0 0;
    padding: 6px 8px;
    background: var(--raised);
    border-radius: 6px;
    font-size: 12px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 9em;
    overflow-y: auto;
  }
  .fix {
    padding: 1px 10px;
    font-size: 12px;
  }
</style>
