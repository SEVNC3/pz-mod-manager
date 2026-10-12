<script lang="ts" module>
  /** The admin commands the palette offers, with their arguments (as the server's `help` writes them). */
  export const COMMANDS: { name: string; args: string }[] = [
    { name: "players", args: "" },
    { name: "servermsg", args: '"message"' },
    { name: "save", args: "" },
    { name: "quit", args: "" },
    { name: "kickuser", args: '"user" -r "reason"' },
    { name: "banuser", args: '"user" -ip -r "reason"' },
    { name: "unbanuser", args: '"user"' },
    { name: "banid", args: "SteamID" },
    { name: "unbanid", args: "SteamID" },
    { name: "banip", args: "IP" },
    { name: "unbanip", args: "IP" },
    { name: "voiceban", args: '"user" -true|-false' },
    { name: "setaccesslevel", args: '"user" admin|moderator|overseer|gm|observer|none' },
    { name: "adduser", args: '"user" "password"' },
    { name: "addusertowhitelist", args: '"user"' },
    { name: "removeuserfromwhitelist", args: '"user"' },
    { name: "teleport", args: '"user" "toUser"' },
    { name: "teleportto", args: '"user" x,y,z' },
    { name: "additem", args: '"user" "Module.Item" count' },
    { name: "addxp", args: '"user" Perk=xp' },
    { name: "addvehicle", args: '"Module.Vehicle" "user"' },
    { name: "godmod", args: '"user" -true|-false' },
    { name: "invisible", args: '"user" -true|-false' },
    { name: "noclip", args: '"user" -true|-false' },
    { name: "createhorde", args: 'count "user"' },
    { name: "checkModsNeedUpdate", args: "" },
    { name: "reloadoptions", args: "" },
    { name: "showoptions", args: "" },
    { name: "changeoption", args: 'Option "value"' },
    { name: "chopper", args: "" },
    { name: "thunder", args: '"user"' },
    { name: "lightning", args: '"user"' },
    { name: "gunshot", args: "" },
    { name: "startrain", args: "1-100" },
    { name: "stoprain", args: "" },
    { name: "startstorm", args: "hours" },
    { name: "stopweather", args: "" },
    { name: "setTimeSpeed", args: "multiplier" },
    { name: "help", args: "" },
  ];
</script>

