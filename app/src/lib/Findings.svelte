<script lang="ts">
  import Icon from "./Icon.svelte";
  import { fmtNum } from "./format";
  import { t } from "./i18n.svelte";

  // The find-the-broken-mod rounds (Check setup's results are its own window, CheckSetup.svelte).
  let { bisect, busy, onBisectAnswer, onBisectStop }: {
    bisect: { suspects: string[][]; testing: string[][]; round: number } | null;
    busy: boolean;
    onBisectAnswer: (stillBroken: boolean) => void;
    onBisectStop: () => void;
  } = $props();
</script>

{#if bisect}
  <div class="findings" role="region" aria-label={t("findings.findBroken")}>
    <p>
      <strong>{t("findings.round", { round: bisect.round })}</strong>
      {t("findings.suspects", { count: bisect.suspects.flat().length, testing: fmtNum(bisect.testing.flat().length) })}
      {t("findings.testHint")}
    </p>
    <div class="row">
      <button onclick={() => onBisectAnswer(true)} disabled={busy}><Icon name="alert" size={14} />{t("findings.stillHappens")}</button>
      <button onclick={() => onBisectAnswer(false)} disabled={busy}><Icon name="check" size={14} />{t("findings.gone")}</button>
      <button class="quiet" onclick={onBisectStop} disabled={busy}><Icon name="rollback" size={14} />{t("findings.stop")}</button>
    </div>
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
  }
  .findings p {
    margin: 0 0 8px;
  }
  .findings .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
</style>
