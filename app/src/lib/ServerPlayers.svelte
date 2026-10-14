<script lang="ts">
  import { track } from "./activity.svelte";
  import { onMount, tick, untrack } from "svelte";
  import { api, type Dossier, type People, type ServerCard, type WorldGrid } from "./api";
  import { fmtDateTime } from "./format";
  import type { IconName } from "./Icon.svelte";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import { prefs } from "./prefs.svelte";
  import { arrowPick } from "./ui";
  import Icon from "./Icon.svelte";
  import MapGrid from "./MapGrid.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import ServerCommands, { B42_ONLY, FORMS, q } from "./ServerCommands.svelte";

  let { server, ask, hidden = false }: {
    server: ServerCard;
    /** The Servers tab is behind another one: no reading meanwhile. */
    hidden?: boolean;
    ask: (title: string, lines: string[], choices: { label: string; icon: IconName }[]) => Promise<number>;
  } = $props();

  const LEVELS = ["admin", "moderator", "overseer", "gm", "observer", "none"];
  /** A broadcast's most lines: each is a message of its own. */
  const MAX_LINES = 10;
  let players = $state<string[] | null>(null);
  /** Accounts, bans and characters from the server's own files (not for one run elsewhere). */
  let people = $state<People | null>(null);
  let view = $state("online");
  let levels = $state<Record<string, string>>({});
  let message = $state("");
  let error = $state("");
  let done = $state("");
  let working = $state(false);
  /** A player's Commands menu (`map`: opened from the map: kick and message too), and the form picked from it. */
  let menu = $state<{ x: number; y: number; name: string; map?: boolean } | null>(null);
  let form = $state<{ command: string; player: string; at?: [number, number, number] } | null>(null);
  let newUser = $state("");
  let newPassword = $state("");
  let banId = $state("");
  let banIp = $state("");
  let messageBox = $state<HTMLTextAreaElement>();
  /** Notes, tags, joins and leaves PZMM keeps; the player whose note is open. */
  let dossier = $state<Dossier>({ players: {}, history: [] });
  let editing = $state<{ name: string; note: string; tags: string } | null>(null);
  let historyFilter = $state("");
  /** The world's cells for the map (read when it first opens). */
  let world = $state<WorldGrid | null>(null);
  let worldError = $state("");
  const up = $derived(server.status === "running");
  const local = $derived(!server.remote);
  const b42 = $derived(server.build !== "legacy41");
  const views = $derived(local ? ["online", "map", "characters", "history", "whitelist", "bans"] : ["online", "history"]);
  const forms = $derived(FORMS.filter((c) => b42 || !B42_ONLY.includes(c)));
  const lines = $derived(message.split("\n").map((l) => l.trim()).filter(Boolean));
  const users = $derived(people?.accounts?.users.filter((u) => !u.banned) ?? []);
  const banned = $derived(people?.accounts?.users.filter((u) => u.banned) ?? []);

  async function refresh() {
    if (!up) return;
    try {
      players = await api.hostPlayers(server.name);
      error = "";
    } catch (e) {
      error = t(...keyedError(e));
    }
  }

  async function loadPeople() {
    dossier = await api.hostDossier(server.name).catch(() => dossier);
    if (!local) return;
    people = await api.hostPeople(server.name).catch(() => null);
  }

  async function saveNote(e: Event) {
    e.preventDefault();
    if (!editing) return;
    error = "";
    try {
      dossier = await api.hostPlayerSet(server.name, editing.name, editing.note, editing.tags.split(","));
      editing = null;
    } catch (err) {
      error = t(...keyedError(err));
    }
  }

  function openNote(name: string) {
    const p = dossier.players[name];
    editing = editing?.name === name ? null : { name, note: p?.note ?? "", tags: p?.tags.join(", ") ?? "" };
  }

  /** Joins and leaves, newest first, for the name typed. */
  const history = $derived.by(() => {
    const f = historyFilter.trim().toLowerCase();
    return dossier.history.filter((h) => !f || h.name.toLowerCase().includes(f)).slice(-500).reverse();
  });

  $effect(() => {
    if (view === "map") openMap();
  });

  async function openMap() {
    if (world || worldError) return;
    world = await api.hostWorld(server.name).catch((e) => ((worldError = t(...keyedError(e))), null));
  }
  /** Characters on the map: online ones in the accent colour, the dead greyed. */
  const dots = $derived(
    world
      ? (people?.characters ?? []).filter((c) => c.username || c.name).map((c) => ({ x: c.x / world!.size, y: c.y / world!.size, colour: c.dead ? "--muted" : players?.includes(c.username) ? "--accent" : "--text", name: c.username || c.name }))
      : [],
  );
  const paintWorld = (c: { kind: string }): [string, number] => [c.kind === "water" ? "--map-water" : c.kind === "vanilla" ? "--muted" : c.kind === "conflict" ? "--danger" : "--map-cell", 0.3];

  /** Message everyone about a player: the broadcast box, their name first. */
  async function messageAbout(name: string) {
    view = "online";
    message = `${name}: ${message}`;
    await tick();
    messageBox?.focus();
  }

  function playerMenu(name: string, x: number, y: number, map = false) {
    menu = { x, y, name, map };
  }

  const menuItems = (name: string, map: boolean): MenuItem[] => [
    ...forms.map((c) => ({ label: t(`hosting.cmd.${c}`), icon: "play" as const, run: () => (form = { command: c, player: name }), disabled: !up })),
    null,
    { label: t("hosting.players.mute"), icon: "lock", run: () => run(`voiceban ${q(name)} -true`, t("hosting.players.muted", { name })), disabled: !up },
    { label: t("hosting.players.unmute"), icon: "undo", run: () => run(`voiceban ${q(name)} -false`, t("hosting.players.unmuted", { name })), disabled: !up },
    null,
    ...(map
      ? [
          { label: t("hosting.players.messageAbout"), icon: "message" as const, run: () => messageAbout(name), disabled: !up },
          { label: t("hosting.players.notes"), icon: "info" as const, run: () => openNote(name) },
        ]
      : []),
    { label: t("hosting.players.kick"), icon: "x", run: () => kick(name), disabled: !up || working },
    { label: t("hosting.players.ban"), icon: "lock", run: () => ban(name), disabled: !up || working, danger: true },
  ];

  async function run(cmd: string, ok: string) {
    error = done = "";
    working = true;
    try {
      const reply = await api.hostSend(server.name, cmd);
      done = reply?.trim() || ok;
      await refresh();
      await loadPeople();
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      working = false;
    }
  }

  async function kick(name: string) {
    if ((await ask(t("hosting.players.kickTitle", { name }), [t("hosting.players.kickLine")], [{ label: t("hosting.players.kick"), icon: "x" }])) !== 0) return;
    run(`kickuser ${q(name)}`, t("hosting.players.kicked", { name }));
  }

  async function ban(name: string) {
    const how = await ask(t("hosting.players.banTitle", { name }), [t("hosting.players.banLine"), t("hosting.players.banIpLine")], [{ label: t("hosting.players.ban"), icon: "lock" }, { label: t("hosting.players.banWithIp"), icon: "lock" }]);
    if (how === 0) run(`banuser ${q(name)}`, t("hosting.players.banned", { name }));
    else if (how === 1) run(`banuser ${q(name)} -ip`, t("hosting.players.banned", { name }));
  }

  /** An IPv4 or IPv6 address, as banip takes it. */
  const isIp = (s: string) => /^(\d{1,3}\.){3}\d{1,3}$/.test(s) || (/^[0-9a-f:]+$/i.test(s) && s.includes(":"));

  function banAddress(e: Event) {
    e.preventDefault();
    const ip = banIp.trim();
    if (!isIp(ip)) return;
    run(`banip ${ip}`, t("hosting.players.bannedIpDone", { ip }));
    banIp = "";
  }

  async function unwhitelist(name: string) {
    if ((await ask(t("hosting.players.removeTitle", { name }), [t("hosting.players.removeLine")], [{ label: t("hosting.players.remove"), icon: "trash" }])) !== 0) return;
    run(`removeuserfromwhitelist ${q(name)}`, t("hosting.players.removed", { name }));
  }

  /** A new account (with a password), else a connected player put on the whitelist. */
  function whitelist(e: Event) {
    e.preventDefault();
    const name = newUser.trim();
    if (!name) return;
    run(newPassword ? `adduser ${q(name)} ${q(newPassword)}` : `addusertowhitelist ${q(name)}`, t("hosting.players.added", { name }));
    newUser = newPassword = "";
  }

  function banSteam(e: Event) {
    e.preventDefault();
    const id = banId.trim();
    if (!/^\d{17}$/.test(id)) return;
    run(`banid ${id}`, t("hosting.players.bannedId", { id }));
    banId = "";
  }

  /** A player's access level from their account (the select shows it until another is picked); "" (the placeholder) for
   * a role that isn't one of LEVELS, such as Build 42's own roles. A plain user (or no level, Build 41) is "none". */
  function levelOf(name: string) {
    const role = people?.accounts?.users.find((u) => u.username.toLowerCase() === name.toLowerCase())?.role.toLowerCase();
    return role === undefined ? "" : role === "user" || role === "" ? "none" : LEVELS.includes(role) ? role : "";
  }

  function setLevel(name: string) {
    const level = levels[name] ?? "none";
    run(`setaccesslevel ${q(name)} ${level}`, t("hosting.players.levelSet", { name, level }));
  }

  /** Each line is a message of its own, in order (the game's on-screen alert shows the last). */
  async function broadcast(e: Event) {
    e.preventDefault();
    if (!lines.length) return;
    error = done = "";
    working = true;
    try {
      for (const l of lines) await api.hostSend(server.name, `servermsg ${q(l)}`);
      done = t("hosting.players.sent", { count: lines.length });
      message = "";
    } catch (err) {
      error = t(...keyedError(err));
    } finally {
      working = false;
    }
  }

  /** Facts about a player, in a row the player's language writes ("admin, 2 days ago, 7656…"). */
  const facts = (xs: (string | false | null | undefined)[]) => new Intl.ListFormat(intlLang(), { type: "unit" }).format(xs.filter((x): x is string => !!x));
  const position = (x: number, y: number, z: number) => `(${Math.round(x)}, ${Math.round(y)}, ${Math.round(z)})`; // bracketed: it sits inside a list

  // shown (again), or the server came up: who's on straight away
  $effect(() => {
    if (!hidden && up) untrack(refresh);
  });
  onMount(() => {
    loadPeople();
    let ticks = 0;
    // the map's positions are the server's last save: read players.db again every 30 s (while the tab shows)
    const timer = setInterval(() => {
      if (hidden) return;
      refresh();
      if (view === "map" && ++ticks % 2 === 0) loadPeople();
    }, 15000);
    return () => clearInterval(timer);
  });

  // the activity bar moves while this tab works
  $effect(() => {
    if (working) return track();
  });
