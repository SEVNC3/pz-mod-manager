<script lang="ts" module>
  /** The admin commands with a form here for one player (the rest are typed in the console). */
  export const FORMS = ["teleport", "teleportto", "additem", "addxp", "addvehicle", "godmod", "invisible", "noclip", "createhorde", "thunder", "lightning"];
  /** Forms for the whole world: weather and the clock. */
  export const WORLD_FORMS = ["startrain", "startstorm", "setTimeSpeed"];
  /** Quote a value for a console argument; quotes inside can't be passed. */
  export const q = (v: string) => `"${v.trim().replaceAll('"', "")}"`;
  /** Commands Build 41's server doesn't have. */
  export const B42_ONLY = ["banip", "unbanip", "setTimeSpeed"];
  /** The skills addxp takes (the game's perk IDs); Build 42 added the crafts at the end. */
  const PERKS = ["Fitness", "Strength", "Sprinting", "Lightfoot", "Nimble", "Sneak", "Axe", "Blunt", "SmallBlunt", "LongBlade", "SmallBlade", "Spear", "Maintenance", "Woodwork", "Cooking", "Farming", "Doctor", "Electricity", "MetalWelding", "Mechanics", "Tailoring", "Aiming", "Reloading", "Fishing", "Trapping", "PlantScavenging", "Blacksmith", "Butchering", "Carving", "FlintKnapping", "Glassmaking", "Husbandry", "Masonry", "Pottery", "Tracking"];
</script>

