<script lang="ts">
  import { api, zbReady, type ZbStatus } from "./api";
  import Icon from "./Icon.svelte";

  let { status, profile, busy, onSetup, onRemove }: {
    status: ZbStatus | null;
    profile: string;
    busy: boolean;
    onSetup: () => Promise<void>;
    onRemove: () => Promise<void>;
  } = $props();

  let plan = $state<{ remove: boolean; lines: string[] } | null>(null);
  let error = $state("");

  async function preview(remove: boolean) {
    error = "";
    try {
      plan = { remove, lines: await api.zbPreview(remove) };
    } catch (e) {
      error = String(e);
    }
  }

  async function apply() {
    const remove = plan?.remove;
    plan = null;
    await (remove ? onRemove() : onSetup());
  }

  const ready = $derived(zbReady(status));
  const installedAny = $derived(!!status && (status.files_current || status.launcher_patched));
  const checks = $derived(
    status
      ? [
          [status.have_mod, "ZombieBuddy mod downloaded", "Subscribed in Steam or installed by PZMM"],
          [status.files_current, "Java agent files in the game folder", status.game_dir ?? "Game folder not found"],
          [status.launcher_patched, "Launch flag in ProjectZomboid64.json", "Loads ZombieBuddy when you start the game from Steam"],
          [status.enabled, `Enabled in the "${profile}" profile`, "So mods that need it can load"],
        ] as [boolean, string, string][]
      : [],
  );
</script>

<section class="zb blueprint" aria-label="ZombieBuddy">
  <div class="inner">
    <h2>ZombieBuddy</h2>
    <p class="lede">
      Some mods include Java code that changes the game in ways Lua mods can't. ZombieBuddy is the loader those mods need.
      Setting it up takes two changes outside your mods folder, and PZMM makes both for you.
    </p>

    <p class="caution">
      <strong>Java mods can do anything on your computer.</strong> ZombieBuddy asks you before it loads each new or changed
      Java mod. Only approve mods from authors you trust.
    </p>

    {#if status}
      <ul class="checks">
        {#each checks as [ok, label, hint]}
          <li class:ok>
            <span class="mark" aria-hidden="true">{#if ok}<Icon name="check" size={12} />{/if}</span>
            <span><strong>{label}</strong><span class="hint">{hint}</span></span>
            <span class="visually-hidden">{ok ? "done" : "not done"}</span>
          </li>
        {/each}
      </ul>

      {#if status.required_by.length}
        <p class="needed">
          Needed by {status.required_by.length} enabled {status.required_by.length === 1 ? "mod" : "mods"}:
          <span class="mono">{status.required_by.join(", ")}</span>
        </p>
      {/if}

      {#if plan}
        <div class="plan" role="region" aria-label="Changes to apply">
          <p><strong>{plan.remove ? "Remove ZombieBuddy from the game folder?" : "PZMM will:"}</strong></p>
          {#if plan.lines.length}
            <ul class="mono">{#each plan.lines as l}<li>{l}</li>{/each}</ul>
          {:else}
            <p>Nothing to change.</p>
          {/if}
          <div class="row">
            <button class="primary" onclick={apply} disabled={busy || !plan.lines.length}>
              <Icon name={plan.remove ? "trash" : "check"} size={14} />{plan.remove ? "Remove" : "Apply changes"}
            </button>
            <button class="quiet" onclick={() => (plan = null)}><Icon name="x" size={14} />Cancel</button>
          </div>
        </div>
      {:else}
        <div class="row">
          {#if !ready}
            <button class="primary" onclick={() => preview(false)} disabled={busy || !status.game_dir}>
              <Icon name="settings" size={14} />{installedAny ? "Repair ZombieBuddy" : "Set up ZombieBuddy"}
            </button>
          {/if}
          {#if installedAny}
            <button class="danger" onclick={() => preview(true)} disabled={busy}><Icon name="trash" size={14} />Remove from game folder</button>
          {/if}
          <button class="quiet" onclick={() => api.openUrl("https://github.com/zed-0xff/ZombieBuddy")}><Icon name="external" size={14} />About ZombieBuddy</button>
        </div>
      {/if}
      {#if error}<p class="error">{error}</p>{/if}

      <p class="fine">
        Game updates and Steam's "Verify integrity of game files" remove the launch flag. If that happens, this page shows
        <em>Repair</em>. PZMM changes only the normal launcher. If you use the alternate launcher or Steam launch options,
        use the <button class="quiet link" onclick={() => api.openUrl("https://github.com/zed-0xff/ZombieBuddy/releases/tag/windows_installer")}>official installer</button> instead.
      </p>
    {/if}
  </div>
</section>

<style>
  .zb {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    width: min(760px, calc(100% - 32px));
    margin: 24px auto 40px;
    padding: 22px 26px 24px;
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
  }
  .caution {
    padding: 10px 14px;
    border-left: 3px solid var(--danger);
    background: color-mix(in oklab, var(--danger) 10%, transparent);
    border-radius: 0 6px 6px 0;
  }
  .checks {
    list-style: none;
    padding: 0;
    margin: 20px 0;
    display: grid;
    gap: 10px;
  }
  .checks li {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .mark {
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--raised);
    color: var(--muted);
    border: 1px solid var(--line);
  }
  .checks li.ok .mark {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
    font-weight: 700;
  }
  .hint {
    display: block;
    overflow-wrap: anywhere;
  }
  .needed {
    color: var(--muted);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 14px 0;
  }
  .plan {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px 16px;
    margin: 14px 0;
  }
  .plan ul {
    margin: 6px 0;
    padding-left: 18px;
    overflow-wrap: anywhere;
  }
  .fine {
    color: var(--muted);
    font-size: 13px;
    margin-top: 24px;
  }
  .link {
    text-underline-offset: auto;
    color: inherit;
    font-size: inherit;
  }
</style>
