<script lang="ts">
  import { onDestroy } from "svelte";
  import LayerIcon from "./LayerIcon.svelte";
  import Icon from "./Icon.svelte";
  import { fmtAgo, fmtDate, fmtNum, fmtSize, workshopUrl } from "./format";
  import { keyedError, t } from "./i18n.svelte";
  import { api, describeSource, type Candidate, type ChangeNote, type Conflict, type ModDetails, type ModRow, type Rule } from "./api";

  let {
    id,
    mods,
    busy,
    lock = false,
    hasUpdate,
    held = false,
    heldNewer = 0,
    onHold,
    onUpdateAnyway,
    neededBy,
    tags,
    allTags,
    onTags,
    onRule,
    onUpdate,
    onInstallWorkshop,
    onToggle,
    onRemove,
    onRollback,
    onSelect,
    onClose,
    firstSeen = 0,
    size = 0,
    updated = 0,
    category = "",
  }: {
    id: string;
    mods: Map<string, ModRow>;
    busy: boolean;
    /** When it first arrived on this PC (0: unknown, not shown). */
    firstSeen?: number;
    /** Its size and when its files last changed (0: unknown), and PZMM's category for it ("" until read). */
    size?: number;
    updated?: number;
    category?: string;
    /** A search for a broken mod owns the lists: installs, Enable/Disable and Remove wait. */
    lock?: boolean;
    hasUpdate: boolean;
    /** Its updates are held, and when the Workshop's newer version came out (0: none). */
    held?: boolean;
    heldNewer?: number;
    /** Hold its updates, or allow them again. */
    onHold: (id: string) => void;
    /** Update a held mod (asks first; it stays held). */
    onUpdateAnyway: (id: string) => void;
    /** Enabled mods that require this one. */
    neededBy: string[];
    /** The player's own tags on this mod, and every tag in use (suggestions). */
    tags: string[];
    allTags: string[];
    onTags: (id: string, tags: string[]) => void;
    /** Save the mod's rule in Mod Load Order Sorter's rules file. */
    onRule: (id: string, rule: Rule) => void;
    onUpdate: (id: string) => void;
    onInstallWorkshop: (workshopId: string) => Promise<boolean>;
    onToggle: (id: string) => void;
    onRemove: (id: string) => void;
    onRollback: (id: string) => void;
    onSelect: (id: string) => void;
    onClose: () => void;
  } = $props();

  let d = $state<ModDetails | null>(null);
  /** The mod's rule in the Sorter's rules file (what the editor changes; mod.info's own rules show beside it). */
  let rule = $state<Rule | null>(null);
  let conflicts = $state<Conflict[]>([]);
  // Workshop matches for missing requirements, by mod id
  let found = $state<Record<string, Candidate[] | "loading" | string>>({});

  async function download(req: string) {
    found[req] = "loading";
    try {
      const list = await api.findRequirement(req, id);
      if (list[0]?.confirmed && list[0]?.fits_build) {
        delete found[req];
        await onInstallWorkshop(list[0].id);
      } else {
        found[req] = list;
      }
    } catch (e) {
      found[req] = t(...keyedError(e));
    }
  }
  let poster = $state<string | null>(null);
  onDestroy(() => poster && URL.revokeObjectURL(poster));
  let loadedFor = "";

  /** What changed: the Workshop change notes since the installed version, for a mod with an update. */
  let changes = $state<ChangeNote[] | string | null>(null);
  $effect(() => {
    const target = id;
    changes = null;
    if (!hasUpdate) return;
    api.modChanges(target).then((c) => target === id && (changes = c)).catch((e) => target === id && (changes = t(...keyedError(e))));
  });

  // Reload when the selection changes, and after any list change (busy flips back),
  // so rollback/remove are reflected.
  $effect(() => {
    const target = id;
    if (busy) return;
    if (loadedFor !== target) [d, rule, conflicts] = [null, null, []]; // never the last mod's, even when this one fails
    api.details(target).then((r) => {
      if (target === id) d = r;
    }).catch(() => {});
    api.modRule(target).then((r) => target === id && (rule = r)).catch(() => (rule = null));
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
  type ListKey = "load_after" | "load_before" | "incompatible";
  /** Ids the mod's own mod.info lists (the row has both; the rules file's are editable). */
  const fromMod = (k: ListKey) => (row?.[k] ?? []).filter((x) => !rule?.[k].includes(x));
  function setRule(patch: Partial<Rule>) {
    if (!rule) return;
    rule = { ...rule, ...patch };
    onRule(id, rule);
  }
  /** Add what's typed in `input` to rule list `k`. */
  function addTo(k: ListKey, input: HTMLInputElement) {
    const v = input.value.trim().replace(/^\\+/, "");
    input.value = "";
    if (rule && v && v !== id && !rule[k].includes(v)) setRule({ [k]: [...rule[k], v] });
  }
  function addTag(input: HTMLInputElement) {
    const v = input.value.trim();
    input.value = "";
    if (v && !tags.some((x) => x.toLowerCase() === v.toLowerCase())) onTags(id, [...tags, v]);
  }
  const POSITIONS = ["", "on", "category"];
  /** Mods it can't run with that are on now. */
  const clashOn = $derived((row?.incompatible ?? []).filter((x) => mods.get(x)?.on));

  const facts = $derived(
    [
      [t("details.author"), d?.info.author],
      [t("details.modVersion"), d?.info.modversion],
      [t("details.needsGameVersion"), d?.info.versionMin ? t("details.versionOrNewer", { version: d.info.versionMin }) : undefined],
      [t("details.pzmmCategory"), category],
      [t("details.workshopId"), (d?.source ?? "").match(/^(?:workshop|steam):(\d+)/)?.[1]],
      [t("details.updated"), fmtAgo(updated)],
      [t("modTable.col.size"), size ? fmtSize(size) : undefined],
      [t("details.languages"), row?.languages.join(", ")],
    ].filter((f): f is [string, string] => !!f[1]),
  );
</script>

<!-- the Workshop items that may provide mod `r` (a missing requirement, or this mod when it isn't installed) -->
{#snippet matches(r: string)}
  {#if found[r] === "loading"}
    <p class="hint">{t("details.looking", { id: r })}</p>
  {:else if typeof found[r] === "string"}
    <p class="bad">{found[r]}</p>
  {:else if Array.isArray(found[r])}
    {@const list = found[r] as Candidate[]}
    {#if list.length === 0}
      <p class="hint">{t("details.noMatch", { id: r })}</p>
    {:else}
      <p class="hint">{t("details.pickOne", { id: r })}</p>
      <ul class="cands">
        {#each list.slice(0, 5) as c (c.id)}
          <li class:best={c.confirmed && c.fits_build}>
            <span>
              <button class="quiet link" onclick={() => api.openUrl(workshopUrl(c.id))}>{c.title}</button>
              <span class="hint">
                {t("details.subscribers", { count: c.subscriptions, n: fmtNum(c.subscriptions) })}{c.confirmed ? ` · ${t("details.idMatches")}` : ""}{c.required_by_author ? ` · ${t("details.listedByAuthor")}` : ""}{c.fits_build ? "" : ` · ${t("details.notTaggedForBuild")}`}
              </span>
            </span>
            <button class={c.confirmed && c.fits_build ? "primary" : ""} disabled={busy || lock} onclick={() => onInstallWorkshop(c.id)}><Icon name="download" size={14} />{t("common.install")}</button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
{/snippet}

<aside class="details" aria-label={t("details.label")}>
  <button class="quiet icon close" onclick={onClose} title={t("details.closeTip")} aria-label={t("details.closeLabel")}><Icon name="x" size={16} /></button>
  {#if row?.missing}
    <h2>{id}</h2>
    <p class="warn">
      {t("details.missingWarn")}
    </p>
    <div class="actions">
      {#if !found[id]}<button class="primary" onclick={() => download(id)} disabled={busy || lock} title={t("details.findTip")}><Icon name="download" size={14} />{t("common.install")}</button>{/if}
      <button onclick={() => onToggle(id)} disabled={busy || lock}><Icon name="x" size={14} />{t("common.disable")}</button>
    </div>
    {@render matches(id)}
  {:else if d}
    {#if poster}
      <img class="poster" src={poster} alt="" />
    {/if}
    <h2>{d.name || d.id}</h2>
    <p class="id mono">{d.id}</p>

    <div class="actions">
      <button class={row?.on ? "" : "primary"} onclick={() => onToggle(id)} disabled={busy || lock}>
        <Icon name={row?.on ? "x" : "check"} size={14} />{t(row?.on ? "common.disable" : "common.enable")}
      </button>
      <button onclick={() => api.openPath(d!.dir)}><Icon name="folder" size={14} />{t("details.showFiles")}</button>
      {#if source.url}
        <button onclick={() => api.openUrl(source.url!)}><Icon name={source.label === "GitHub" ? "external" : "steam"} size={14} />{t(source.label === "GitHub" ? "details.githubPage" : "details.workshopPage")}</button>
      {/if}
    </div>

    <dl>
      <dt>{t("details.source")}</dt>
      <dd>{source.label}{#if d.source.startsWith("local:")}<span class="mono path">{d.source.slice(6)}</span>{/if}</dd>
      {#each facts as [k, v]}
        <dt>{k}</dt>
        <dd>{v}</dd>
      {/each}
      {#if /^(workshop|github|steam):/.test(d.source)}
        <dt>{t("details.updates")}</dt>
        <dd class="hold">
          {t(held ? "details.updatesHeld" : "details.updatesAllowed")}
          {#if held && heldNewer}<span class="hint">{t("details.heldNewer", { date: fmtDate(heldNewer, "long") })}</span>{/if}
          <span class="hold-acts">
            <button class="quiet" onclick={() => onHold(id)} disabled={busy}><Icon name={held ? "unlock" : "lock"} size={14} />{t(held ? "menu.allowUpdates" : "menu.holdUpdates")}</button>
            {#if held && d.ours}<button onclick={() => onUpdateAnyway(id)} disabled={busy}><Icon name="refresh" size={14} />{t("details.updateAnyway")}</button>{/if}
          </span>
        </dd>
      {/if}
      {#if firstSeen}
        <dt>{t("details.firstSeen")}</dt>
        <dd title={t("details.firstSeenTip")}>{fmtDate(firstSeen, "long")}</dd>
      {/if}
      <dt>{t("details.tags")}</dt>
      <dd class="chips">
        {#each tags as x}
          <span class="chip">{x}<button class="quiet x" aria-label={t("details.removeTag", { tag: x })} title={t("details.removeTag", { tag: x })} onclick={() => onTags(id, tags.filter((y) => y !== x))}><Icon name="x" size={10} /></button></span>
        {/each}
        <input class="add" type="text" list="pzmm-tags" placeholder={t("details.addTag")} aria-label={t("details.addTag")} onchange={(e) => addTag(e.currentTarget)} />
      </dd>
    </dl>
    {#if clashOn.length}<p class="bad">{t("details.incompatibleOn", { names: clashOn.map((x) => mods.get(x)?.name || x).join(", ") })}</p>{/if}
    <datalist id="pzmm-tags">{#each allTags.filter((x) => !tags.includes(x)) as x}<option value={x}></option>{/each}</datalist>

    {#if hasUpdate && d.ours}
      <div class="update">
        <span>{t("details.newerVersion")}</span>
        <button class="primary" onclick={() => onUpdate(id)} disabled={busy}><Icon name="refresh" size={14} />{t("details.updateNow")}</button>
      </div>
      <h3>{t("details.whatChanged")}</h3>
      {#if changes === null}
        <p class="hint">{t("details.loadingChanges")}</p>
      {:else if typeof changes === "string"}
        <p class="bad">{changes}</p>
      {:else if !changes.length}
        <p class="hint">{t("details.noChangeNotes")}</p>
      {:else}
        <ol class="notes">
          {#each changes as c}<li><strong>{fmtDate(c.time, "long")}</strong><p>{c.text}</p></li>{/each}
        </ol>
      {/if}
    {/if}

    {#if requires.length}
      <h3>{t("details.requires")}</h3>
      <ul class="requires">
        {#each requires as r}
          {@const m = mods.get(r)}
          <li>
            <span class="req">
              <button class="quiet link mono" onclick={() => m && onSelect(r)} disabled={!m}>{r}</button>
              {#if m}
                <span class={m.on ? "ok" : "bad"}>{t(m.on ? "details.reqEnabled" : "details.reqDisabled")}</span>
              {:else}
                <span class="bad">{t("details.reqMissing")}</span>
                {#if !found[r]}<button class="find" onclick={() => download(r)} disabled={busy || lock}
                  title={t("details.findTip")}><Icon name="download" size={12} />{t("common.install")}</button>{/if}
              {/if}
            </span>
            {#if !m}{@render matches(r)}{/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if neededBy.length}
      <h3>{t("details.neededBy")}</h3>
      <ul class="requires">
        {#each neededBy as n}
          <li><button class="quiet link mono" onclick={() => onSelect(n)}>{mods.get(n)?.name || n}</button></li>
        {/each}
      </ul>
    {/if}

    {#if rule}
      <h3>{t("details.rules")}</h3>
      <dl class="rules">
        {#each [["load_first", t("details.loadFirst")], ["load_last", t("details.loadLast")]] as [k, label]}
          <dt><label for="rule-{k}">{label}</label></dt>
          <dd>
            <select id="rule-{k}" value={rule[k as "load_first"]} disabled={busy} onchange={(e) => setRule({ [k]: e.currentTarget.value })}>
              {#each POSITIONS as v}<option value={v}>{t(`details.position.${v || "none"}`)}</option>{/each}
            </select>
          </dd>
        {/each}
        {#each [["load_after", t("details.loadAfter")], ["load_before", t("details.loadBefore")], ["incompatible", t("details.incompatible")]] as [k, label]}
          {@const key = k as ListKey}
          <dt>{label}</dt>
          <dd class="chips">
            {#each fromMod(key) as x}<span class="chip mono own" title={t("details.fromMod")}>{x}</span>{/each}
            {#each rule[key] as x}
              <span class="chip mono">{x}<button class="quiet x" disabled={busy} aria-label={t("details.removeRuleMod", { id: x })} title={t("details.removeRuleMod", { id: x })}
                onclick={() => setRule({ [key]: rule![key].filter((y) => y !== x) })}><Icon name="x" size={10} /></button></span>
            {/each}
            <input class="add mono" type="text" list="pzmm-mod-ids" disabled={busy} placeholder={t("details.addRuleMod")} aria-label={`${label}: ${t("details.addRuleMod")}`} onchange={(e) => addTo(key, e.currentTarget)} />
          </dd>
        {/each}
        <dt><label for="rule-category">{t("details.category")}</label></dt>
        <dd>
          <input id="rule-category" class="add mono" type="text" list="pzmm-sorter-categories" value={rule.category} disabled={busy} onchange={(e) => setRule({ category: e.currentTarget.value.trim() })} />
        </dd>
      </dl>
      <datalist id="pzmm-mod-ids">{#each [...mods.keys()].filter((m) => m !== id) as m}<option value={m}>{mods.get(m)?.name}</option>{/each}</datalist>
      <!-- i18n-ignore: the Sorter's own category ids -->
      <datalist id="pzmm-sorter-categories">{#each ["coreRequirement", "tweaks", "resource", "map"] as c}<option value={c}></option>{/each}</datalist>
      <p class="note small">{t("details.rulesHint")}
        {#if rule.load_after.length || rule.load_before.length || rule.incompatible.length || rule.load_first || rule.load_last || rule.category}
          <button class="quiet link" disabled={busy} onclick={() => setRule({ load_after: [], load_before: [], incompatible: [], load_first: "", load_last: "", category: "" })}>{t("details.clearRules")}</button>
        {/if}
      </p>
    {/if}

    {#if conflicts.length}
      <h3>{t("details.sharedFiles")}</h3>
      <ul class="conflicts">
        {#each conflicts as c}
          <li title={c.examples.join(", ")}>
            <span class="who">
              <LayerIcon wins={c.wins} loses={!c.wins} label={c.wins ? t("details.winsOver", { other: c.other }) : t("details.othersWin", { other: c.other })} />
              <span class="mono">{c.other}</span>
            </span>
            <span class={c.wins ? "ok" : "bad"}>
              {c.wins ? t("details.filesWin", { count: c.files }) : t("details.filesLose", { count: c.files, other: c.other })}
            </span>
          </li>
        {/each}
      </ul>
      <p class="note small">{t("details.lowerWins")}</p>
    {/if}

    {#if description}
      <h3>{t("details.description")}</h3>
      <p class="desc">{description}</p>
    {/if}

    {#if d.ours}
      <div class="actions owned">
        {#if d.has_backup}
          <button onclick={() => onRollback(id)} disabled={busy}><Icon name="rollback" size={14} />{t("details.restore")}</button>
        {/if}
        <button class="danger" onclick={() => onRemove(id)} disabled={busy || lock}><Icon name="trash" size={14} />{t("common.remove")}</button>
      </div>
    {:else if d.source.startsWith("steam:")}
      <p class="note">{t("details.steamNote")}</p>
      {#if source.url}
        <button onclick={() => api.openUrl(source.url!)}><Icon name="steam" size={14} />{t("details.unsubscribe")}</button>
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
    background: linear-gradient(var(--hover), var(--hover)), var(--panel);
    border-color: var(--hover-line);
    color: var(--on-selected);
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
  .notes {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 260px;
    overflow-y: auto;
  }
  .notes li {
    padding: 6px 0;
    border-bottom: 1px solid var(--line);
  }
  .notes p {
    margin: 2px 0 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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
  .rules {
    margin: 0;
    align-items: center;
  }
  .hold,
  .hold-acts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 12px;
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 0 2px 0 8px;
    line-height: 20px;
  }
  .chip.own {
    padding-right: 8px;
    color: var(--muted);
    border-style: dashed;
  }
  .chip .x {
    padding: 0 4px;
    line-height: 1;
    color: var(--muted);
  }
  .chip .x:hover:not(:disabled) {
    color: var(--danger);
  }
  input.add {
    width: 130px;
    padding: 2px 6px;
    font-size: 12px;
  }
</style>