<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type LogLine, type ServerCard, type ServerEvent } from "./api";
  import { keyedError, t } from "./i18n.svelte";
  import { arrowPick, menuKeys } from "./ui";
  import Icon from "./Icon.svelte";
  import ServerCommands, { B42_ONLY, FORMS, WORLD_FORMS } from "./ServerCommands.svelte";
  import LogView from "./LogView.svelte";

  /** `hidden`: the Servers tab is behind another one, so the file isn't read meanwhile. */
  let { server, hidden = false }: { server: ServerCard; hidden?: boolean } = $props();

  let lines = $state<LogLine[]>([]);
  let attached = $state(false);
  let command = $state("");
  let history: string[] = [];
  let at = -1;
  let error = $state("");
  let sending = $state(false);
  let paletteOpen = $state(false);
  let palette = $state<HTMLElement>();
  let paletteWrap = $state<HTMLElement>();
  let paletteButton = $state<HTMLButtonElement>();
  /** The palette's height: what fits above its button (a short window has less than 320px), so opening it doesn't scroll the button away. */
  let paletteMax = $state(320);
  let input = $state<HTMLInputElement>();
  /** The last RCON reply (it isn't in the server's log). */
  let reply = $state<{ cmd: string; text: string } | null>(null);
  /** The palette command whose form is open, and who's online for it. */
  let form = $state<string | null>(null);
  let online = $state<string[]>([]);
  /** What the log box shows: the console, or the chat or perk log (followed by polling). */
  const SOURCES = ["console", "chat", "perk"] as const;
  let source = $state<(typeof SOURCES)[number]>("console");
  let other = $state<{ file: string | null; lines: string[] }>({ file: null, lines: [] });
  /** The chat or perk log as log lines. */
  const otherLines = $derived<LogLine[]>(other.lines.map((text, seq) => ({ seq, text, level: "info", note: null })));

  async function loadOther() {
    if (source === "console") return;
    const [file, got] = await api.hostGameLog(server.name, source).catch(() => [null, []] as [string | null, string[]]);
    other = { file, lines: got };
  }

  function pick(s: (typeof SOURCES)[number]) {
    source = s;
    other = { file: null, lines: [] };
    loadOther();
  }

  const reachable = $derived(server.status === "running" || (server.attached && server.status === "starting"));
  /** The commands this server's build has. */
  const commands = $derived(COMMANDS.filter((c) => server.build !== "legacy41" || !B42_ONLY.includes(c.name)));
  const hint = $derived(commands.find((c) => c.name.toLowerCase() === command.trim().split(/\s/)[0]?.toLowerCase()));

  /** Lines waiting for the next frame: a busy server's lines are added a frame's worth at a time, not one copy each. */
  let queued: LogLine[] = [];
  function follow(add: LogLine[]) {
    if (!queued.length) requestAnimationFrame(flush);
    queued.push(...add);
    if (queued.length > 10000) queued = queued.slice(-5000);
  }
  function flush() {
    // a line can come both as an event and in a load's reply
    let last = lines.at(-1)?.seq ?? -1;
    const add = queued.filter((l) => l.seq > last && ((last = l.seq), true));
    queued = [];
    if (add.length) lines = [...lines, ...add].slice(-5000);
  }
  /** Start over (the lines queued go too). */
  function clear() {
    lines = [];
    queued = [];
  }

  async function load() {
    // the file read before: only its lines after the last one
    const after = attached ? 0 : (lines.at(-1)?.seq ?? 0);
    let got = await api.hostLog(server.name, after).catch(() => null);
    if (got?.attached && after) got = await api.hostLog(server.name, 0).catch(() => null);
    if (!got) return;
    // nothing new, or the lines that came after (a new file starts over)
    if (!got.attached && !attached && after && (!got.lines.length || got.lines[0].seq > after)) return follow(got.lines);
    attached = got.attached;
    clear();
    follow(got.lines);
  }

  async function send(e: Event) {
    e.preventDefault();
    const cmd = command.trim();
    if (!cmd) return;
    error = "";
    sending = true;
    try {
      const text = await api.hostSend(server.name, cmd);
      history = [cmd, ...history.filter((h) => h !== cmd)].slice(0, 50);
      at = -1;
      command = "";
      // RCON answers directly; the console's answer shows in the log
      reply = text === null ? null : { cmd, text: text.trimEnd() };
    } catch (err) {
      error = t(...keyedError(err));
    } finally {
      sending = false;
    }
  }

  function keys(e: KeyboardEvent) {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    at = Math.max(-1, Math.min(history.length - 1, at + (e.key === "ArrowUp" ? 1 : -1)));
    command = at < 0 ? "" : history[at];
  }

  /** Height between the palette button and the top of what scrolls it (the window's top if nothing does). */
  function roomAbove() {
    let top = 0;
    for (let e = paletteButton?.parentElement; e; e = e.parentElement)
      if (/auto|scroll/.test(getComputedStyle(e).overflowY)) {
        top = e.getBoundingClientRect().top;
        break;
      }
    return (paletteButton?.getBoundingClientRect().top ?? 0) - top - 12;
  }

  /** The palette opens on its first command. */
  async function togglePalette() {
    paletteOpen = !paletteOpen;
    paletteMax = Math.max(120, Math.min(320, roomAbove()));
    await tick();
    palette?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
  }

  async function choose(c: (typeof COMMANDS)[number]) {
    paletteOpen = false;
    if (FORMS.includes(c.name) || WORLD_FORMS.includes(c.name)) {
      form = c.name;
      online = await api.hostPlayers(server.name).catch(() => []);
      return;
    }
    command = c.args ? `${c.name} ` : c.name;
    await tick();
    input?.focus();
  }

  const look = () => (source !== "console" ? loadOther() : !attached && reachable && load());
  // shown again: read straight away (the first look is the mount's)
  let shownBefore = false;
  $effect(() => {
    if (hidden) return;
    if (shownBefore) untrack(look);
    shownBefore = true;
  });
  onMount(() => {
    load();
    const unlisten = listen<ServerEvent>("server", ({ payload: e }) => {
      if (e.name !== server.name) return;
      if (e.line) {
        if (!attached) clear(); // the file's lines give way to the live ones
        attached = true;
        follow([e.line]);
      } else if (e.status === "starting") load();
    });
    // a server PZMM didn't start (or no longer has the console of): its server-console.txt, read
    // again while the window shows
    const timer = setInterval(() => !document.hidden && !hidden && look(), 5000);
    return () => {
      unlisten.then((f) => f());
      clearInterval(timer);
    };
  });
