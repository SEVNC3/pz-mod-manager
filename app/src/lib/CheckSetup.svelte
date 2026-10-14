<script lang="ts">
  import type { Finding, LogReport } from "./api";
  import { groups, remedies, SEVERITIES, type Fixes, type Group } from "./findings";
  import { intlLang, t, th } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  /** Check setup: what's wrong, worst first, each with what fixes it. Problems that can't stop the
   * game can be hidden ("It's fine"); they come back when their mod changes. */
  let { findings, logs, failed, checking, busy, fixes, names, onHide, onRecheck, onReadLog, onSelect, onFindBroken, onClose }: {
    findings: Finding[];
    /** Errors in the last game and server logs, and the mods they point at. */
    logs: LogReport[];
    /** A check is running: what shows may change. */
    checking: boolean;
    /** The check couldn't run: no results to show. */
    failed: boolean;
    busy: boolean;
    fixes: Fixes;
    names: (id: string) => string;
    onHide: (f: Finding, on: boolean) => Promise<unknown>;
    /** Check again (after a fix). */
    onRecheck: () => Promise<unknown>;
    onReadLog: () => void;
    /** Show a mod the logs point at. */
    onSelect: (id: string) => void;
    onFindBroken: () => void;
    onClose: () => void;
  } = $props();

  const all = $derived(groups(findings));
  const shown = $derived(all.filter((g) => !g.hidden));
  const ignored = $derived(all.filter((g) => g.hidden));
  let showIgnored = $state(false);
  let working = $state(false);

  const fixesOf = (g: Group) => remedies(g.finding, g.ids, fixes);
  /** Fix all: each group's first safe fix (installs, turning requirements on, sorting, ZombieBuddy). */
  const safe = $derived(shown.flatMap((g) => fixesOf(g).filter((r) => r.safe).slice(0, 1)));
  /** Hidden by "It's fine": never what stops the game; security warnings are trusted per file instead. */
  const hideable = (g: Group) => g.severity === "check" || g.severity === "low";

  async function act(run: () => Promise<unknown> | void) {
    working = true;
    try {
      await run();
      await onRecheck();
    } finally {
      working = false;
    }
  }
  async function fixAll() {
    working = true;
    try {
      // one at a time: installs and sorting each change the list the next one reads
      for (const r of safe) await r.run();
      await onRecheck();
    } finally {
      working = false;
    }
  }
  const who = (g: Group) => g.ids.map(names).join(", ");
  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
</script>

