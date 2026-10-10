<script lang="ts">
  import type { Finding, ModErrors } from "./api";
  import Icon from "./Icon.svelte";

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
  <div class="findings" role="region" aria-label="Find the broken mod">
    <p>
      <strong>Finding the broken mod, round {bisect.round}:</strong>
      {bisect.suspects.length} suspects left, {bisect.testing.length} enabled for this test (plus what they need).
      Start the game, check whether the problem happens, then come back here.
    </p>
    <div class="row">
      <button onclick={() => onBisectAnswer(true)} disabled={busy}><Icon name="alert" size={14} />The problem still happens</button>
      <button onclick={() => onBisectAnswer(false)} disabled={busy}><Icon name="check" size={14} />The problem is gone</button>
      <button class="quiet" onclick={onBisectStop} disabled={busy}><Icon name="rollback" size={14} />Stop and restore my list</button>
    </div>
  </div>
{/if}

{#if findings}
  <div class="findings" role="region" aria-label="Setup check">
    <div class="findings-head">
      <strong>
        {findings.length === 0 ? "No problems found." : `${findings.filter((f) => f.level === "error").length} errors, ${findings.filter((f) => f.level === "warning").length} warnings`}
      </strong>
      <button class="quiet" onclick={onClose}><Icon name="x" size={14} />Close</button>
    </div>
    {#if findings.length}
      <ul>
        {#each findings as f}
          <li class={f.level}><span class="mono">{f.subject}</span> {f.text}</li>
        {/each}
      </ul>
    {/if}
    {#if gameErrors.length}
      <p class="findings-sub"><strong>Lua errors in your last game session</strong> (from console.txt):</p>
      <ul>
        {#each gameErrors as e}
          <li class="warning">
            <button class="quiet link" disabled={!e.id} onclick={() => e.id && onSelect(e.id)}>{e.name}</button>:
            {e.count} {e.count === 1 ? "error" : "errors"}, first at <span class="mono">{e.example}</span>
          </li>
        {/each}
      </ul>
    {/if}
    <p class="findings-sub">
      Something breaks but nothing above explains it?
      <button class="quiet link" onclick={onBisectStart} disabled={busy}>Find the broken mod</button>:
      PZMM halves your enabled mods each time you test the game.
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
