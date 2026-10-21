<script lang="ts">
  import { track } from "./activity.svelte";
  import { onMount, tick } from "svelte";
  import { api, type LineProblem, type ModRow, type ModSide, type Preset, type Row, type ServerCard, type ServerLines, type TagView } from "./api";
  import { fmtNum, modChanges, workshopUrl } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import MapOrder from "./MapOrder.svelte";
  import LeaveOut from "./LeaveOut.svelte";
  import { autosort, orderOf, sortLoadOrder } from "./order";

  let { server, profiles, profile, rows, ask, dirty = $bindable(false), saveAll = $bindable() }: {
    server: ServerCard;
    /** The lines differ from the ini's (out). */
    dirty?: boolean;
    /** Save what's changed (out, for the leave box's Save and leave): true only when it was saved. */
    saveAll?: () => Promise<boolean>;
    profiles: string[];
    profile: string;
    /** The library: installed mods with their load rules, for Autosort and names. */
    rows: Row[];
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
  } = $props();

  let modsText = $state("");
  let itemsText = $state("");
  let maps = $state<string[]>([]);
  let fromProfile = $state("");
  let collection = $state("");
  let loading = $state(false);
  /** Mod Manager: Server's presets (saved_modlists_server.txt), shared with that mod. */
  let presets = $state<Preset[]>([]);
  let presetPick = $state(0);
  let notes = $state<string[]>([]);
  /** The last check, for the lines as they were then (null: not checked since the last edit). */
  let problems = $state<LineProblem[] | null>(null);
  /** What the checks found out about the entries: Workshop titles, each mod's [name, item]. */
  let titles = $state<Record<string, string>>({});
  let modInfo = $state<Record<string, [string, string]>>({});
  /** Workshop items listed items require that the list lacks: [needed by, item]. */
  let required = $state<[string, string][]>([]);
  let checking = $state(false);
  let error = $state("");
  let done = $state("");
  /** Edit the lines as text (the ini's own form) instead of the list. */
  let raw = $state(false);
  let search = $state("");
  let from = $state<number | null>(null);
  let at = $state<number | null>(null);
  /** The PZMM list in the description: on for this server, and how it fits. */
  let tagOn = $state(false);
  let tag = $state<TagView | null>(null);
  let copied = $state(false);
  /** Mods only the server needs, or players may leave out (see `sides` on the server). */
  // svelte-ignore state_referenced_locally
  let sides = $state<Record<string, ModSide>>({ ...server.sides });

  async function setSide(id: string, side: string) {
    error = "";
    const next = { ...sides };
    if (side === "server" || side === "optional") next[id] = side;
    else delete next[id];
    try {
      await api.hostSidesSet(server.name, next);
      sides = next;
      fit();
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  const list = (s: string) => s.split(/[\n;]/).map((l) => l.trim().replace(/^\\/, "")).filter(Boolean);
  const lines = $derived<ServerLines>({ mods: list(modsText), workshop: list(itemsText), maps });
  const build = $derived(server.build === "legacy41" ? 41 : 42);
  const errors = $derived((problems ?? []).filter((p) => p.error));
  const installed = $derived(new Map(rows.filter((r): r is ModRow => r.kind === "mod").map((r) => [r.id, r])));
  const nameOf = (id: string) => modInfo[id]?.[0] || installed.get(id)?.name || "";
  const itemOf = (id: string) => modInfo[id]?.[1] || "";
  const query = $derived(search.trim().toLowerCase());
  const hit = (...parts: string[]) => !query || parts.some((p) => p.toLowerCase().includes(query));
  const shownMods = $derived(lines.mods.map((id, i) => ({ id, i })).filter(({ id }) => hit(id, nameOf(id), itemOf(id), titles[itemOf(id)] ?? "")));
  const shownItems = $derived(lines.workshop.filter((w) => hit(w, titles[w] ?? "")));
  /** Each entry's problems, by mod ID or Workshop ID. */
  const flagged = $derived.by(() => {
    const m = new Map<string, LineProblem[]>();
    for (const p of problems ?? []) m.set(p.id, [...(m.get(p.id) ?? []), p]);
    return m;
  });
  const missing = $derived([...new Set(required.map((r) => r[1]))].filter((w) => !lines.workshop.includes(w)));
  /** The lines as the ini has them ("": not read). */
  let saved = $state("");
  $effect(() => {
    dirty = !!saved && JSON.stringify(lines) !== saved;
  });

  function show(l: ServerLines) {
    modsText = l.mods.join("\n");
    itemsText = l.workshop.join("\n");
    maps = l.maps;
    problems = null;
  }

  function setMods(next: string[]) {
    modsText = next.join("\n");
    problems = null;
  }

  async function load() {
    try {
      show(await api.hostLines(server.name));
      saved = JSON.stringify(lines);
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function useProfile() {
    error = done = "";
    try {
      const [l, n] = await api.hostLinesFromProfile(fromProfile, build, server.name);
      show(l.maps.length ? l : { ...l, maps });
      notes = n;
      check();
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  /** Pasted Workshop collections or items (several at once): `add` puts them (and what they need) in the list, else they replace it. */
  async function useCollection(add: boolean) {
    error = done = "";
    loading = true;
    try {
      const [l, n] = add ? await api.hostLinesWith(lines, collection, build, false) : await api.hostLinesFromCollection(collection.trim(), build);
      show(l.maps.length ? l : { ...l, maps });
      notes = n;
      if (add) collection = "";
      check();
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      loading = false;
    }
  }

  /** The required Workshop items the list lacks, their mods first (they're what the others need). */
  async function addRequired() {
    error = done = "";
    loading = true;
    try {
      const [l, n] = await api.hostLinesWith(lines, missing.join(" "), build, true);
      show(l);
      notes = n;
      check();
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      loading = false;
    }
  }

  function usePreset() {
    const p = presets[presetPick];
    if (p) show({ mods: p.mods, workshop: p.workshop, maps });
    notes = [];
  }

  /** Save these lines as a Mod Manager: Server preset named after the server. */
  async function savePreset() {
    error = done = "";
    try {
      presets = (await api.presetSave({ from: "server", name: server.name, mods: lines.mods, workshop: lines.workshop })).presets.filter((p) => p.from === "server");
      done = t("hosting.mods.presetSaved", { name: server.name });
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  /** The lines as the ini has them (B42 puts `\` before each mod ID), for a host's panel. */
  async function copyLines() {
    const prefix = build === 42 ? "\\" : "";
    const text = [`Mods=${lines.mods.map((m) => prefix + m).join(";")}`, `WorkshopItems=${lines.workshop.join(";")}`, ...(lines.maps.length ? [`Map=${lines.maps.join(";")}`] : [])].join("\n");
    await navigator.clipboard.writeText(text).catch(() => {});
    done = t("hosting.mods.copied");
  }

  async function check() {
    checking = true;
    error = "";
    try {
      const r = await api.hostCheck(lines, server.name);
      problems = r.problems ?? [];
      titles = { ...titles, ...r.titles };
      modInfo = { ...modInfo, ...r.mods };
      required = r.required ?? [];
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      checking = false;
    }
    return problems;
  }

  /** Names and IDs as a list in the player's language ("A, B and C"). */
  const listOf = (xs: string[]) => new Intl.ListFormat(intlLang(), { type: "conjunction" }).format(xs);
  const say = (p: LineProblem) => t(`hosting.check.${p.key}`, { id: p.id, item: p.item });

  // --- the list ---

  /** The mod at `a` dropped before position `b`. */
  function move(a: number, b: number) {
    if (b === a || b === a + 1 || b < 0 || b > lines.mods.length) return;
    const next = [...lines.mods];
    const [m] = next.splice(a, 1);
    next.splice(b > a ? b - 1 : b, 0, m);
    setMods(next);
  }

  /** Alt+arrows move the focused mod (and keep it focused). */
  async function moveKey(e: KeyboardEvent, i: number) {
    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown") || query) return;
    e.preventDefault();
    const li = e.currentTarget as HTMLElement;
    move(i, e.key === "ArrowUp" ? i - 1 : i + 2);
    await tick();
    li.focus();
  }

  function drop(to: number) {
    if (from !== null) move(from, to);
    from = at = null;
  }

  const removeMod = (id: string) => setMods(lines.mods.filter((m) => m !== id));

  /** A Workshop item goes with the mods it gave the list. */
  function removeItem(w: string) {
    itemsText = lines.workshop.filter((x) => x !== w).join("\n");
    setMods(lines.mods.filter((m) => itemOf(m) !== w));
  }

  /** Autosort, as the load order has it: frameworks first, translations last, every requirement and load rule
   * of the installed mods; mods not installed here keep their place. */
  async function sortMods() {
    const stub = (id: string): ModRow => ({ kind: "mod", id, name: "", source: "", on: true, missing: true, requires: [], incompatible: [], load_after: [], load_before: [], load_first: "", load_last: "", pinned: false, author: "", only41: false, only42: false, languages: [] });
    const rowsNow: ModRow[] = lines.mods.map((id) => ({ ...(installed.get(id) ?? stub(id)), on: true }));
    try {
      const c = await api.categories();
      setMods(orderOf(autosort(rowsNow, c.of, c.order)));
    } catch {
      setMods(orderOf(sortLoadOrder(rowsNow))); // no tags (offline): the load rules still hold
    }
    done = t("hosting.mods.sorted");
  }

  /** Check, preview, then write the lines: true only when they were written (not on a problem or a cancelled preview). */
  async function write() {
    error = done = "";
    const found = await check();
    if (!found || found.some((p) => p.error)) return false;
    try {
      const p = await api.hostPreview(server.name, lines);
      const changes = [
        ...modChanges(p.added, p.removed),
        ...(p.items_added.length ? [t("hosting.mods.itemsAdded", { count: p.items_added.length, ids: listOf(p.items_added) })] : []),
        ...(p.items_removed.length ? [t("hosting.mods.itemsRemoved", { count: p.items_removed.length, ids: listOf(p.items_removed) })] : []),
        ...(p.maps_from.join(";") !== p.maps_to.join(";") ? [t("preview.map", { from: p.maps_from.join("; ") || "—", to: p.maps_to.join("; ") })] : []),
        ...found.map(say),
      ];
      if ((await ask(t("hosting.mods.writeTitle", { name: server.name }), changes, [{ label: t("hosting.mods.saveConfirm"), icon: "upload" }])) !== 0) return false;
      await api.hostWrite(server.name, lines);
      saved = JSON.stringify(lines);
      done = server.status === "running" ? t("hosting.mods.writtenRestart") : t("hosting.mods.written");
      fit(server.managed && tagOn);
      // asked before this returns: Save and leave leaves after it, so the next section reads the regions it adds
      if (p.maps_from.join(";") !== p.maps_to.join(";")) await offerSpawns();
      return true;
    } catch (e) {
      error = t(...keyedError(e));
      return false;
    }
  }
  saveAll = write;

  /** New map mods with spawn points: offer to add their regions to the spawn list. */
  async function offerSpawns() {
    const c = await api.hostConfig(server.name).catch(() => null);
    if (!c?.spawn_offers.length) return;
    const names = listOf(c.spawn_offers.map((o) => o.name));
    if ((await ask(t("hosting.mods.spawnTitle"), [t("hosting.mods.spawnLine", { names })], [{ label: t("hosting.mods.spawnAdd"), icon: "plus" }])) !== 0) return;
    try {
      await api.hostSpawnWrite(server.name, [...c.regions, ...c.spawn_offers]);
      done = t("hosting.mods.spawnAdded", { count: c.spawn_offers.length });
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  /** How the list fits the description now; `write` puts it in the config. */
  async function fit(write = false) {
    try {
      tag = await api.hostTag(server.name, lines, write);
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function setTag(on: boolean) {
    error = "";
    try {
      await api.hostTagSet(server.name, on);
      tagOn = on;
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function copyTag() {
    copied = false;
    await fit();
    if (!tag?.tag) return;
    await navigator.clipboard.writeText(tag.tag).catch(() => {});
    copied = true;
  }

  onMount(async () => {
    fromProfile = profile;
    tagOn = server.tag;
    await load();
    if (lines.workshop.length) {
      fit();
      check(); // titles and names for the list
    }
    api.lists().then((l) => (presets = l.presets.filter((p) => p.from === "server"))).catch(() => {});
  });

  // the activity bar moves while this tab works
  $effect(() => {
    if (loading) return track();
  });
</script>

<div class="mods">
  {#if server.managed}
    <div class="row">
      <label for="sm-profile">{t("hosting.mods.fromProfile")}</label>
      <select id="sm-profile" bind:value={fromProfile}>
        {#each profiles as p}<option value={p}>{p}</option>{/each}
      </select>
      <button onclick={useProfile}><Icon name="download" size={14} />{t("hosting.mods.useProfile")}</button>
    </div>
    <LeaveOut server={server.name} {rows} />
    <div class="row">
      <label for="sm-collection">{t("hosting.mods.fromCollection")}</label>
      <textarea id="sm-collection" class="links" rows="1" bind:value={collection} placeholder={t("hosting.mods.collectionPlaceholder")} spellcheck="false"></textarea>
      <button onclick={() => useCollection(false)} disabled={loading || !collection.trim()} title={t("hosting.mods.replaceTip")}><Icon name="download" size={14} />{loading ? t("hosting.wizard.loading") : t("hosting.wizard.load")}</button>
      <button onclick={() => useCollection(true)} disabled={loading || !collection.trim()} title={t("hosting.mods.addTip")}><Icon name="plus" size={14} />{t("hosting.mods.add")}</button>
    </div>
    {#if presets.length}
      <div class="row">
        <label for="sm-preset">{t("hosting.mods.fromPreset")}</label>
        <select id="sm-preset" bind:value={presetPick}>
          {#each presets as p, i}<option value={i}>{p.name}</option>{/each}
        </select>
        <button onclick={usePreset}><Icon name="download" size={14} />{t("hosting.mods.useProfile")}</button>
      </div>
    {/if}
    <p class="hint">{t("hosting.mods.hint")}</p>
  {:else}
    <p class="hint">{t("hosting.mods.readOnly")}</p>
  {/if}
  {#if notes.length}<ul class="hint notes">{#each notes as n}<li>{n}</li>{/each}</ul>{/if}
  {#if missing.length && server.managed}
    <div class="needs" role="status">
      <Icon name="alert" size={14} />
      <span>{t("hosting.mods.requiredMissing", { count: missing.length, items: listOf(missing.map((w) => titles[w] || w)) })}</span>
      <button onclick={addRequired} disabled={loading}><Icon name="plus" size={14} />{t("hosting.mods.addRequired", { count: missing.length })}</button>
    </div>
  {/if}

  <div class="tools">
    {#if !raw}
      <input type="search" bind:value={search} placeholder={t("hosting.mods.search")} aria-label={t("hosting.mods.search")} />
      {#if server.managed}<button onclick={sortMods} disabled={lines.mods.length < 2} title={t("hosting.mods.autosortTip")}><Icon name="wand" size={14} />{t("hosting.mods.autosort")}</button>{/if}
    {/if}
    <span class="grow"></span>
    <label class="check"><input type="checkbox" bind:checked={raw} />{t("hosting.mods.raw")}</label>
  </div>
  {#if raw}
    <div class="cols">
      <label>
        <!-- i18n-ignore: the ini key -->
        <span class="mono">Mods=</span> <span class="hint">{t("hosting.mods.modsHint", { count: lines.mods.length })}</span>
        <textarea class="mono" bind:value={modsText} oninput={() => (problems = null)} readonly={!server.managed} spellcheck="false" rows="12"></textarea>
      </label>
      <label>
        <!-- i18n-ignore: the ini key -->
        <span class="mono">WorkshopItems=</span> <span class="hint">{t("hosting.mods.itemsHint", { count: lines.workshop.length })}</span>
        <textarea class="mono" bind:value={itemsText} oninput={() => (problems = null)} readonly={!server.managed} spellcheck="false" rows="12"></textarea>
      </label>
    </div>
  {:else}
    <!-- i18n-ignore: the ini key -->
    <h3><span class="mono">Mods=</span> <span class="hint">{t("hosting.mods.modsHint", { count: lines.mods.length })}{query ? ` · ${t("hosting.mods.shown", { count: shownMods.length })}` : ""}</span></h3>
    {#if query && server.managed}<p class="hint">{t("hosting.mods.noDragFiltered")}</p>{/if}
    <ol class="drag-list entries" role="listbox" aria-label={t("hosting.mods.modsLabel")}>
      {#each shownMods as { id, i } (id)}
        {@const item = itemOf(id)}
        {@const bad = flagged.get(id) ?? []}
        <li role="option" aria-selected="false" class:drop={at === i} class:dragging={from === i} draggable={server.managed && !query} tabindex="0"
          onkeydown={(e) => server.managed && moveKey(e, i)}
          ondragstart={() => (from = i)} ondragend={() => { from = at = null; }}
          ondragover={(e) => { e.preventDefault(); at = i; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}>
          <span class="num mono">{fmtNum(i + 1)}</span>
          <span class="what">
            <strong>{nameOf(id) || id}</strong>{#if nameOf(id)} <span class="mono id">{id}</span>{/if}
            {#if item}<span class="item">{titles[item] || item}</span>{/if}
          </span>
          {#if bad.length}<span class="flag" class:bad={bad.some((p) => p.error)} title={bad.map(say).join("\n")}><Icon name={bad.some((p) => p.error) ? "error" : "alert"} size={14} /></span>{/if}
          {#if server.managed}
            <select class="side" value={sides[id] ?? "both"} onchange={(e) => setSide(id, e.currentTarget.value)} title={t("hosting.mods.sideTip")} aria-label={t("hosting.mods.sideLabel", { name: nameOf(id) || id })}>
              {#each ["both", "server", "optional"] as s}<option value={s}>{t(`hosting.mods.side.${s}`)}</option>{/each}
            </select>
          {/if}
          {#if server.managed}<button class="quiet icon" onclick={() => removeMod(id)} title={t("hosting.mods.remove")} aria-label={t("hosting.mods.removeLabel", { name: nameOf(id) || id })}><Icon name="x" size={14} /></button>{/if}
        </li>
      {/each}
      {#if server.managed && !query}
        <li class="tail" class:drop={at === lines.mods.length} aria-hidden="true"
          ondragover={(e) => { e.preventDefault(); at = lines.mods.length; }}
          ondrop={(e) => { e.preventDefault(); drop(lines.mods.length); }}></li>
      {/if}
    </ol>
    <!-- i18n-ignore: the ini key -->
    <h3><span class="mono">WorkshopItems=</span> <span class="hint">{t("hosting.mods.itemsHint", { count: lines.workshop.length })}</span></h3>
    <ul class="drag-list entries items" aria-label={t("hosting.mods.itemsLabel")}>
      {#each shownItems as w (w)}
        {@const bad = flagged.get(w)?.filter((p) => p.key === "badId" || p.key === "notOnWorkshop" || p.key === "requiredItem") ?? []}
        {@const count = lines.mods.filter((m) => itemOf(m) === w).length}
        <li>
          <span class="what">
            <strong>{titles[w] || w}</strong>{#if titles[w]} <span class="mono id">{w}</span>{/if}
            {#if count}<span class="item">{t("hosting.mods.itemMods", { count })}</span>{/if}
          </span>
          {#if bad.length}<span class="flag" class:bad={bad.some((p) => p.error)} title={bad.map(say).join("\n")}><Icon name={bad.some((p) => p.error) ? "error" : "alert"} size={14} /></span>{/if}
          <button class="quiet icon" onclick={() => api.openUrl(workshopUrl(w))} title={t("panel.workshopPage")} aria-label={t("panel.workshopPage")}><Icon name="steam" size={14} /></button>
          {#if server.managed}<button class="quiet icon" onclick={() => removeItem(w)} title={t("hosting.mods.removeItemTip")} aria-label={t("hosting.mods.removeLabel", { name: titles[w] || w })}><Icon name="x" size={14} /></button>{/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if maps.length}
    <!-- i18n-ignore: the ini key -->
    <h3 class="mono">Map=</h3>
    {#if server.managed}<MapOrder bind:maps />{:else}<p class="mono">{maps.join(";")}</p>{/if}
  {/if}
  {#if problems}
    {#if problems.length}
      <ul class="problems">
        {#each problems as p}<li class:error={p.error}><Icon name={p.error ? "error" : "alert"} size={14} />{say(p)}</li>{/each}
      </ul>
    {:else}
      <p class="hint" role="status"><Icon name="check" size={14} /> {t("hosting.mods.allGood")}</p>
    {/if}
  {/if}
  {#if server.managed}
    <div class="row">
      {#if !dirty}<button class="primary" onclick={write} disabled={checking}><Icon name="upload" size={14} />{t("hosting.mods.write")}</button>{/if}
      <button onclick={check} disabled={checking}><Icon name="search" size={14} />{checking ? t("hosting.mods.checking") : t("hosting.mods.check")}</button>
      <button class="quiet" onclick={load}><Icon name="rollback" size={14} />{t("hosting.mods.reload")}</button>
      <button class="quiet" onclick={savePreset} title={t("hosting.mods.savePresetTip")}><Icon name="upload" size={14} />{t("hosting.mods.savePreset")}</button>
    </div>
  {/if}
  <div class="row">
    <button class="quiet" onclick={copyLines} title={t("hosting.mods.copyTip")}><Icon name="copy" size={14} />{t("hosting.mods.copy")}</button>
  </div>
  {#if errors.length}<p class="error">{t("hosting.mods.blocked", { count: errors.length })}</p>{/if}
  <section class="tag" aria-label={t("hosting.tag.title")}>
    <h3>{t("hosting.tag.title")}</h3>
    {#if server.managed}
      <label class="check"><input type="checkbox" checked={tagOn} onchange={(e) => setTag(e.currentTarget.checked)} />{t("hosting.tag.on")}</label>
    {/if}
    <p class="hint">{t("hosting.tag.hint")}</p>
    {#if tag}
      <p class={tag.form === "full" ? "hint" : "warn"} role="status">
        {#if tag.form !== "full"}<Icon name="alert" size={14} />{/if}{t(`hosting.tag.${tag.form}`, { length: fmtNum(tag.length), limit: fmtNum(tag.limit) })}
      </p>
    {/if}
    <div class="row">
      <button onclick={copyTag} disabled={!lines.workshop.length}><Icon name="copy" size={14} />{t("hosting.tag.copy")}</button>
    </div>
    {#if copied}<p class="hint" role="status">{t("hosting.tag.copied")}</p>{/if}
  </section>
  {#if done}<p class="hint" role="status">{done}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
  {#if server.managed && dirty}
    <div class="row bar">
      <button class="primary" onclick={write} disabled={checking}><Icon name="upload" size={14} />{t("hosting.mods.write")}</button>
      <button onclick={load} disabled={checking}><Icon name="undo" size={14} />{t("hosting.settings.discard")}</button>
      <span class="hint">{t("hosting.mods.unsavedHint")}</span>
    </div>
  {/if}
</div>

<style>
  .row,
  .tools {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    margin: 8px 0;
  }
  .row label {
    color: var(--muted);
  }
  /* unsaved changes: Save stays in reach at the bottom, as Settings' bar does */
  .bar {
    position: sticky;
    bottom: 0;
    padding: 8px 0;
    background: var(--bg);
  }
  .grow {
    flex: 1;
  }
  .links {
    flex: 1;
    min-width: 220px;
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .cols label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  textarea {
    resize: vertical;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--raised);
    color: inherit;
  }
  h3 {
    margin: 16px 0 4px;
  }
  /* the ini keys in the headings: Plex Mono ships in 400 only */
  h3 .mono,
  h3.mono {
    font-weight: 400;
  }
  .entries {
    max-height: 420px;
    overflow: auto;
  }
  .entries .num {
    width: 24px;
    text-align: right;
    color: var(--accent-text);
  }
  .entries li:hover .num {
    color: var(--on-selected);
  }
  .items li {
    cursor: default;
  }
  .what {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .id,
  .item {
    margin-left: 8px;
    color: var(--muted);
    font-size: 12px;
  }
  .entries li:hover .id,
  .entries li:hover .item {
    color: inherit;
  }
  .side {
    flex: none;
    font-size: 12px;
  }
  .flag {
    display: flex;
    color: var(--map-over);
  }
  .flag.bad {
    color: var(--danger);
  }
  .needs {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 8px 0;
    color: var(--map-over);
  }
  .needs span {
    flex: 1;
    min-width: 0;
    color: var(--text);
  }
  .problems {
    list-style: none;
    padding: 0;
    margin: 10px 0;
  }
  .problems li {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--muted);
  }
  .problems li.error {
    color: var(--danger);
  }
  .notes {
    padding-left: 18px;
  }
  .tag {
    margin-top: 18px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .warn {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    color: var(--danger);
  }
</style>