</script>

<svelte:window onpointerdown={(e) => paletteOpen && !paletteWrap?.contains(e.target as Node) && (paletteOpen = false)} />

<div class="console">
  <LogView
    lines={source === "console" ? lines : otherLines}
    status={source === "console" ? (attached ? t("hosting.console.live") : t("hosting.console.file")) : (other.file ?? t(source === "chat" ? "hosting.console.noChat" : "hosting.console.noPerks"))}
    label={t("hosting.console.logLabel")}
    id="con-log"
    chips
  >
    {#snippet tools()}
      <div class="subtabs" role="tablist" aria-label={t("hosting.console.source")} tabindex="-1" onkeydown={(e) => arrowPick(e, [...SOURCES], source, (v) => pick(v as typeof source))}>
        {#each SOURCES as s}
          <button role="tab" aria-controls="con-log" aria-selected={source === s} tabindex={source === s ? 0 : -1} class:active={source === s} onclick={() => pick(s)}>{t(`hosting.console.sources.${s}`)}</button>
        {/each}
      </div>
    {/snippet}
  </LogView>
  <form class="send" onsubmit={send}>
    <div class="palette-wrap" bind:this={paletteWrap}>
      <button type="button" bind:this={paletteButton} onclick={togglePalette} aria-haspopup="menu" aria-expanded={paletteOpen} disabled={!reachable}><Icon name="chevron-up" size={14} />{t("hosting.console.commands")}</button>
      {#if paletteOpen}
        <div class="popup-menu palette" role="menu" bind:this={palette} style:max-height="{paletteMax}px" tabindex="-1" onkeydown={(e) => menuKeys(e, palette, (r) => { paletteOpen = false; if (r) paletteButton?.focus(); })}>
          {#each commands as c}
            <button class="item" role="menuitem" onclick={() => choose(c)}>
              <span class="mono">{c.name}</span>
              <span class="hint">{t(`hosting.cmd.${c.name}`)}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <!-- i18n-ignore: a console command -->
    <input type="text" class="mono" bind:this={input} bind:value={command} onkeydown={keys} placeholder="players" aria-label={t("hosting.console.command")} spellcheck="false" autocomplete="off" disabled={!reachable} />
    <button type="submit" disabled={!reachable || sending || !command.trim()}><Icon name="play" size={14} />{t("hosting.console.send")}</button>
  </form>
  {#if hint}
    <!-- i18n-ignore: command syntax -->
    <p class="hint"><span class="mono">{hint.name} {hint.args}</span> · {t(`hosting.cmd.${hint.name}`)}</p>
  {/if}
  {#if form}
    {#key form}
      <ServerCommands {server} command={form} players={online} onClose={() => (form = null)}
        onDone={(cmd, text) => { reply = text === null ? null : { cmd, text: text.trimEnd() }; history = [cmd, ...history.filter((h) => h !== cmd)].slice(0, 50); }} />
    {/key}
  {/if}
  {#if reply}<pre class="mono reply" aria-live="polite">&gt; {reply.cmd}{"\n"}{reply.text}</pre>{/if}
  {#if !reachable}<p class="hint">{t("hosting.console.offline")}</p>{:else if !attached}<p class="hint">{t("hosting.console.viaRcon")}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .send {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .send input[type="text"] {
    flex: 1;
  }
  .reply {
    margin: 8px 0 0;
    padding: 8px 10px;
    max-height: 200px;
    overflow: auto;
    white-space: pre-wrap;
    border-radius: 6px;
    background: var(--bg);
    border: 1px solid var(--line);
  }
  .palette-wrap {
    position: relative;
  }
  .palette {
    position: absolute;
    bottom: calc(100% + 4px);
    left: 0;
    overflow: auto;
    min-width: 380px;
  }
  .palette .item {
    justify-content: space-between;
    gap: 16px;
  }
</style>