{#snippet line(g: Group)}
  <li class={g.severity}>
    <div class="what">
      <span class="who">{who(g)}</span>
      <span>{t(`doctor.${g.key}`, g.finding.vars)}</span>
    </div>
    <div class="acts">
      {#if g.hidden}
        <button class="quiet" disabled={busy || working} onclick={() => act(() => Promise.all(g.ids.map((id) => onHide({ ...g.finding, subject: id }, false))))}><Icon name="eye" size={14} />{t("check.fix.unhide")}</button>
      {:else}
        {#each fixesOf(g) as r, i}
          <button class={i === 0 ? "primary" : "quiet"} disabled={busy || working} onclick={() => act(r.run)}><Icon name={r.icon} size={14} />{t(`check.fix.${r.label}`, r.vars)}</button>
        {/each}
        {#if hideable(g)}
          <button class="quiet" disabled={busy || working} onclick={() => act(() => Promise.all(g.ids.map((id) => onHide({ ...g.finding, subject: id }, true))))} title={t("check.hideTip")}><Icon name="eye-off" size={14} />{t("check.fix.hide")}</button>
        {/if}
      {/if}
    </div>
  </li>
{/snippet}

<Modal title={t("check.title")} width={900} {onClose}>
  {#snippet head()}
    <span class="sum">{checking && !findings.length ? t("app.checking") : failed ? t("home.health.failed") : shown.length ? t("check.summary", { count: shown.length }) : t("findings.noProblems")}{#if checking && findings.length} · {t("app.checking")}{/if}</span>
  {/snippet}
  <div class="body">
    {#each SEVERITIES as s}
      {@const list = shown.filter((g) => g.severity === s)}
      {#if list.length}
        <details class="sev {s}" open={s !== "low"}>
          <summary><Icon name="chevron-right" size={14} /><span class="badge">{t(`check.severity.${s}`)}</span> <span class="n">{list.length}</span> <span class="hint">{t(`check.severityHint.${s}`)}</span></summary>
          <ul>{#each list as g}{@render line(g)}{/each}</ul>
        </details>
      {/if}
    {/each}
    {#each logs as log}
      <section class="log">
        <p class="sub">{@html th("findings.logErrors", { file: fileName(log.file) })}</p>
        {#if log.first}
          <p>
            {t("findings.firstError", { line: log.first.line })}
            <!-- the mods as a list in the UI's language, each a link -->
            {#if log.first.ids.length}{t("findings.pointsAt")}
              {#each new Intl.ListFormat(intlLang(), { type: "unit" }).formatToParts(log.first.ids) as p}{#if p.type === "element"}<button class="quiet link" onclick={() => onSelect(p.value)}>{p.value}</button>{:else}{p.value}{/if}{/each}{/if}
          </p>
          <pre class="mono excerpt">{log.first.text}</pre>
        {/if}
        {#if log.mods.length}
          <ul class="errs">
            {#each log.mods as e}
              <li><button class="quiet link" disabled={!e.id} onclick={() => e.id && onSelect(e.id)}>{e.name}</button>: {@html th("findings.errorCount", { count: e.count, example: e.example })}</li>
            {/each}
          </ul>
        {/if}
      </section>
    {/each}
    {#if ignored.length}
      <button class="quiet link" onclick={() => (showIgnored = !showIgnored)}>{t("check.ignored", { count: ignored.length })}</button>
      {#if showIgnored}<ul class="ignored">{#each ignored as g}{@render line(g)}{/each}</ul>{/if}
    {/if}
  </div>
  {#snippet foot()}
    <button class="quiet" onclick={onReadLog} title={t("gameLog.openTip")}><Icon name="message" size={14} />{t("gameLog.open")}</button>
    <button class="quiet" onclick={onFindBroken} disabled={busy}><Icon name="search" size={14} />{t("findings.findBroken")}</button>
    <span class="grow"></span>
    <button class="quiet" onclick={() => act(() => undefined)} disabled={busy || working || checking}><Icon name="refresh" size={14} />{t("check.again")}</button>
    {#if safe.length}
      <button class="primary" onclick={fixAll} disabled={busy || working}><Icon name="wand" size={14} />{t("check.fixAll", { count: safe.length })}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .sum {
    flex: 1;
    color: var(--muted);
    font-size: 13px;
  }
  .grow {
    flex: 1;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 10px 14px;
  }
  details {
    margin-bottom: 10px;
  }
  summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 4px;
    cursor: pointer;
    font-weight: 700;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  details[open] > summary > :global(.icon:first-child) {
    rotate: 90deg;
  }
  .badge {
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 12px;
  }
  .n {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--muted);
  }
  .hint {
    font-weight: 400;
    font-size: 12px;
    color: var(--muted);
  }
  .broken .badge {
    background: color-mix(in oklab, var(--danger) 25%, transparent);
    color: var(--danger);
  }
  .risk .badge {
    background: color-mix(in oklab, var(--warn) 22%, transparent);
    color: var(--warn);
  }
  .check .badge {
    background: var(--accent-soft);
    color: var(--text);
  }
  .low .badge {
    background: color-mix(in oklab, var(--muted) 20%, transparent);
    color: var(--muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-left: 2px solid transparent;
    border-bottom: 1px solid var(--line);
  }
  li.broken {
    border-left-color: var(--danger);
  }
  li.risk {
    border-left-color: var(--warn);
  }
  .what {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
    font-size: 13px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .who {
    font-weight: 700;
  }
  /* the fixes take what they need up to half the row; the sentence keeps the rest */
  .acts {
    flex: 0 1 auto;
    max-width: 55%;
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
  }
  .log {
    margin: 4px 0 12px;
    font-size: 13px;
  }
  .log p {
    margin: 4px 0;
  }
  .sub {
    color: var(--muted);
  }
  .link {
    padding: 0;
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
  .errs li {
    display: block;
    padding: 2px 0;
    border: 0;
  }
  .ignored {
    opacity: 0.7;
  }
</style>
