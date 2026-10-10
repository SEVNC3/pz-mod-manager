<script lang="ts">
  import { api, describeSource, type Conflict, type Details, type ModRow } from "./api";

  let {
    id,
    mods,
    busy,
    hasUpdate,
    onUpdate,
    onToggle,
    onRemove,
    onRollback,
    onSelect,
  }: {
    id: string;
    mods: Map<string, ModRow>;
    busy: boolean;
    hasUpdate: boolean;
    onUpdate: (id: string) => void;
    onToggle: (id: string) => void;
    onRemove: (id: string) => void;
    onRollback: (id: string) => void;
    onSelect: (id: string) => void;
  } = $props();

  let d = $state<Details | null>(null);
  let conflicts = $state<Conflict[]>([]);
  let poster = $state<string | null>(null);
  let loadedFor = "";

  // Reload when the selection changes, and after any list change (busy flips back),
  // so rollback/remove are reflected.
  $effect(() => {
    const target = id;
    void busy;
    if (busy) return;
    api.details(target).then((r) => {
      if (target === id) d = r;
    });
    api.conflicts(target).then((c) => {
      if (target === id) conflicts = c;
    }).catch(() => (conflicts = []));
    if (loadedFor !== target) {
      loadedFor = target;
      if (poster) URL.revokeObjectURL(poster);
      poster = null;
      api.poster(target).then((buf) => {
        if (target === id && buf.byteLength) poster = URL.createObjectURL(new Blob([buf]));
      });
    }
  });

  const row = $derived(mods.get(id));
  const source = $derived(describeSource(d?.source ?? row?.source ?? ""));
  const description = $derived(
    (d?.info.description ?? "").replace(/<\s*(LINE|br)\s*\/?>/gi, "\n").replace(/<[^>]+>/g, "").trim(),
  );
  const requires = $derived(
    (d?.info.require ?? "")
      .split(/[,;]/)
      .map((s) => s.trim().replace(/^\\+/, ""))
      .filter(Boolean),
  );
  const facts = $derived(
    [
      ["Author", d?.info.author],
      ["Mod version", d?.info.modversion],
      ["Needs game version", d?.info.versionMin ? `${d.info.versionMin} or newer` : undefined],
    ].filter((f): f is [string, string] => !!f[1]),
  );
</script>

<aside class="details" aria-label="Mod details">
  {#if row?.missing}
    <h2>{id}</h2>
    <p class="warn">
      This mod is enabled in your profile but isn't installed. Install it, subscribe to it in Steam, or disable it.
    </p>
    <button onclick={() => onToggle(id)} disabled={busy}>Disable</button>
  {:else if d}
    {#if poster}
      <img class="poster" src={poster} alt="" />
    {/if}
    <h2>{d.name || d.id}</h2>
    <p class="id mono">{d.id}</p>

    <div class="actions">
      <button class={row?.on ? "" : "primary"} onclick={() => onToggle(id)} disabled={busy}>
        {row?.on ? "Disable" : "Enable"}
      </button>
      <button onclick={() => api.openPath(d!.dir)}>Show files</button>
      {#if source.url}
        <button onclick={() => api.openUrl(source.url!)}>Open {source.label === "GitHub" ? "GitHub" : "Workshop"} page</button>
      {/if}
    </div>

    <dl>
      <dt>Source</dt>
      <dd>{source.label}{#if d.source.startsWith("local:")}<span class="mono path">{d.source.slice(6)}</span>{/if}</dd>
      {#each facts as [k, v]}
        <dt>{k}</dt>
        <dd>{v}</dd>
      {/each}
    </dl>

    {#if hasUpdate && d.ours}
      <div class="update">
        <span>A newer version is on the Workshop.</span>
        <button class="primary" onclick={() => onUpdate(id)} disabled={busy}>Update now</button>
      </div>
    {/if}

    {#if requires.length}
      <h3>Requires</h3>
      <ul class="requires">
        {#each requires as r}
          {@const m = mods.get(r)}
          <li>
            <button class="quiet link mono" onclick={() => m && onSelect(r)} disabled={!m}>{r}</button>
            <span class={m?.on ? "ok" : "bad"}>{m ? (m.on ? "enabled" : "disabled") : "not installed"}</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#if conflicts.length}
      <h3>Shared files</h3>
      <ul class="conflicts">
        {#each conflicts as c}
          <li title={c.examples.join(", ")}>
            <span class="mono">{c.other}</span>
            <span class={c.wins ? "ok" : "bad"}>
              {c.files} {c.files === 1 ? "file" : "files"}, {c.wins ? "this mod's copies win" : `${c.other}'s copies win`}
            </span>
          </li>
        {/each}
      </ul>
      <p class="note small">The mod lower in the load order wins. Drag to change which one's files are used.</p>
    {/if}

    {#if description}
      <h3>Description</h3>
      <p class="desc">{description}</p>
    {/if}

    {#if d.ours}
      <div class="actions owned">
        {#if d.has_backup}
          <button onclick={() => onRollback(id)} disabled={busy}>Restore previous version</button>
        {/if}
        <button class="danger" onclick={() => onRemove(id)} disabled={busy}>Remove</button>
      </div>
    {:else if d.source.startsWith("steam:")}
      <p class="note">This mod comes from your Steam subscriptions. Unsubscribe in Steam to remove it.</p>
    {/if}
  {/if}
</aside>

<style>
  .details {
    overflow-y: auto;
    padding: 16px 18px 24px;
    border-left: 1px solid var(--line);
    background: var(--panel);
  }
  .poster {
    display: block;
    width: 100%;
    max-height: 220px;
    object-fit: cover;
    border-radius: 8px;
    margin-bottom: 12px;
    background: var(--raised);
  }
  h2 {
    margin: 0;
    font-size: 20px;
    line-height: 1.2;
  }
  h3 {
    margin: 18px 0 6px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  .id {
    margin: 2px 0 12px;
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .owned {
    margin-top: 22px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 16px 0 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .path {
    display: block;
    color: var(--muted);
  }
  .requires {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .requires li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    align-items: baseline;
  }
  .link {
    padding: 0;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .ok {
    color: var(--muted);
  }
  .bad,
  .warn {
    color: var(--danger);
  }
  .desc {
    white-space: pre-line;
    margin: 0;
    color: color-mix(in oklab, var(--text) 85%, var(--muted));
  }
  .note {
    margin-top: 22px;
    color: var(--muted);
  }
  .note.small {
    margin-top: 4px;
    font-size: 12px;
  }
  .update {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: 14px;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--accent-soft);
  }
  .conflicts {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .conflicts li {
    display: flex;
    flex-direction: column;
    padding: 3px 0;
  }
</style>