<script lang="ts">
  import { api, type Catalog, type ServerCard } from "./api";
  import { keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";

  // A form for one admin command (Console's palette, a player's Commands menu): it writes the command and sends it.
  let { server, command, player = "", at = [0, 0, 0], players, onDone, onClose }: {
    server: ServerCard;
    /** One of FORMS or WORLD_FORMS. */
    command: string;
    player?: string;
    /** Where teleportto sends them (a tile: x, y, floor). */
    at?: [number, number, number];
    /** Who's online, offered as names. */
    players: string[];
    /** The reply (RCON's; null when it shows in the log). */
    onDone: (cmd: string, reply: string | null) => void;
    onClose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let who = $state(player);
  let to = $state("");
  let thing = $state("");
  let count = $state(1);
  let perk = $state("Woodwork");
  let xp = $state(100);
  let on = $state(true);
  // svelte-ignore state_referenced_locally
  let spot = $state([...at]);
  /** Rain's strength (1-100), a storm's game hours, the clock's multiplier. */
  // svelte-ignore state_referenced_locally
  let amount = $state(command === "startrain" ? 50 : command === "startstorm" ? 6 : 1);
  let error = $state("");
  let sending = $state(false);
  /** The game's items and vehicles (read the first time a form needs them). */
  let catalog = $state<Catalog | null>(null);
  let catalogError = $state("");

  const b41 = $derived(server.build === "legacy41");
  const world = $derived(WORLD_FORMS.includes(command));
  const needs = $derived(command === "additem" ? "items" : command === "addvehicle" ? "vehicles" : null);
  /** Up to 60 of the catalog's matches for what's typed, for the field's suggestions. */
  const matches = $derived.by(() => {
    if (!catalog || !needs) return [];
    const q = thing.trim().toLowerCase();
    return catalog[needs].filter((e) => !q || e.id.toLowerCase().includes(q) || e.name.toLowerCase().includes(q)).slice(0, 60);
  });
  const known = $derived(!!catalog && !!needs && catalog[needs].some((e) => e.id === thing.trim()));

  $effect(() => {
    if (needs && !catalog && !catalogError) api.hostCatalog(server.name).then((c) => (catalog = c)).catch((e) => (catalogError = t(...keyedError(e))));
  });

  /** The command as the server takes it (Build 41 and 42 name the god and invisible ones differently). */
  const text = $derived.by(() => {
    switch (command) {
      case "teleport": return `teleport ${q(who)} ${q(to)}`;
      case "teleportto": return `teleportto ${q(who)} ${spot.map((n) => Math.floor(n || 0)).join(",")}`;
      case "thunder": case "lightning": return `${command} ${q(who)}`;
      case "startrain": return `startrain ${Math.min(100, Math.max(1, Math.round(amount)))}`;
      case "startstorm": return `startstorm ${Math.max(1, Math.round(amount))}`;
      case "setTimeSpeed": return `setTimeSpeed ${Math.max(0.1, amount)}`;
      case "additem": return `additem ${q(who)} ${q(thing)} ${Math.max(1, Math.floor(count))}`;
      case "addxp": return `addxp ${q(who)} ${perk.trim()}=${Math.floor(xp)}`;
      case "addvehicle": return `addvehicle ${q(thing)} ${q(who)}`;
      case "godmod": return `${b41 ? "godmod" : "godmodplayer"} ${q(who)} -${on}`;
      case "invisible": return `${b41 ? "invisible" : "invisibleplayer"} ${q(who)} -${on}`;
      case "noclip": return `noclip ${q(who)} -${on}`;
      default: return `createhorde ${Math.max(1, Math.floor(count))} ${q(who)}`;
    }
  });
  const ready = $derived((world || !!who.trim()) && (command !== "teleport" || !!to.trim()) && (!needs || !!thing.trim()) && (command !== "addxp" || /^\w+$/.test(perk.trim())));

  async function send(e: Event) {
    e.preventDefault();
    if (!ready) return;
    error = "";
    sending = true;
    try {
      onDone(text, await api.hostSend(server.name, text));
    } catch (err) {
      error = t(...keyedError(err));
    } finally {
      sending = false;
    }
  }
</script>

<form class="cmd" onsubmit={send} aria-label={t(`hosting.cmd.${command}`)}>
  <div class="head">
    <strong>{t(`hosting.cmd.${command}`)}</strong>
    <button type="button" class="quiet icon" onclick={onClose} aria-label={t("common.close")}><Icon name="x" size={14} /></button>
  </div>
  <div class="fields">
    {#if !world}
      <label>{t(command === "teleport" || command === "teleportto" ? "hosting.form.move" : command === "createhorde" ? "hosting.form.near" : "hosting.form.player")}
        <input type="text" list="cmd-players" bind:value={who} required spellcheck="false" autocomplete="off" />
      </label>
    {/if}
    {#if command === "teleportto"}
      {#each ["x", "y", "floor"] as k, i}
        <label>{t(`hosting.form.${k}`)}<input type="number" bind:value={spot[i]} min={i === 2 ? -32 : 0} max={i === 2 ? 31 : undefined} required /></label>
      {/each}
    {/if}
    {#if command === "startrain"}
      <label class="wide">{t("hosting.form.intensity", { value: amount })}<input type="range" bind:value={amount} min="1" max="100" /></label>
    {:else if command === "startstorm"}
      <label>{t("hosting.form.hours")}<input type="number" bind:value={amount} min="1" max="72" required /></label>
    {:else if command === "setTimeSpeed"}
      <label>{t("hosting.form.speed")}<input type="number" bind:value={amount} min="0.1" max="100" step="0.1" required /></label>
    {/if}
    {#if command === "teleport"}
      <label>{t("hosting.form.to")}<input type="text" list="cmd-players" bind:value={to} required spellcheck="false" autocomplete="off" /></label>
    {/if}
    {#if needs}
      <label class="wide">{t(needs === "items" ? "hosting.form.item" : "hosting.form.vehicle")}
        <input type="text" list="cmd-things" bind:value={thing} required spellcheck="false" autocomplete="off" placeholder={t("hosting.form.search")} />
      </label>
      <datalist id="cmd-things">{#each matches as m (m.id)}<option value={m.id}>{m.name}</option>{/each}</datalist>
    {/if}
    {#if command === "additem" || command === "createhorde"}
      <label>{t(command === "additem" ? "hosting.form.count" : "hosting.form.zombies")}<input type="number" bind:value={count} min="1" max={command === "additem" ? 100 : 500} required /></label>
    {/if}
    {#if command === "addxp"}
      <label>{t("hosting.form.skill")}<input type="text" list="cmd-perks" bind:value={perk} required spellcheck="false" autocomplete="off" /></label>
      <datalist id="cmd-perks">{#each PERKS as p}<option value={p}></option>{/each}</datalist>
      <label>{t("hosting.form.xp")}<input type="number" bind:value={xp} min="1" required /></label>
    {/if}
    {#if command === "godmod" || command === "invisible" || command === "noclip"}
      <label class="check"><input type="checkbox" bind:checked={on} />{t("hosting.form.on")}</label>
    {/if}
  </div>
  <datalist id="cmd-players">{#each players as p}<option value={p}></option>{/each}</datalist>
  {#if command === "setTimeSpeed"}<p class="hint">{t("hosting.form.speedHint")}</p>{/if}
  {#if needs && catalogError}<p class="hint">{t("hosting.form.noCatalog", { error: catalogError })}</p>
  {:else if needs && catalog && thing.trim() && !known}<p class="hint">{t("hosting.form.unknown")}</p>{/if}
  <div class="send">
    <!-- i18n-ignore: the console command it sends -->
    <code class="mono">{text}</code>
    <button class="primary" type="submit" disabled={!ready || sending}><Icon name="play" size={14} />{t("hosting.console.send")}</button>
  </div>
  {#if error}<p class="error">{error}</p>{/if}
</form>

<style>
  .cmd {
    margin: 8px 0;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--panel);
    box-shadow: var(--card-shadow);
  }
  .head,
  .send {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 8px 12px;
    margin: 8px 0;
  }
  .fields label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
  }
  .fields .wide {
    grid-column: span 2;
  }
  .fields .check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .send code {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--muted);
  }
</style>
