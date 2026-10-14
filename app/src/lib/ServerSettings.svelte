<script lang="ts">
  import { track } from "./activity.svelte";
  import { onMount } from "svelte";
  import { confirmed } from "./ask.svelte";
  import { api, type ConfigView, type ServerCard, type Setting, type SpawnRegion } from "./api";
  import { fmtNum } from "./format";
  import { keyedError, t } from "./i18n.svelte";
  import { arrowPick } from "./ui";
  import Icon from "./Icon.svelte";

  let { server = null, save = null, dirty = $bindable(false) }: {
    server?: ServerCard | null;
    /** A save's sandbox settings (map_sand.bin) instead of a server's files. */
    save?: { mode: string; name: string } | null;
    /** Edits not saved yet, in any part (out). */
    dirty?: boolean;
  } = $props();

  const PARTS = ["ini", "sandbox", "spawn", "files"] as const;
  /** The files the text editor opens, after the server's name. */
  const FILES = [".ini", "_SandboxVars.lua", "_spawnregions.lua", "_spawnpoints.lua"];
  const SUFFIX = { ini: ".ini", sandbox: "_SandboxVars.lua" } as const;
  // svelte-ignore state_referenced_locally
  let part = $state<(typeof PARTS)[number]>(save ? "sandbox" : "ini");
  let view = $state<ConfigView | null>(null);
  /** Changed values by key, per file. */
  let edits = $state<{ ini: Record<string, string>; sandbox: Record<string, string> }>({ ini: {}, sandbox: {} });
  let filter = $state("");
  let regions = $state<SpawnRegion[]>([]);
  let newRegion = $state({ name: "", file: "" });
  let preset = $state("");
  let presetName = $state("");
  let error = $state("");
  let done = $state("");
  let working = $state(false);
  /** Sandbox settings shown: the game's own, or the ones mods add. */
  let origin = $state<"base" | "mods">("base");
  let file = $state(FILES[0]);
  let text = $state<string | null>(null);
  let textSaved = $state("");

  type Form = "ini" | "sandbox" | "spawn";
  /** Read the files again; `fresh`: the forms that start over from them (what was just written), so unsaved edits in the others stay. */
  async function load(fresh: Form[]) {
    try {
      view = save ? await api.saveSandbox(save.mode, save.name) : await api.hostConfig(server?.name ?? "");
      for (const f of fresh) {
        if (f === "spawn") regions = structuredClone($state.snapshot(view.regions));
        else edits[f] = {};
      }
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  const value = (file: "ini" | "sandbox", s: Setting) => edits[file][s.key] ?? s.value;
  function set(file: "ini" | "sandbox", s: Setting, v: string) {
    if (v === s.value) delete edits[file][s.key];
    else edits[file][s.key] = v;
  }
  const changed = (file: "ini" | "sandbox") => Object.keys(edits[file]).length;
  const match = (s: Setting) => !filter.trim() || `${s.key} ${s.label} ${s.description} ${s.source}`.toLowerCase().includes(filter.trim().toLowerCase());
  const fromMods = $derived((view?.sandbox ?? []).filter((s) => s.source));
  const shownSandbox = $derived((view?.sandbox ?? []).filter((s) => !!s.source === (origin === "mods")));

  async function loadText() {
    text = null;
    try {
      text = textSaved = await api.hostText(server?.name ?? "", file);
    } catch (e) {
      error = t(...keyedError(e));
    }
  }
  /** Open another file in the text editor: what was typed is dropped only when the player says so. */
  async function pickText(e: Event & { currentTarget: HTMLSelectElement }) {
    const el = e.currentTarget;
    if (text !== null && text !== textSaved && !(await confirmed(t("hosting.leaveTitle"), { label: t("hosting.leave"), icon: "undo" }, [t("hosting.leaveLine")]))) return void (el.value = file);
    file = el.value;
    loadText();
  }

  const FORM_OF: Record<string, Form> = { ".ini": "ini", "_SandboxVars.lua": "sandbox", "_spawnregions.lua": "spawn" };
  const restorePrev = (suffix: string) =>
    run(async () => {
      await api.hostTextRestore(server?.name ?? "", suffix);
      if (part === "files") await loadText();
    }, t("hosting.settings.restored"), FORM_OF[suffix] ? [FORM_OF[suffix]] : []);

  /** Settings in their sections, in file order. */
  function sections(list: Setting[]) {
    const out: [string, Setting[]][] = [];
    for (const s of list.filter(match)) {
      const at = out.find(([id]) => id === s.section);
      if (at) at[1].push(s);
      else out.push([s.section, [s]]);
    }
    return out;
  }
  const sectionLabel = (file: "ini" | "sandbox", id: string) => (file === "ini" ? t(`hosting.settings.section.${id}`) : id || t("hosting.settings.general"));

  async function run(f: () => Promise<unknown>, ok: string, fresh: Form[] = []) {
    error = done = "";
    working = true;
    try {
      await f();
      done = ok;
      await load(fresh);
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      working = false;
    }
  }

  /** A server run elsewhere: send PZMM's copies to its host (the host's are backed up first). */
  async function send() {
    error = done = "";
    working = true;
    try {
      done = (await api.hostFtpPush(server?.name ?? "")) ? t("hosting.files.pushedBackup") : t("hosting.files.pushed");
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      working = false;
    }
  }

  const saved = () => (save ? t("saves.sandboxSaved") : server?.status === "running" ? t("hosting.settings.savedRestart") : t("hosting.settings.saved"));
  const write = (file: "ini" | "sandbox") =>
    run(
      () =>
        save
          ? api.saveSandboxWrite(save.mode, save.name, Object.entries(edits.sandbox))
          : (file === "ini" ? api.hostIniWrite : api.hostSandboxWrite)(server?.name ?? "", Object.entries(edits[file])),
      saved(),
      [file],
    );

  async function applyPreset() {
    error = done = "";
    const [kind, ...rest] = preset.split(":");
    try {
      const values = save ? await api.saveSandboxPreset(rest.join(":"), kind === "own") : await api.hostPreset(server?.name ?? "", rest.join(":"), kind === "own");
      let n = 0;
      for (const s of view?.sandbox ?? []) {
        if (s.key in values) {
          set("sandbox", s, values[s.key]);
          n++;
        }
      }
      done = t("hosting.settings.presetApplied", { count: n });
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  /** The form as it is now (edits kept: saving a preset writes nothing to the server). */
  async function savePreset() {
    if (!view) return;
    const name = presetName.trim();
    const values = Object.fromEntries(view.sandbox.filter((s) => !/^version$/i.test(s.key)).map((s) => [s.key, value("sandbox", s)]));
    error = done = "";
    try {
      await api.hostPresetSave(name, values);
      view.own_presets = [...new Set([...view.own_presets, name])].sort();
      done = t("hosting.settings.presetSaved", { name });
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  function addRegion() {
    regions.push({ name: newRegion.name.trim(), file: newRegion.file.trim(), server_file: !newRegion.file.includes("/") });
    newRegion = { name: "", file: "" };
  }
  const regionsChanged = $derived(JSON.stringify(regions) !== JSON.stringify(view?.regions ?? []));
  $effect(() => {
    dirty = !!(changed("ini") || changed("sandbox") || regionsChanged || (text !== null && text !== textSaved));
  });

  onMount(() => {
    load(["ini", "sandbox", "spawn"]);
  });

  // the activity bar moves while this tab works
  $effect(() => {
    if (working) return track();
  });
</script>

{#snippet field(file: "ini" | "sandbox", s: Setting)}
  {@const v = value(file, s)}
  <div class="field" class:edited={s.key in edits[file]}>
    <div class="label">
      <label class:mono={!s.label} for="cfg-{file}-{s.key}">{s.label || s.key}</label>
      {#if s.label}<span class="hint mono">{s.key}</span>{/if}
      {#if s.description}<span class="hint">{s.description}</span>{/if}
      {#if s.default !== null}<span class="hint">{t("hosting.settings.default", { value: s.default })}</span>{/if}
    </div>
    {#if s.kind === "bool"}
      <input id="cfg-{file}-{s.key}" type="checkbox" checked={v === "true"} onchange={(e) => set(file, s, String(e.currentTarget.checked))} />
    {:else if s.kind === "choice"}
      <select id="cfg-{file}-{s.key}" value={v} onchange={(e) => set(file, s, e.currentTarget.value)}>
        {#each s.choices as [cv, label]}<option value={cv}>{label}</option>{/each}
      </select>
    {:else if s.kind === "number"}
      <input id="cfg-{file}-{s.key}" type="number" step="any" min={s.min ?? undefined} max={s.max ?? undefined} value={v} oninput={(e) => set(file, s, e.currentTarget.value)} />
    {:else}
      <input id="cfg-{file}-{s.key}" type={s.kind === "secret" ? "password" : "text"} autocomplete="off" spellcheck="false" value={v} oninput={(e) => set(file, s, e.currentTarget.value)} />
    {/if}
  </div>
{/snippet}

{#snippet form(file: "ini" | "sandbox", list: Setting[])}
  {#each sections(list) as [id, items] (id)}
    <details open={!!filter.trim() || undefined}>
      <summary>{sectionLabel(file, id)} <span class="hint">{fmtNum(items.length)}</span></summary>
      {#each items as s (s.key)}{@render field(file, s)}{/each}
    </details>
  {/each}
  {#if changed(file)}
    <div class="row bar">
      <button class="primary" onclick={() => write(file)} disabled={working}><Icon name="check" size={14} />{t("hosting.settings.save", { count: changed(file) })}</button>
      <button onclick={() => (edits[file] = {})} disabled={working}><Icon name="undo" size={14} />{t("hosting.settings.discard")}</button>
      <span class="hint">{t("hosting.settings.backupNote")}</span>
    </div>
  {:else if !save && view?.prev.includes(SUFFIX[file])}
    <div class="row">
      <button onclick={() => restorePrev(SUFFIX[file])} disabled={working} title={t("hosting.settings.restoreTip")}><Icon name="rollback" size={14} />{t("hosting.settings.restore")}</button>
    </div>
  {/if}
{/snippet}

<div class="settings">
  {#if server?.remote}
    <!-- a server run elsewhere: these are PZMM's copies, got over FTP -->
    <div class="row">
      <p class="hint">{t("hosting.settings.remoteCopy")}</p>
      <button onclick={send} disabled={working}><Icon name="upload" size={14} />{t("hosting.files.push")}</button>
    </div>
  {/if}
  <div class="row">
    {#if !save}<div class="subtabs" role="tablist" aria-label={t("hosting.settings.label")} tabindex="-1" onkeydown={(e) => arrowPick(e, [...PARTS], part, (v) => (part = v as typeof part))}>
      {#each PARTS as id}
        <button role="tab" id="cfg-tab-{id}" aria-controls="cfg-panel" aria-selected={part === id} tabindex={part === id ? 0 : -1} class:active={part === id} onclick={() => ((part = id), (done = ""), id === "files" && (text === null || text === textSaved) && loadText())}>{t(`hosting.settings.part.${id}`)}</button>
      {/each}
    </div>{/if}
    {#if part === "ini" || part === "sandbox"}<input type="search" bind:value={filter} placeholder={t("hosting.settings.filter")} aria-label={t("hosting.settings.filter")} />{/if}
  </div>

  <div id="cfg-panel" role={save ? undefined : "tabpanel"} aria-labelledby={save ? undefined : `cfg-tab-${part}`}>
    {#if view}
      {#if part === "ini"}
        <p class="hint">{t("hosting.settings.iniHint")}</p>
        {@render form("ini", view.ini)}
      {:else if part === "sandbox"}
        {#if view.sandbox_error}
          <p class="hint">{t("hosting.settings.unreadable", { error: view.sandbox_error })}</p>
        {:else}
          <div class="row">
            <select bind:value={preset} aria-label={t("hosting.settings.preset")}>
              <option value="" disabled>{t("hosting.settings.preset")}</option>
              {#each view.presets as p}<option value="game:{p}">{p}</option>{/each}
              {#each view.own_presets as p}<option value="own:{p}">{t("hosting.settings.ownPreset", { name: p })}</option>{/each}
            </select>
            <button onclick={applyPreset} disabled={!preset}><Icon name="wand" size={14} />{t("hosting.settings.applyPreset")}</button>
            <input type="text" bind:value={presetName} placeholder={t("hosting.settings.presetName")} aria-label={t("hosting.settings.presetName")} maxlength="60" />
            <button onclick={savePreset} disabled={working || !presetName.trim()}><Icon name="copy" size={14} />{t("hosting.settings.savePreset")}</button>
          </div>
          <p class="hint">{t("hosting.settings.presetHint")} {t("hosting.settings.presetWhere")}</p>
          {#if fromMods.length}
            <div class="subtabs" role="tablist" aria-label={t("hosting.settings.origin")} tabindex="-1" onkeydown={(e) => arrowPick(e, ["base", "mods"], origin, (v) => (origin = v as typeof origin))}>
              <button role="tab" id="cfg-origin-base" aria-controls="cfg-origin" aria-selected={origin === "base"} tabindex={origin === "base" ? 0 : -1} class:active={origin === "base"} onclick={() => (origin = "base")}>{t("hosting.settings.base")}</button>
              <button role="tab" id="cfg-origin-mods" aria-controls="cfg-origin" aria-selected={origin === "mods"} tabindex={origin === "mods" ? 0 : -1} class:active={origin === "mods"} onclick={() => (origin = "mods")}>{t("hosting.settings.fromMods", { count: fromMods.length })}</button>
            </div>
          {/if}
          <div id="cfg-origin" role={fromMods.length ? "tabpanel" : undefined} aria-labelledby={fromMods.length ? `cfg-origin-${origin}` : undefined}>{@render form("sandbox", shownSandbox)}</div>
        {/if}
      {:else if part === "spawn"}
        {#if view.regions_error}
          <p class="hint">{t("hosting.settings.unreadable", { error: view.regions_error })}</p>
        {:else}
          <p class="hint">{t("hosting.settings.spawnHint")}</p>
          {#if view.spawn_offers.length}
            <div class="offers" role="region" aria-label={t("hosting.settings.offersLabel")}>
              <p class="hint">{t("hosting.settings.offers")}</p>
              {#each view.spawn_offers as o}
                <div class="row">
                  <strong>{o.name}</strong>
                  <span class="hint mono grow">{o.file}</span>
                  <button onclick={() => regions.push(o)} disabled={regions.some((r) => r.file === o.file)}><Icon name="plus" size={14} />{t("hosting.settings.addRegion")}</button>
                </div>
              {/each}
            </div>
          {/if}
          <ul class="regions">
            {#each regions as r, i}
              <li>
                <strong>{r.name}</strong>
                <span class="hint mono">{r.file}</span>
                <button class="quiet" onclick={() => regions.splice(i, 1)} title={t("common.remove")} aria-label={t("common.remove")}><Icon name="x" size={14} /></button>
              </li>
            {/each}
          </ul>
          <form class="row" onsubmit={(e) => (e.preventDefault(), addRegion())}>
            <input type="text" bind:value={newRegion.name} placeholder={t("hosting.settings.regionName")} aria-label={t("hosting.settings.regionName")} />
            <input type="text" class="grow" bind:value={newRegion.file} placeholder={t("hosting.settings.regionFile")} aria-label={t("hosting.settings.regionFile")} />
            <button type="submit" disabled={!newRegion.name.trim() || !newRegion.file.trim()}><Icon name="plus" size={14} />{t("hosting.settings.addRegion")}</button>
          </form>
          <div class="row bar">
            <button class="primary" onclick={() => run(() => api.hostSpawnWrite(server?.name ?? "", regions), saved(), ["spawn"])} disabled={working || !regionsChanged}><Icon name="check" size={14} />{t("hosting.settings.saveRegions")}</button>
            {#if view.prev.includes("_spawnregions.lua") && !regionsChanged}
              <button onclick={() => restorePrev("_spawnregions.lua")} disabled={working} title={t("hosting.settings.restoreTip")}><Icon name="rollback" size={14} />{t("hosting.settings.restore")}</button>
            {/if}
            <span class="hint">{t("hosting.settings.backupNote")}</span>
          </div>
        {/if}
      {:else}
        <div class="row">
          <select value={file} onchange={pickText} aria-label={t("hosting.settings.file")}>
            {#each FILES as f}<option value={f}>{server?.name}{f}</option>{/each}
          </select>
          <span class="hint">{t("hosting.settings.textHint")}</span>
        </div>
        {#if text !== null}
          <textarea class="mono text" bind:value={text} spellcheck="false" aria-label={t("hosting.settings.file")}></textarea>
          <div class="row bar">
            <button class="primary" onclick={() => run(async () => { await api.hostTextWrite(server?.name ?? "", file, text ?? ""); textSaved = text ?? ""; }, saved(), FORM_OF[file] ? [FORM_OF[file]] : [])} disabled={working || text === textSaved}><Icon name="check" size={14} />{t("hosting.settings.saveText")}</button>
            {#if text !== textSaved}<button onclick={() => (text = textSaved)} disabled={working}><Icon name="undo" size={14} />{t("hosting.settings.discard")}</button>{/if}
            {#if view.prev.includes(file)}
              <button onclick={() => restorePrev(file)} disabled={working || text !== textSaved} title={t("hosting.settings.restoreTip")}><Icon name="rollback" size={14} />{t("hosting.settings.restore")}</button>
            {/if}
            <span class="hint">{t("hosting.settings.backupNote")}</span>
          </div>
        {/if}
      {/if}
    {/if}
  </div>
  {#if done}<p class="hint" role="status">{done}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 6px 0;
  }
  .row input[type="search"] {
    flex: 1;
  }
  .bar {
    position: sticky;
    bottom: 0;
    padding: 8px 0;
    background: var(--bg);
  }
  details {
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  summary {
    cursor: pointer;
    padding: 6px 0;
    font-weight: 700;
  }
  .field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 6px 8px;
    margin: 0 -8px;
    border-radius: 4px;
  }
  .field.edited {
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }
  .field:hover {
    color: var(--on-selected);
    background: var(--hover);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  .label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .field input[type="text"],
  .field input[type="password"],
  .field select {
    width: 260px;
    flex: none;
  }
  .field input[type="number"] {
    width: 120px;
    flex: none;
  }
  .regions {
    list-style: none;
    padding: 0;
    margin: 6px 0;
    border-top: 1px solid var(--line);
  }
  .regions li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 0;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .regions .mono {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .grow {
    flex: 1;
  }
  .offers {
    margin: 6px 0;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--panel);
  }
  .text {
    width: 100%;
    min-height: 380px;
    box-sizing: border-box;
    font-size: 12px;
    line-height: 1.5;
    resize: vertical;
  }
</style>
