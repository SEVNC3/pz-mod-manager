<script lang="ts">
  import LayerIcon from "./LayerIcon.svelte";
  import Icon from "./Icon.svelte";
  import { api, describeSource, type Candidate, type Conflict, type Details, type ModRow } from "./api";

  let {
    id,
    mods,
    busy,
    hasUpdate,
    onUpdate,
    onInstallWorkshop,
    onToggle,
    onRemove,
    onRollback,
    onSelect,
    onClose,
  }: {
    id: string;
    mods: Map<string, ModRow>;
    busy: boolean;
    hasUpdate: boolean;
    onUpdate: (id: string) => void;
    onInstallWorkshop: (workshopId: string) => Promise<boolean>;
    onToggle: (id: string) => void;
    onRemove: (id: string) => void;
    onRollback: (id: string) => void;
    onSelect: (id: string) => void;
    onClose: () => void;
  } = $props();

  let d = $state<Details | null>(null);
  let conflicts = $state<Conflict[]>([]);
  // Workshop matches for missing requirements, by mod id
  let found = $state<Record<string, Candidate[] | "loading" | string>>({});

  async function download(req: string) {
    found[req] = "loading";
    try {
      const list = await api.findRequirement(req, id);
      if (list[0]?.confirmed && list[0]?.fits_build) {
        found[req] = [];
        delete found[req];
        await onInstallWorkshop(list[0].id);
      } else {
        found[req] = list;
      }
    } catch (e) {
      found[req] = String(e);
    }
  }
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
    }).catch(() => {});
    api.conflicts(target).then((c) => {
      if (target === id) conflicts = c;
    }).catch(() => (conflicts = []));
    if (loadedFor !== target) {
      loadedFor = target;
      found = {};
      if (poster) URL.revokeObjectURL(poster);
      poster = null;
      api.poster(target).then((buf) => {
        if (target === id && buf.byteLength) poster = URL.createObjectURL(new Blob([buf]));
      }).catch(() => {});
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
  <button class="quiet icon close" onclick={onClose} title="Close (Esc) to see the load order" aria-label="Close details"><Icon name="x" size={16} /></button>
  {#if row?.missing}
    <h2>{id}</h2>
    <p class="warn">
      This mod is enabled in your profile but isn't installed. Install it, subscribe to it in Steam, or disable it.
    </p>
    <button onclick={() => onToggle(id)} disabled={busy}><Icon name="x" size={14} />Disable</button>
  {:else if d}
    {#if poster}
      <img class="poster" src={poster} alt="" />
    {/if}
    <h2>{d.name || d.id}</h2>
    <p class="id mono">{d.id}</p>

    <div class="actions">
      <button class={row?.on ? "" : "primary"} onclick={() => onToggle(id)} disabled={busy}>
        <Icon name={row?.on ? "x" : "check"} size={14} />{row?.on ? "Disable" : "Enable"}
      </button>
      <button onclick={() => api.openPath(d!.dir)}><Icon name="folder" size={14} />Show files</button>
      {#if source.url}
        <button onclick={() => api.openUrl(source.url!)}><Icon name={source.label === "GitHub" ? "external" : "steam"} size={14} />{source.label === "GitHub" ? "GitHub" : "Workshop"} page</button>
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
        <button class="primary" onclick={() => onUpdate(id)} disabled={busy}><Icon name="refresh" size={14} />Update now</button>
      </div>
    {/if}

    {#if requires.length}
      <h3>Requires</h3>
      <ul class="requires">
        {#each requires as r}
          {@const m = mods.get(r)}
          <li>
            <span class="req">
              <button class="quiet link mono" onclick={() => m && onSelect(r)} disabled={!m}>{r}</button>
              {#if m}
                <span class={m.on ? "ok" : "bad"}>{m.on ? "enabled" : "disabled"}</span>
              {:else}
                <span class="bad">not installed</span>
                {#if !found[r]}<button class="find" onclick={() => download(r)} disabled={busy}
                  title="Find it on the Steam Workshop and install it"><Icon name="download" size={12} />Install</button>{/if}
              {/if}
            </span>
            {#if !m && found[r] === "loading"}
              <p class="hint">Looking for {r} on the Workshop…</p>
            {:else if !m && typeof found[r] === "string"}
              <p class="bad">{found[r]}</p>
            {:else if !m && Array.isArray(found[r])}
              {@const list = found[r] as Candidate[]}
              {#if list.length === 0}
                <p class="hint">Nothing on the Workshop matches {r}.</p>
              {:else}
                <p class="hint">No result is clearly {r}. Pick the right one:</p>
                <ul class="cands">
                  {#each list.slice(0, 5) as c (c.id)}
                    <li class:best={c.confirmed && c.fits_build}>
                      <span>
                        <button class="quiet link" onclick={() => api.openUrl(`https://steamcommunity.com/sharedfiles/filedetails/?id=${c.id}`)}>{c.title}</button>
                        <span class="hint">
                          {c.subscriptions.toLocaleString()} subscribers{c.confirmed ? " · Mod ID matches" : ""}{c.required_by_author ? " · listed by the author" : ""}{c.fits_build ? "" : " · not tagged for your game build"}
                        </span>
                      </span>
                      <button class={c.confirmed && c.fits_build ? "primary" : ""} disabled={busy} onclick={() => onInstallWorkshop(c.id)}><Icon name="download" size={14} />Install</button>
                    </li>
                  {/each}
                </ul>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if conflicts.length}
      <h3>Shared files</h3>
      <ul class="conflicts">
        {#each conflicts as c}
          <li title={c.examples.join(", ")}>
            <span class="who">
              <LayerIcon wins={c.wins} loses={!c.wins} label={c.wins ? `This mod's copies win over ${c.other}` : `${c.other}'s copies win`} />
              <span class="mono">{c.other}</span>
            </span>
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
          <button onclick={() => onRollback(id)} disabled={busy}><Icon name="rollback" size={14} />Restore previous version</button>
        {/if}
        <button class="danger" onclick={() => onRemove(id)} disabled={busy}><Icon name="trash" size={14} />Remove</button>
      </div>
    {:else if d.source.startsWith("steam:")}
      <p class="note">This mod comes from your Steam subscriptions. Unsubscribe in Steam to remove it.</p>
      {#if source.url}
        <button onclick={() => api.openUrl(source.url!)}><Icon name="steam" size={14} />Unsubscribe on the Workshop page</button>
      {/if}
    {/if}
  {/if}
</aside>

<style>
  .details {
    position: relative;
    overflow-y: auto;
    padding: 16px 18px 24px;
    border-left: 1px solid var(--line);
    background: var(--panel);
  }
  /* sits on the poster: a rounded square in the panel's colour, so it shows on any image */
  .close {
    position: absolute;
    top: 22px;
    right: 26px;
    z-index: 1;
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: 8px;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.5);
  }
  .close:hover:not(:disabled) {
    background: var(--accent);
    border-color: var(--hover-line);
    color: #fff;
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
  .requires > li {
    padding: 3px 0;
  }
  .req {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .req .link {
    margin-right: auto;
  }
  .find {
    padding: 1px 10px;
    font-size: 12px;
  }
  .hint {
    display: block;
    margin: 2px 0;
    color: var(--muted);
    font-size: 12px;
  }
  .cands {
    list-style: none;
    margin: 4px 0 8px;
    padding: 0 0 0 10px;
    border-left: 2px solid var(--line);
  }
  .cands li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
  }
  .cands li.best {
    border-left: 2px solid var(--accent-text);
    margin-left: -12px;
    padding-left: 10px;
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
  .who {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .conflicts li > span:last-child {
    padding-left: 22px;
  }
</style>
