<script lang="ts">
  import type { Finding, ModErrors } from "./api";
  import Icon from "./Icon.svelte";
  import { t, th } from "./i18n.svelte";

  // The setup check (findings, Lua errors from the last game) and the find-the-broken-mod rounds.
  let { findings, gameErrors, bisect, busy, onClose, onSelect, onBisectStart, onBisectAnswer, onBisectStop }: {
    findings: Finding[] | null;
    gameErrors: ModErrors[];
    bisect: { suspects: string[]; testing: string[]; round: number } | null;
    busy: boolean;
    onClose: () => void;
    onSelect: (id: string) => void;
    onBisectStart: () => void;
    onBisectAnswer: (stillBroken: boolean) => void;
    onBisectStop: () => void;
  } = $props();
</script>

{#if bisect}
  <div class="findings" role="region" aria-label={t("findings.findBroken")}>
    <p>
      <strong>{t("findings.round", { round: bisect.round })}</strong>
      {t("findings.suspects", { count: bisect.suspects.length, testing: bisect.testing.length })}
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
  <div class="findings" role="region" aria-label={t("findings.setupCheck")}>
    <div class="findings-head">
      <strong>
        {findings.length === 0 ? t("findings.noProblems") : t("findings.summary", { errors: t("findings.errors", { count: findings.filter((f) => f.level === "error").length }), warnings: t("findings.warnings", { count: findings.filter((f) => f.level === "warning").length }) })}
      </strong>
      <button class="quiet" onclick={onClose}><Icon name="x" size={14} />{t("common.close")}</button>
    </div>
    {#if findings.length}
      <ul>
        {#each findings as f}
          <li class={f.level}><span class="mono">{f.subject}</span> {f.text}</li>
        {/each}
      </ul>
    {/if}
    {#if gameErrors.length}
      <p class="findings-sub">{@html th("findings.luaErrors")}</p>
      <ul>
        {#each gameErrors as e}
          <li class="warning">
            <button class="quiet link" disabled={!e.id} onclick={() => e.id && onSelect(e.id)}>{e.name}</button>:
            {@html th("findings.errorCount", { count: e.count, example: e.example })}
          </li>
        {/each}
      </ul>
    {/if}
    <p class="findings-sub">
      {t("findings.unexplained")}
      <button class="quiet link" onclick={onBisectStart} disabled={busy}>{t("findings.findBroken")}</button>:
      {t("findings.halves")}
    </p>
  </div>
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
  .link {
    padding: 0 4px;
  }
</style>