</script>

<div class="players">
  {#if up}
    <form class="broadcast" onsubmit={broadcast}>
      <label for="pl-message">{t("hosting.players.message")}</label>
      <textarea id="pl-message" bind:this={messageBox} bind:value={message} rows="3" maxlength="2000" placeholder={t("hosting.players.messagePlaceholder")}></textarea>
      {#if lines.length}
        <div class="preview" aria-label={t("hosting.players.preview")}>
          <span class="hint">{t("hosting.players.preview")}</span>
          <p class="alert">{lines.at(-1)}</p>
          {#each lines as l}<p class="chat">{l}</p>{/each}
        </div>
      {/if}
      <div class="row">
        <span class="hint grow">{lines.length > MAX_LINES ? t("hosting.players.tooMany", { count: MAX_LINES }) : t("hosting.players.lines", { count: lines.length })}</span>
        <!-- one primary: an open command form's Send while it shows -->
        <button class:primary={!form} type="submit" disabled={working || !lines.length || lines.length > MAX_LINES}><Icon name="message" size={14} />{t("hosting.players.send")}</button>
      </div>
    </form>
  {:else}
    <p class="hint">{t("hosting.players.offline")}</p>
  {/if}

  <div class="bar">
    {#if views.length > 1}
      <div class="subtabs" role="tablist" aria-label={t("hosting.players.views")} tabindex="-1" onkeydown={(e) => arrowPick(e, views, view, (v) => (view = v))}>
        {#each views as v}
          <button role="tab" id="pl-tab-{v}" aria-controls="pl-panel" aria-selected={view === v} tabindex={view === v ? 0 : -1} class:active={view === v} onclick={() => (view = v)}>{t(`hosting.players.view.${v}`)}</button>
        {/each}
      </div>
    {/if}
    <span class="grow"></span>
    <button onclick={() => { refresh(); loadPeople(); }} disabled={working}><Icon name="refresh" size={14} />{t("common.refresh")}</button>
  </div>

  <div id="pl-panel" role="tabpanel" aria-labelledby="pl-tab-{view}">
    {#if view === "online"}
      {#if !up}
        <!-- nothing to list -->
      {:else if players && !players.length}
        <p class="hint">{t("hosting.players.nobody")}</p>
      {:else if players}
        <h3>{t("hosting.players.online", { count: players.length })}</h3>
        <ul class="list">
          {#each players as p}
            <li>
              <span class="who"><strong>{p}</strong>{@render tagsOf(p)}</span>
              <span class="btns">
                <button class="quiet" onclick={() => openNote(p)} aria-expanded={editing?.name === p}><Icon name="info" size={14} />{t("hosting.players.notes")}</button>
                <select value={levels[p] ?? levelOf(p)} onchange={(e) => (levels[p] = e.currentTarget.value)} aria-label={t("hosting.players.level", { name: p })}>
                  <option value="" disabled>{t("hosting.players.level", { name: p })}</option>
                  <!-- i18n-ignore: the server's access level names -->
                  {#each LEVELS as l}<option value={l}>{l}</option>{/each}
                </select>
                <button onclick={() => setLevel(p)} disabled={working || !levels[p] || levels[p] === levelOf(p)}><Icon name="check" size={14} />{t("hosting.players.setLevel")}</button>
                <!-- kick and ban are in the menu: a row of buttons that fits on one line -->
                <button onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); playerMenu(p, r.left, r.bottom + 2); }} aria-haspopup="menu" aria-expanded={menu?.name === p && !menu.map}><Icon name="chevron-down" size={14} />{t("hosting.players.commands")}</button>
              </span>
            </li>
            {#if editing?.name === p}{@render noteEditor()}{/if}
          {/each}
        </ul>
      {/if}
    {:else if view === "map"}
      <p class="hint">{t("hosting.players.mapHint")}</p>
      {#if worldError}<p class="error">{worldError}</p>
      {:else if !world}<p class="hint">{t("maps.readingFiles")}</p>
      {:else}
        <MapGrid grid={world.cells} paint={paintWorld} worlds={prefs.mapsWorld ? ["", ...world.maps] : []} backdrop={prefs.mapsWorld ? (world.size === 300 ? 41 : 42) : undefined} tiles={world.size} {dots} label={t("hosting.players.mapLabel", { count: dots.length })}
          onDot={(i, x, y) => playerMenu(dots[i].name, x, y, true)}
          onPick={([x, y]) => up && (form = { command: "teleportto", player: "", at: [Math.floor((x + 0.5) * world!.size), Math.floor((y + 0.5) * world!.size), 0] })}>
          {#snippet tip([x, y])}
            <strong>{t("maps.cell", { x, y })}</strong>
            {#each dots.filter((d) => Math.floor(d.x) === x && Math.floor(d.y) === y) as d}<span>{d.name}</span>{/each}
          {/snippet}
        </MapGrid>
        {#if !dots.length}<p class="hint">{t("hosting.players.noCharacters")}</p>{/if}
      {/if}
      {#if editing}<ul class="list">{@render noteEditor()}</ul>{/if}
    {:else if view === "history"}
      <p class="hint">{t("hosting.players.historyHint")}</p>
      <input type="search" bind:value={historyFilter} placeholder={t("hosting.players.username")} aria-label={t("hosting.players.username")} />
      {#if !history.length}
        <p class="hint">{t("hosting.players.noHistory")}</p>
      {:else}
        <ul class="list">
          {#each history as h, i (i)}
            <li>
              <span class="who"><strong>{h.name}</strong>{@render tagsOf(h.name)} <span class="hint">{t(h.event ? `hosting.players.event${h.event[0].toUpperCase()}${h.event.slice(1)}` : h.joined ? "hosting.players.joined" : "hosting.players.left")}</span></span>
              <span class="hint">{fmtDateTime(h.time)}</span>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if view === "characters"}
      <p class="hint">{t("hosting.players.charactersHint")}</p>
      {#if !people?.characters.length}
        <p class="hint">{t("hosting.players.noCharacters")}</p>
      {:else}
        <ul class="list">
          {#each people.characters as c (c.id)}
            {@const on = players?.includes(c.username)}
            {@const pos = position(c.x, c.y, c.z)}
            <li>
              <span class="who">
                <strong>{c.username || c.name}</strong>{#if c.username}{@render tagsOf(c.username)}{/if}
                <span class="hint">{#each new Intl.ListFormat(intlLang(), { type: "unit" }).formatToParts([c.name, c.dead ? t("hosting.players.dead") : "", pos, on ? t("hosting.players.isOnline") : ""].filter(Boolean)) as p}{#if p.type === "element" && p.value === pos}<span class="mono">{p.value}</span>{:else}{p.value}{/if}{/each}</span>
              </span>
              {#if c.username}
                <span class="btns">
                  <button class="quiet" onclick={() => openNote(c.username)} aria-expanded={editing?.name === c.username}><Icon name="info" size={14} />{t("hosting.players.notes")}</button>
                  <button class="danger" onclick={() => ban(c.username)} disabled={working || !up}><Icon name="lock" size={14} />{t("hosting.players.ban")}</button>
                </span>
              {/if}
            </li>
            {#if c.username && editing?.name === c.username}{@render noteEditor()}{/if}
          {/each}
        </ul>
      {/if}
    {:else if view === "whitelist"}
      <form class="row" onsubmit={whitelist}>
        <input type="text" bind:value={newUser} placeholder={t("hosting.players.username")} aria-label={t("hosting.players.username")} spellcheck="false" autocomplete="off" />
        <input type="password" bind:value={newPassword} placeholder={t("hosting.players.passwordOptional")} aria-label={t("hosting.players.passwordOptional")} autocomplete="off" />
        <button type="submit" disabled={working || !up || !newUser.trim()} title={t("hosting.players.addTip")}><Icon name="plus" size={14} />{t("hosting.players.add")}</button>
      </form>
      {#if people?.accounts_error}<p class="hint">{people.accounts_error}</p>
      {:else if !users.length}<p class="hint">{t("hosting.players.noAccounts")}</p>
      {:else}
        <ul class="list">
          {#each users as u (u.username)}
            <li>
              <span class="who">
                <strong>{u.username}</strong>
                <span class="hint">{facts([u.role, u.last && t("hosting.players.lastSeen", { when: u.last }), u.steamid])}</span>
              </span>
              <span class="btns">
                <button onclick={() => unwhitelist(u.username)} disabled={working || !up}><Icon name="trash" size={14} />{t("hosting.players.remove")}</button>
                <button class="danger" onclick={() => ban(u.username)} disabled={working || !up}><Icon name="lock" size={14} />{t("hosting.players.ban")}</button>
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    {:else}
      <form class="row" onsubmit={banSteam}>
        <input type="text" class="mono" bind:value={banId} placeholder={t("hosting.players.steamId")} aria-label={t("hosting.players.steamId")} inputmode="numeric" spellcheck="false" />
        <button type="submit" class="danger" disabled={working || !up || !/^\d{17}$/.test(banId.trim())}><Icon name="lock" size={14} />{t("hosting.players.banId")}</button>
      </form>
      {#if b42}
        <form class="row" onsubmit={banAddress}>
          <input type="text" class="mono" bind:value={banIp} placeholder={t("hosting.players.ipAddress")} aria-label={t("hosting.players.ipAddress")} spellcheck="false" autocomplete="off" />
          <button type="submit" class="danger" disabled={working || !up || !isIp(banIp.trim())}><Icon name="lock" size={14} />{t("hosting.players.banIp")}</button>
        </form>
      {/if}
      {#if people?.accounts_error}<p class="hint">{people.accounts_error}</p>
      {:else if !banned.length && !people?.accounts?.banned_ids.length && !people?.accounts?.banned_ips.length}
        <p class="hint">{t("hosting.players.noBans")}</p>
      {:else}
        <ul class="list">
          {#each banned as u (u.username)}
            <li>
              <span class="who"><strong>{u.username}</strong> <span class="hint">{t("hosting.players.bannedAccount")}</span></span>
              <button onclick={() => run(`unbanuser ${q(u.username)}`, t("hosting.players.unbanned", { name: u.username }))} disabled={working || !up}><Icon name="undo" size={14} />{t("hosting.players.unban")}</button>
            </li>
          {/each}
          {#each people?.accounts?.banned_ids ?? [] as [id, reason] (id)}
            <li>
              <span class="who"><span class="mono">{id}</span> <span class="hint">{reason || t("hosting.players.bannedSteam")}</span></span>
              <button onclick={() => run(`unbanid ${id}`, t("hosting.players.unbanned", { name: id }))} disabled={working || !up}><Icon name="undo" size={14} />{t("hosting.players.unban")}</button>
            </li>
          {/each}
          {#each people?.accounts?.banned_ips ?? [] as [ip, name, reason] (ip + name)}
            <li>
              <span class="who"><span class="mono">{ip}</span> <span class="hint">{facts([name, reason || t("hosting.players.bannedIp")])}</span></span>
              {#if b42}<button onclick={() => run(`unbanip ${ip}`, t("hosting.players.unbanned", { name: ip }))} disabled={working || !up}><Icon name="undo" size={14} />{t("hosting.players.unban")}</button>
              {:else if name}<button onclick={() => run(`unbanuser ${q(name)}`, t("hosting.players.unbanned", { name }))} disabled={working || !up}><Icon name="undo" size={14} />{t("hosting.players.unban")}</button>{/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
  {#if form}
    {#key form}
      <ServerCommands {server} command={form.command} player={form.player} at={form.at} players={players ?? []} onClose={() => (form = null)}
        onDone={(_, reply) => { done = reply?.trim() || t("hosting.players.done"); form = null; }} />
    {/key}
  {/if}
  {#if done}<p class="hint" role="status">{done}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

{#snippet tagsOf(name: string)}
  {#each dossier.players[name]?.tags ?? [] as tag}<span class="tag">{tag}</span>{/each}
  {#if dossier.players[name]?.note}<span class="hint note" title={dossier.players[name].note}>{dossier.players[name].note}</span>{/if}
{/snippet}

{#snippet noteEditor()}
  {#if editing}
    <li class="editor">
      <form onsubmit={saveNote}>
        <label>{t("hosting.players.noteFor", { name: editing.name })}<textarea bind:value={editing.note} rows="2" maxlength="2000"></textarea></label>
        <label>{t("hosting.players.tags")}<input type="text" bind:value={editing.tags} placeholder={t("hosting.players.tagsPlaceholder")} /></label>
        <div class="btns">
          <button type="submit"><Icon name="check" size={14} />{t("hosting.players.saveNote")}</button>
          <button class="quiet" type="button" onclick={() => (editing = null)}>{t("common.cancel")}</button>
        </div>
      </form>
    </li>
  {/if}
{/snippet}

{#if menu}
  {@const name = menu.name}
  <Menu x={menu.x} y={menu.y} label={t("hosting.players.commands")} onClose={() => (menu = null)} items={menuItems(name, !!menu.map)} />
{/if}

<style>
  .row,
  .bar {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    margin: 8px 0;
  }
  .row input {
    flex: 1;
    min-width: 140px;
  }
  .grow {
    flex: 1;
  }
  .broadcast {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .broadcast label {
    color: var(--muted);
  }
  textarea {
    resize: vertical;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--raised);
    color: inherit;
  }
  .preview {
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--bg);
    border: 1px solid var(--line);
  }
  .preview p {
    margin: 2px 0;
    overflow-wrap: anywhere;
  }
  /* the game's on-screen alert is large red text, centred; its chat shows every line */
  .alert {
    text-align: center;
    font-size: 16px;
    color: var(--danger);
  }
  .chat {
    font-size: 12px;
  }
  h3 {
    margin: 18px 0 4px;
    font-size: 15px;
  }
  .list {
    list-style: none;
    padding: 0;
    margin: 0;
    border-top: 1px solid var(--line);
  }
  /* a row too narrow for its buttons puts them all on a line of their own, so rows stay alike */
  .list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 12px;
    padding: 6px 8px;
    margin: 0 -8px;
    border-radius: 4px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  .list li:not(.editor):hover {
    color: var(--on-selected);
    background: var(--hover);
    box-shadow: inset 0 0 0 1px var(--hover-line);
  }
  /* Ban's red is unreadable on the highlight (Daylight's ~2.9:1) */
  .list li:not(.editor):hover button.danger:not(:disabled) {
    color: var(--on-selected);
  }
  .who {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .tag {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 4px;
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--muted);
  }
  .note {
    display: inline-block;
    max-width: 40ch;
    margin-left: 6px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    vertical-align: bottom;
    font-size: 12px;
  }
  .list li.editor {
    display: block;
  }
  .editor form {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .editor label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
  }
  .players :global(.map-grid) {
    height: 420px;
    margin: 8px 0;
    border-radius: 6px;
    border: 1px solid var(--line);
  }
  .btns {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  li > .btns {
    margin-left: auto;
  }
</style>
