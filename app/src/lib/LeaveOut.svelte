<script lang="ts">
  import { api, type ModRow, type Row } from "./api";
  import { fmtNum } from "./format";
  import { keyedError, t } from "./i18n.svelte";

  // The mods a server gets when PZMM writes its mod lines: everything enabled, less what's unticked here (kept per
  // server, core pz/leave_out.rs). Mods that look client-side are only labelled; nothing is unticked for the player.
  let { server, rows, onChange }: {
    /** The server's ini name. */
    server: string;
    /** The active profile's library. */
    rows: Row[];
    /** After a change is saved (the lines shown follow it). */
    onChange?: () => void;
  } = $props();

  let left = $state<string[]>([]);
  let error = $state("");
  $effect(() => {
    const s = server.trim();
    left = [];
    if (s) api.leaveOutGet(s).then((l) => s === server.trim() && (left = l), () => {});
  });

  const mods = $derived(rows.filter((r): r is ModRow => r.kind === "mod" && r.on && !r.missing));
  const needs = $derived(new Map(rows.filter((r): r is ModRow => r.kind === "mod").map((m) => [m.id, m.requires])));
  /** Its requirements, through installed mods, reach Project Viewpoint or ZombieBuddy (as leave_out::client_side). */
  function clientSide(id: string) {
    if (id === "Viewpoint" || id === "ZombieBuddy") return false;
    const todo = [...(needs.get(id) ?? [])];
    const seen = new Set<string>();
    while (todo.length) {
      const r = todo.pop()!;
      if (r === "Viewpoint" || r === "ZombieBuddy") return true;
      if (!seen.has(r)) {
        seen.add(r);
        todo.push(...(needs.get(r) ?? []));
      }
    }
    return false;
  }

  async function toggle(id: string, send: boolean) {
    const next = send ? left.filter((x) => x !== id) : [...left, id];
    error = "";
    try {
      await api.leaveOutSet(server.trim(), next);
      left = next;
      onChange?.();
    } catch (e) {
      error = t(...keyedError(e));
    }
  }
</script>

{#if server.trim() && mods.length}
  <details class="leave">
    <summary>
      <strong>{t("servers.leaveOut.title")}</strong>
      {#if left.length}<span class="muted">{t("servers.leaveOut.count", { left: fmtNum(left.filter((id) => mods.some((m) => m.id === id)).length), count: fmtNum(mods.length) })}</span>{/if}
    </summary>
    <p class="hint">{t("servers.leaveOut.hint", { server: server.trim() })}</p>
    <ul>
      {#each mods as m (m.id)}
        <li>
          <label>
            <input type="checkbox" checked={!left.includes(m.id)} onchange={(e) => toggle(m.id, e.currentTarget.checked)} />
            <span>{m.name || m.id}</span>
            <!-- i18n-ignore: a mod id -->
            <span class="mono muted">{m.id}</span>
            {#if clientSide(m.id)}<span class="side" title={t("servers.leaveOut.clientSideTip")}>{t("servers.leaveOut.clientSide")}</span>{/if}
          </label>
        </li>
      {/each}
    </ul>
    {#if error}<p class="error">{error}</p>{/if}
  </details>
{/if}

<style>
  .leave {
    margin: 10px 0;
  }
  .leave summary {
    cursor: pointer;
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .leave ul {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    max-height: 320px;
    overflow: auto;
  }
  .leave label {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 2px 0;
  }
  .muted {
    color: var(--muted);
    font-size: 12px;
  }
  .side {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent-text);
  }
</style>
