<script lang="ts">
  import { onMount } from "svelte";
  import { api, type SaveDetails, type SavePlayer, type ServerBackup } from "./api";
  import { fmtDateTime, fmtNum, fmtSize, modChanges } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import { arrowPick } from "./ui";
  import Icon from "./Icon.svelte";
  import ServerSettings from "./ServerSettings.svelte";

  let { mode, name, mods, enabledIds, profile, ask, onChanged }: {
    mode: string;
    name: string;
    /** The save's mods; null for a deleted save (only its backups are left). */
    mods: string[] | null;
    enabledIds: string[];
    profile: string;
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
    /** The save changed (restored, new mods): read the list again. */
    onChanged: () => void;
  } = $props();
  const uid = $props.id();

  const PARTS = $derived(mods ? (["backups", "mods", "recovery", "sandbox", "players", "data"] as const) : (["backups"] as const));
  let part = $state<"backups" | "mods" | "recovery" | "sandbox" | "players" | "data">("backups");
  let view = $state<SaveDetails | null>(null);
  let players = $state<SavePlayer[] | null>(null);
  let slot = $state("");
  let error = $state("");
  let done = $state("");
  let working = $state(false);
  const REASONS = ["manual", "launch", "exit", "interval", "crash", "restore", "repair", "reset"];
  const RESETS = ["zombies", "time", "animals"];
  /** Screenshots as object URLs: "" for the save, else by backup file. */
  let thumbs = $state<Record<string, string>>({});
  /** The backup being named, and the name typed. */
  let naming = $state<{ file: string; value: string } | null>(null);

  const load = () =>
    api
      .saveDetails(mode, name)
      .then((v) => {
        view = v;
        for (const b of v.backups) if (!(b.file in thumbs)) thumb(b.file);
      })
      .catch((e) => (error = t(...keyedError(e))));

  /** Closed: a screenshot still on its way isn't made into a URL nobody frees. */
  let closed = false;
  async function thumb(file: string) {
    thumbs[file] = "";
    const buf = await api.saveThumb(mode, name, file).catch(() => null);
    if (buf?.byteLength && !closed) thumbs[file] = URL.createObjectURL(new Blob([buf], { type: "image/png" }));
  }

  async function run(f: () => Promise<unknown>, ok: string) {
    error = done = "";
    working = true;
    try {
      await f();
      done = ok;
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      working = false;
      load();
    }
  }

  const label = (b: ServerBackup) => (b.reason === "slot" ? b.label : REASONS.includes(b.reason) ? t(`saves.reason.${b.reason}`) : b.reason);

  async function backUp() {
    const s = slot.trim();
    await run(() => api.saveBackup(mode, name, s), s ? t("saves.slotSaved", { slot: s }) : t("saves.backedUp"));
    if (!error) slot = "";
  }

  async function restore(b: ServerBackup) {
    const when = fmtDateTime(b.time);
    if ((await ask(t("saves.restoreTitle", { name, when }), [t("saves.restoreKeeps"), t("saves.closeGame")], [{ label: t("saves.restore"), icon: "rollback" }])) !== 0) return;
    await run(() => api.saveRestore(mode, name, b.file), t("saves.restored", { when }));
    onChanged();
  }

  async function remove(b: ServerBackup) {
    if ((await ask(t("saves.deleteTitle", { what: `${label(b)} · ${fmtDateTime(b.time)}` }), [t("saves.deleteWhat")], [{ label: t("common.delete"), icon: "trash" }])) !== 0) return;
    run(() => api.saveBackupDelete(mode, name, b.file), t("saves.deleted"));
  }

  async function nameBackup(b: ServerBackup, copy: boolean) {
    const slot = naming?.value.trim() ?? "";
    if (!slot) return;
    await run(() => api.saveBackupName(mode, name, b.file, slot, copy), t(copy ? "saves.copiedAs" : "saves.renamed", { slot }));
    if (!error) naming = null;
  }

  async function reset(what: string) {
    const count = view?.resets[what] ?? 0;
    if ((await ask(t(`saves.reset.${what}.title`), [t(`saves.reset.${what}.what`), t("saves.reset.backupFirst"), t("saves.closeGame")], [{ label: t("saves.reset.do"), icon: "wand" }])) !== 0) return;
    run(() => api.saveReset(mode, name, what), t("saves.reset.done", { count }));
  }

  async function cloneWithMods() {
    if (!mods) return;
    const added = enabledIds.filter((id) => !mods.includes(id));
    const removed = mods.filter((id) => !enabledIds.includes(id));
    if ((await ask(t("saves.cloneTitle", { save: name, profile }), [...modChanges(added, removed), t("saves.cloneKeeps"), t("saves.closeGame")], [{ label: t("saves.clone"), icon: "copy" }])) !== 0) return;
    let copy = "";
    await run(async () => (copy = await api.saveClone(mode, name)), "");
    if (copy) {
      done = t("saves.cloned", { copy, save: name });
      onChanged();
    }
  }

  async function repair() {
    const count = view?.broken.cells.length ?? 0;
    if ((await ask(t("saves.repairTitle", { count }), [t("saves.repairWhat"), t("saves.closeGame")], [{ label: t("saves.repair"), icon: "wand" }])) !== 0) return;
    run(() => api.saveRepair(mode, name), t("saves.repaired", { count }));
  }

  async function giveMods() {
    if (!mods) return;
    const added = enabledIds.filter((id) => !mods.includes(id));
    const removed = mods.filter((id) => !enabledIds.includes(id));
    const lines = [...modChanges(added, removed), ...(removed.length ? [t("saves.removedWarning")] : []), t("saves.mapsKept"), t("saves.closeGame")];
    if ((await ask(t("saves.giveModsTitle", { save: name, profile }), lines, [{ label: t("saves.giveMods"), icon: "upload" }])) !== 0) return;
    await run(() => api.saveSetMods(mode, name), t("saves.modsGiven"));
    onChanged();
  }

  /** The database browser: the save's databases, the table picked ("file\ntable") and its rows from `offset`. */
  let dbs = $state<[string, [string, number][]][] | null>(null);
  let picked = $state("");
  let offset = $state(0);
  let rows = $state<{ columns: string[]; rows: (string | number | null | { bytes: number })[][] } | null>(null);
  const cell = (v: string | number | null | { bytes: number }) => (v !== null && typeof v === "object" ? t("saves.data.bytes", { size: fmtSize(v.bytes) }) : String(v ?? ""));
  const PAGE = 100;
  const pickedCount = $derived.by(() => {
    const [file, table] = picked.split("\n");
    return dbs?.find(([f]) => f === file)?.[1].find(([n]) => n === table)?.[1] ?? 0;
  });
  async function showTable(from: number) {
    const [file, table] = picked.split("\n");
    if (!table) return;
    error = "";
    try {
      rows = await api.saveTable(mode, name, file, table, from);
      offset = from;
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  // read once, when the part is first opened (null until they arrive: "Reading…")
  let asked = { data: false, players: false };
  $effect(() => {
    if (part === "data" && !asked.data) {
      asked.data = true;
      api.saveDatabases(mode, name).then((d) => (dbs = d)).catch((e) => ((error = t(...keyedError(e))), (dbs = [])));
    }
  });

  $effect(() => {
    if (part === "players" && !asked.players) {
      asked.players = true;
      api.savePlayers(mode, name).then((p) => (players = p)).catch((e) => ((error = t(...keyedError(e))), (players = [])));
    }
  });

  onMount(() => {
    load();
    if (mods) thumb("");
    return () => {
      closed = true;
      Object.values(thumbs).forEach((u) => u && URL.revokeObjectURL(u));
    };
  });
</script>

<div class="tools">
  {#if thumbs[""]}<img class="shot" src={thumbs[""]} alt={t("saves.thumbAlt", { name })} />{/if}
  {#if view?.broken.cells.length || view?.broken.other.length}
    <div class="warn" role="alert">
      <Icon name="alert" size={14} />
      <span>
        {#if view.broken.cells.length}{t("saves.brokenCells", { count: view.broken.cells.length })}{/if}
        {#if view.broken.other.length}{t("saves.brokenOther", { count: view.broken.other.length })} <span class="mono">{new Intl.ListFormat(intlLang(), { type: "unit" }).format(view.broken.other.slice(0, 5))}</span>{/if}
      </span>
      {#if view.broken.cells.length}<button onclick={repair} disabled={working}><Icon name="wand" size={14} />{t("saves.repair")}</button>{/if}
    </div>
  {/if}

  {#if PARTS.length > 1}
    <div class="subtabs" role="tablist" aria-label={t("saves.toolsLabel")} tabindex="-1" onkeydown={(e) => arrowPick(e, [...PARTS], part, (v) => (part = v as typeof part))}>
      {#each PARTS as id}
        <button role="tab" id="{uid}-tab-{id}" aria-controls="{uid}-panel" aria-selected={part === id} tabindex={part === id ? 0 : -1} class:active={part === id} onclick={() => ((part = id), (done = ""))}>{t(`saves.part.${id}`)}</button>
      {/each}
    </div>
  {/if}

  <div id="{uid}-panel" role={PARTS.length > 1 ? "tabpanel" : undefined} aria-labelledby={PARTS.length > 1 ? `${uid}-tab-${part}` : undefined}>
  {#if part === "backups"}
    <form class="row" onsubmit={(e) => (e.preventDefault(), backUp())}>
      <input type="text" bind:value={slot} maxlength="60" placeholder={t("saves.slotName")} aria-label={t("saves.slotName")} />
      <button class="primary" type="submit" disabled={working}><Icon name="download" size={14} />{slot.trim() ? t("saves.saveSlot") : t("saves.backUpNow")}</button>
    </form>
    <p class="hint">{t("saves.backupHint")} {t("saves.backupWhilePlaying")}</p>
    {#if view && !view.backups.length}
      <p class="hint">{t("saves.noBackups")}</p>
    {:else if view}
      <ul class="list" aria-label={t("saves.backupsLabel")}>
        {#each view.backups as b (b.file)}
          <li>
            <span>
              {#if thumbs[b.file]}<img class="mini" src={thumbs[b.file]} alt="" />{/if}
              {#if b.reason === "slot"}<Icon name="star" size={12} />{/if}
              <strong>{fmtDateTime(b.time)}</strong> <span class="hint">{fmtSize(b.size)} · {label(b)}</span>
            </span>
            <span class="btns">
              <button onclick={() => restore(b)} disabled={working}><Icon name="rollback" size={14} />{t("saves.restore")}</button>
              <button onclick={() => (naming = naming?.file === b.file ? null : { file: b.file, value: b.reason === "slot" ? b.label : "" })} disabled={working} aria-expanded={naming?.file === b.file}><Icon name="star" size={14} />{t("saves.name")}</button>
              <button class="quiet" onclick={() => remove(b)} disabled={working} title={t("common.delete")} aria-label={t("common.delete")}><Icon name="trash" size={14} /></button>
            </span>
            {#if naming?.file === b.file}
              <form class="row naming" onsubmit={(e) => (e.preventDefault(), nameBackup(b, false))}>
                <input type="text" bind:value={naming.value} maxlength="60" placeholder={t("saves.slotName")} aria-label={t("saves.slotName")} />
                <button type="submit" disabled={working || !naming.value.trim()}>{t(b.reason === "slot" ? "saves.rename" : "saves.keepAs")}</button>
                <button type="button" onclick={() => nameBackup(b, true)} disabled={working || !naming.value.trim()}><Icon name="copy" size={14} />{t("saves.copyAs")}</button>
                <button type="button" class="quiet" onclick={() => (naming = null)}>{t("common.cancel")}</button>
              </form>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {:else if part === "mods" && mods}
    <p class="hint">{t("saves.modsHint", { profile })}</p>
    <span class="btns">
      <button onclick={giveMods} disabled={working}><Icon name="upload" size={14} />{t("saves.giveMods")}</button>
      <button onclick={cloneWithMods} disabled={working}><Icon name="copy" size={14} />{t("saves.clone")}</button>
    </span>
    <p class="hint">{t("saves.cloneHint")}</p>
  {:else if part === "recovery" && mods}
    <p class="hint">{t("saves.reset.hint")}</p>
    <ul class="list">
      {#each RESETS as what}
        <li>
          <span><strong>{t(`saves.reset.${what}.label`)}</strong> <span class="hint">{t(`saves.reset.${what}.short`)}</span></span>
          <button onclick={() => reset(what)} disabled={working || !view?.resets[what]} title={view && !view.resets[what] ? t("saves.reset.nothing") : undefined}><Icon name="wand" size={14} />{t("saves.reset.do")}</button>
        </li>
      {/each}
    </ul>
    <p class="hint">{t("saves.reset.noHeal")}</p>
  {:else if part === "sandbox"}
    <p class="hint">{t("saves.sandboxHint")}</p>
    <ServerSettings save={{ mode, name }} />
  {:else if part === "players"}
    {#if !players}
      <p class="hint">{t("saves.reading")}</p>
    {:else if players.length}
      <table>
        <thead>
          <tr><th>{t("saves.players.name")}</th><th>{t("saves.players.where")}</th><th>{t("saves.players.state")}</th><th>{t("saves.players.data")}</th></tr>
        </thead>
        <tbody>
          {#each players as p (p.id + p.username)}
            <tr>
              <td>{p.name}{#if p.username} <span class="hint">({p.username})</span>{/if}</td>
              <!-- i18n-ignore: map coordinates -->
              <td class="mono">{Math.floor(p.x)}, {Math.floor(p.y)}, {Math.floor(p.z)}</td>
              <td>{p.dead ? t("saves.players.dead") : t("saves.players.alive")}</td>
              <td>{fmtSize(p.data_size)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="hint">{t("saves.players.hint")}</p>
    {:else if !error}
      <p class="hint">{t("saves.players.none")}</p>
    {/if}
  {:else if part === "data"}
    <p class="hint">{t("saves.data.hint")}</p>
    {#if !dbs}
      <p class="hint">{t("saves.reading")}</p>
    {:else if dbs.length}
      <select bind:value={picked} onchange={() => showTable(0)} aria-label={t("saves.data.pick")}>
        <option value="" disabled>{t("saves.data.pick")}</option>
        {#each dbs as [file, tables]}
          <optgroup label={file}>
            {#each tables as [table, count]}<option value={`${file}\n${table}`}>{t("saves.data.table", { table, count })}</option>{/each}
          </optgroup>
        {/each}
      </select>
    {:else if !error}
      <p class="hint">{t("saves.data.none")}</p>
    {/if}
    {#if rows}
      <div class="scroll">
        <table>
          <thead><tr>{#each rows.columns as c}<th class="mono">{c}</th>{/each}</tr></thead>
          <tbody>
            {#each rows.rows as r}<tr>{#each r as v}<td class="mono">{cell(v)}</td>{/each}</tr>{/each}
          </tbody>
        </table>
      </div>
      <div class="row">
        <button onclick={() => showTable(Math.max(0, offset - PAGE))} disabled={offset === 0}>{t("saves.data.prev")}</button>
        <span class="hint">{t("saves.data.range", { from: fmtNum(rows.rows.length ? offset + 1 : 0), to: fmtNum(offset + rows.rows.length), total: fmtNum(pickedCount) })}</span>
        <button onclick={() => showTable(offset + PAGE)} disabled={offset + PAGE >= pickedCount}>{t("saves.data.next")}</button>
      </div>
    {/if}
  {/if}
  </div>
  {#if done}<p class="hint" role="status">{done}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .tools {
    padding: 8px 0 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 8px 0 4px;
  }
  .warn {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 10px;
    color: var(--danger);
    overflow-wrap: anywhere;
  }
  .list {
    list-style: none;
    padding: 0;
    margin: 8px 0 0;
    border-top: 1px solid var(--line);
  }
  .shot {
    float: right;
    width: 160px;
    margin: 0 0 8px 12px;
    border-radius: 6px;
    border: 1px solid var(--line);
  }
  .mini {
    height: 28px;
    border-radius: 3px;
    vertical-align: middle;
    margin-right: 6px;
  }
  .naming {
    flex-basis: 100%;
  }
  .list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 8px;
    margin: 0 -8px;
    border-radius: 4px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .list li:hover {
    background: var(--hover);
    color: var(--on-selected);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .list li:hover .hint {
    color: color-mix(in oklab, var(--on-selected) 72%, transparent);
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 8px;
  }
  th,
  td {
    text-align: left;
    padding: 4px 8px 4px 0;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  th {
    color: var(--muted);
    font-weight: 700;
  }
  .mono {
    overflow-wrap: anywhere;
  }
  /* the mono font has no bold: a column name in it stays regular rather than smeared */
  th.mono {
    font-weight: 400;
  }
  .scroll {
    max-height: 360px;
    overflow: auto;
  }
  .scroll td {
    max-width: 240px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
