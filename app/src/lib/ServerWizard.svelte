<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type HostView, type ServerLines } from "./api";
  import { confirmed } from "./ask.svelte";
  import { fmtNum } from "./format";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import { arrowPick } from "./ui";
  import Icon from "./Icon.svelte";

  let { host, profiles, profile, taken, onDone, onCancel }: {
    host: HostView;
    profiles: string[];
    profile: string;
    /** Server names in use. */
    taken: string[];
    onDone: (name: string) => void;
    onCancel: () => void;
  } = $props();

  const STEPS = ["server", "network", "admin", "mods"] as const;
  let step = $state(0);

  let name = $state("");
  let build = $state<"b42" | "legacy41">("b42");
  let installDir = $state("");
  let ownData = $state(false);
  let dataDir = $state("");
  let gamePort = $state(16261);
  let directPort = $state(16262);
  let ports = $state<boolean[] | null>(null);
  let publicListing = $state(false);
  let upnp = $state(false);
  /** The address players join at, when a tunnel gives one. */
  let address = $state("");
  let maxPlayers = $state(16);
  let joinPassword = $state("");
  let adminUser = $state("admin");
  let adminPassword = $state(newPassword());
  let memoryGb = $state(0);
  let modsFrom = $state<"empty" | "profile" | "collection">("empty");
  let fromProfile = $state("");
  let collection = $state("");
  let lines = $state<ServerLines>({ mods: [], workshop: [], maps: [] });
  let notes = $state<string[]>([]);
  let loading = $state(false);
  let creating = $state(false);
  let progress = $state<number | null>(null);
  let error = $state("");
  let copied = $state("");
  /** Set up, but its first start failed: it's a server now, started from its own page. */
  let created = $state(false);
  /** What `lines` were loaded from: another build, source, profile or collection makes them stale. */
  const source = $derived(`${build} ${modsFrom} ${modsFrom === "collection" ? collection.trim() : fromProfile}`);
  let loadedFor = $state("");

  const ramGb = $derived(Math.floor(host.ram_mb / 1024));
  const maxGb = $derived(Math.max(2, Math.min(32, ramGb - 2)));
  const defaultInstall = $derived(build === "b42" ? host.install_b42 : host.install_41);
  const firewall = $derived(
    // i18n-ignore: a Windows command
    `netsh advfirewall firewall add rule name="Project Zomboid server" dir=in action=allow protocol=UDP localport=${gamePort},${directPort}`,
  );

  /** 20 letters and digits that don't read alike, from the OS's random source. */
  function newPassword() {
    const abc = "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    return [...crypto.getRandomValues(new Uint8Array(20))].map((b) => abc[b % abc.length]).join("");
  }

  const BAD_NAME = /[<>:"/\\|?*&^%!();,=`$]|^\.|[. ]$/;
  const nameError = $derived(
    !name.trim() ? "" : BAD_NAME.test(name.trim()) ? t("hosting.wizard.nameBad") : taken.includes(name.trim()) ? t("hosting.wizard.nameTaken") : "",
  );
  /** Another server on this PC with either port (one run elsewhere has its own). */
  const clashOf = (a: number, b: number) => host.servers.find((s) => !s.remote && [s.game_port, s.direct_port].some((p) => p === a || p === b));
  const clash = $derived(clashOf(gamePort, directPort));
  const portError = $derived(
    [gamePort, directPort].some((p) => !Number.isInteger(p) || p < 1024 || p > 65535) ? t("hosting.wizard.portRange")
    : gamePort === directPort ? t("hosting.wizard.portSame")
    : clash ? t("hosting.wizard.portUsed", { name: clash.name })
    : "",
  );
  const stepOk = $derived(
    [
      !!name.trim() && !nameError && (!ownData || !!dataDir.trim()),
      !portError && maxPlayers >= 1 && maxPlayers <= 100,
      !!adminUser.trim() && !/[\s"]/.test(adminUser) && adminPassword.length >= 8,
      !loading,
    ][step],
  );

  async function checkPorts() {
    ports = await api.hostPorts([[gamePort, true], [directPort, true]]).catch(() => null);
  }

  async function pick(which: "install" | "data") {
    const dir = await api.pickFolder(which === "install" ? installDir || defaultInstall : dataDir || host.zomboid).catch(() => null);
    if (!dir) return;
    if (which === "install") installDir = dir;
    else dataDir = dir;
  }

  async function copy(text: string, what: string) {
    await navigator.clipboard.writeText(text).catch(() => {});
    copied = what;
    setTimeout(() => copied === what && (copied = ""), 1500);
  }

  async function loadMods() {
    error = "";
    loading = true;
    const want = source;
    try {
      [lines, notes] =
        modsFrom === "profile" ? await api.hostLinesFromProfile(fromProfile, build === "b42" ? 42 : 41)
        : modsFrom === "collection" ? await api.hostLinesFromCollection(collection.trim(), build === "b42" ? 42 : 41)
        : [{ mods: [], workshop: [], maps: [] }, []];
      loadedFor = want;
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      loading = false;
    }
  }

  async function create() {
    error = "";
    creating = true;
    progress = null;
    try {
      await api.hostCreate({
        name: name.trim(),
        build,
        install_dir: installDir.trim() || null,
        cachedir: ownData ? dataDir.trim() : null,
        game_port: gamePort,
        direct_port: directPort,
        admin_user: adminUser.trim(),
        admin_password: adminPassword,
        join_password: joinPassword,
        max_players: maxPlayers,
        public: publicListing,
        upnp,
        address: address.trim(),
        memory_mb: Math.round(memoryGb * 1024),
        lines: modsFrom === "empty" ? { mods: [], workshop: [], maps: [] } : lines,
      });
      onDone(name.trim());
    } catch (e) {
      error = t(...keyedError(e));
      const n = name.trim();
      created = await api.hostView().then((h) => h.servers.some((s) => s.name === n)).catch(() => false);
    } finally {
      creating = false;
    }
  }

  /** Esc: once something is filled in, ask before throwing it away. */
  async function cancel() {
    if (creating) return;
    if (created) return onDone(name.trim());
    if ((step || name.trim()) && !(await confirmed(t("hosting.wizard.discardAsk"), { label: t("hosting.wizard.discard"), icon: "x" }))) return;
    onCancel();
  }

  onMount(() => {
    memoryGb = host.memory_mb / 1024;
    fromProfile = profile;
    // the first pair no other server here uses
    while (clashOf(gamePort, directPort)) {
      gamePort += 2;
      directPort += 2;
    }
    const unlisten = listen<[number, number]>("progress", (e) => creating && (progress = e.payload[0]));
    return () => unlisten.then((f) => f());
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: Esc leaves the wizard (and stops there, or the app's own Esc answers the discard question at once) -->
<div class="strip wizard" role="group" aria-label={t("hosting.wizard.title")} onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), cancel())}>
  <p><strong>{t("hosting.wizard.title")}</strong> <span class="hint">{t("hosting.wizard.step", { n: step + 1, count: STEPS.length, name: t(`hosting.wizard.steps.${STEPS[step]}`) })}</span></p>

  {#if step === 0}
    <div class="grid">
      <label for="w-name">{t("hosting.wizard.name")}</label>
      <div>
        <input id="w-name" type="text" bind:value={name} spellcheck="false" maxlength="60" />
        {#if nameError}<span class="error">{nameError}</span>{:else}<span class="hint">{t("hosting.wizard.nameHint")}</span>{/if}
      </div>
      <span class="label">{t("hosting.wizard.build")}</span>
      <div class="seg" role="radiogroup" aria-label={t("hosting.wizard.build")} tabindex="-1" onkeydown={(e) => arrowPick(e, ["b42", "legacy41"], build, (v) => (build = v as typeof build))}>
        <button role="radio" aria-checked={build === "b42"} tabindex={build === "b42" ? 0 : -1} class:active={build === "b42"} onclick={() => (build = "b42")}>{t("hosting.wizard.b42")}</button>
        <button role="radio" aria-checked={build === "legacy41"} tabindex={build === "legacy41" ? 0 : -1} class:active={build === "legacy41"} onclick={() => (build = "legacy41")}>{t("hosting.wizard.b41")}</button>
      </div>
      <label for="w-install">{t("hosting.wizard.install")}</label>
      <div class="row">
        <input id="w-install" type="text" bind:value={installDir} placeholder={defaultInstall} spellcheck="false" />
        <button onclick={() => pick("install")}><Icon name="folder" size={14} />{t("hosting.wizard.browse")}</button>
      </div>
      <span class="label">{t("hosting.wizard.data")}</span>
      <div>
        <label class="check"><input type="radio" name="w-data" checked={!ownData} onchange={() => (ownData = false)} />{t("hosting.wizard.dataZomboid", { dir: host.zomboid })}</label>
        <label class="check"><input type="radio" name="w-data" checked={ownData} onchange={() => (ownData = true)} />{t("hosting.wizard.dataOwn")}</label>
        {#if ownData}
          <div class="row">
            <input type="text" bind:value={dataDir} aria-label={t("hosting.wizard.dataOwn")} spellcheck="false" />
            <button onclick={() => pick("data")}><Icon name="folder" size={14} />{t("hosting.wizard.browse")}</button>
          </div>
        {/if}
        <span class="hint">{t("hosting.wizard.dataHint")}</span>
      </div>
    </div>
  {:else if step === 1}
    <div class="grid">
      <label for="w-port">{t("hosting.wizard.gamePort")}</label>
      <div class="row">
        <input id="w-port" type="number" min="1024" max="65535" bind:value={gamePort} oninput={() => (ports = null)} />
        <label for="w-port2">{t("hosting.wizard.directPort")}</label>
        <input id="w-port2" type="number" min="1024" max="65535" bind:value={directPort} oninput={() => (ports = null)} />
        <button onclick={checkPorts} disabled={!!portError}><Icon name="search" size={14} />{t("hosting.wizard.checkPorts")}</button>
      </div>
      <span></span>
      <div>
        {#if portError}<span class="error">{portError}</span>
        {:else if ports}
          <span class={ports.every(Boolean) ? "hint" : "error"}>{ports.every(Boolean) ? t("hosting.wizard.portsFree") : t("hosting.wizard.portsBusy", { ports: new Intl.ListFormat(intlLang(), { type: "conjunction" }).format([gamePort, directPort].filter((_, i) => !ports?.[i]).map(String)) })}</span>
        {/if}
        <p class="hint">{t("hosting.wizard.firewall")}</p>
        <div class="row">
          <code class="mono cmd">{firewall}</code>
          <button onclick={() => copy(firewall, "fw")}><Icon name="copy" size={14} />{copied === "fw" ? t("hosting.wizard.copied") : t("hosting.wizard.copy")}</button>
        </div>
      </div>
      <label for="w-max">{t("hosting.wizard.maxPlayers")}</label>
      <input id="w-max" type="number" min="1" max="100" bind:value={maxPlayers} />
      <label for="w-join">{t("hosting.wizard.joinPassword")}</label>
      <div>
        <input id="w-join" type="text" bind:value={joinPassword} spellcheck="false" autocomplete="off" />
        <span class="hint">{t("hosting.wizard.joinHint")}</span>
      </div>
      <span class="label">{t("hosting.wizard.options")}</span>
      <div>
        <label class="check"><input type="checkbox" bind:checked={publicListing} />{t("hosting.wizard.public")}</label>
        <label class="check"><input type="checkbox" bind:checked={upnp} />{t("hosting.wizard.upnp")}</label>
        <span class="hint">{t("hosting.wizard.upnpHint")}</span>
      </div>
      <span class="label">{t("hosting.wizard.tunnelTitle")}</span>
      <div>
        <p class="hint">{t("hosting.wizard.tunnelHint", { game: gamePort, direct: directPort })}</p>
        <div class="row">
          <button onclick={() => api.openUrl("https://playit.gg/download")}><Icon name="external" size={14} />{t("hosting.wizard.tunnelDownload")}</button>
          <button onclick={() => api.openUrl("https://playit.gg/support/")}><Icon name="external" size={14} />{t("hosting.wizard.tunnelDocs")}</button>
        </div>
        <label for="w-addr">{t("hosting.wizard.tunnelAddress")}</label>
        <div class="row">
          <!-- i18n-ignore: an address example -->
          <input id="w-addr" type="text" bind:value={address} spellcheck="false" autocomplete="off" placeholder="example.gl.joinmc.link:12345" />
          <span class="hint">{t("hosting.wizard.tunnelAddressHint")}</span>
        </div>
      </div>
    </div>
  {:else if step === 2}
    <div class="grid">
      <label for="w-admin">{t("hosting.wizard.adminUser")}</label>
      <input id="w-admin" type="text" bind:value={adminUser} spellcheck="false" autocomplete="off" />
      <label for="w-pw">{t("hosting.wizard.adminPassword")}</label>
      <div>
        <div class="row">
          <input id="w-pw" class="mono" type="text" bind:value={adminPassword} spellcheck="false" autocomplete="off" />
          <button onclick={() => copy(adminPassword, "pw")}><Icon name="copy" size={14} />{copied === "pw" ? t("hosting.wizard.copied") : t("hosting.wizard.copy")}</button>
          <button onclick={() => (adminPassword = newPassword())}><Icon name="refresh" size={14} />{t("hosting.wizard.newPassword")}</button>
        </div>
        <span class="hint">{t("hosting.wizard.passwordHint")}</span>
      </div>
      <label for="w-mem">{t("hosting.wizard.memory")}</label>
      <div>
        <div class="row">
          <input id="w-mem" type="range" min="2" max={maxGb} step="0.5" bind:value={memoryGb} />
          <strong>{t("hosting.wizard.gb", { n: fmtNum(memoryGb) })}</strong>
        </div>
        <span class="hint">{t("hosting.wizard.memoryOf", { ram: fmtNum(ramGb) })}</span>
        <ul class="hint tips">
          <li>{t("hosting.wizard.memSmall")}</li>
          <li>{t("hosting.wizard.memMedium")}</li>
          <li>{t("hosting.wizard.memLarge")}</li>
        </ul>
      </div>
    </div>
  {:else}
    <div class="grid">
      <span class="label">{t("hosting.wizard.mods")}</span>
      <div>
        <label class="check"><input type="radio" name="w-mods" checked={modsFrom === "empty"} onchange={() => (modsFrom = "empty")} />{t("hosting.wizard.modsEmpty")}</label>
        <label class="check"><input type="radio" name="w-mods" checked={modsFrom === "profile"} onchange={() => (modsFrom = "profile")} />{t("hosting.wizard.modsProfile")}</label>
        {#if modsFrom === "profile"}
          <div class="row">
            <select bind:value={fromProfile} aria-label={t("hosting.wizard.modsProfile")}>
              {#each profiles as p}<option value={p}>{p}</option>{/each}
            </select>
            <button onclick={loadMods} disabled={loading}><Icon name="download" size={14} />{t("hosting.wizard.load")}</button>
          </div>
        {/if}
        <label class="check"><input type="radio" name="w-mods" checked={modsFrom === "collection"} onchange={() => (modsFrom = "collection")} />{t("hosting.wizard.modsCollection")}</label>
        {#if modsFrom === "collection"}
          <div class="row">
            <!-- i18n-ignore: a URL -->
            <input type="text" bind:value={collection} placeholder="https://steamcommunity.com/sharedfiles/filedetails/?id=…" aria-label={t("hosting.wizard.modsCollection")} spellcheck="false" />
            <button onclick={loadMods} disabled={loading || !collection.trim()}><Icon name="download" size={14} />{loading ? t("hosting.wizard.loading") : t("hosting.wizard.load")}</button>
          </div>
        {/if}
        {#if modsFrom !== "empty" && loadedFor === source && (lines.mods.length || lines.workshop.length)}
          <p class="hint">{t("hosting.wizard.modsLoaded", { mods: t("hosting.wizard.modIds", { count: lines.mods.length }), items: t("hosting.wizard.items", { count: lines.workshop.length }), maps: t("hosting.wizard.maps", { count: lines.maps.length }) })}</p>
        {:else if modsFrom !== "empty" && loadedFor && loadedFor !== source}
          <p class="hint">{t("hosting.wizard.loadAgain")}</p>
        {/if}
        {#if loadedFor === source && notes.length}<ul class="hint tips">{#each notes as n}<li>{n}</li>{/each}</ul>{/if}
        <span class="hint">{t("hosting.wizard.modsHint")}</span>
      </div>
      <span class="label">{t("hosting.wizard.summary")}</span>
      <ul class="tips">
        <li>{t("hosting.wizard.sumInstall", { dir: installDir.trim() || defaultInstall })}</li>
        <li>{t("hosting.wizard.sumConfig", { file: `${ownData ? dataDir.trim() : host.zomboid}/Server/${name.trim()}.ini` })}</li>
        <li>{t("hosting.wizard.sumPorts", { game: gamePort, direct: directPort })}</li>
        {#if address.trim()}<li>{t("hosting.wizard.sumAddress", { address: address.trim() })}</li>{/if}
      </ul>
    </div>
    {#if creating}
      <div class="row progress">
        {#if progress !== null && progress < 100}
          <progress max="100" value={progress} aria-label={t("common.installing")}></progress><span class="hint">{t("hosting.wizard.installing", { n: progress })}</span>
          <button onclick={() => api.hostInstallStop().catch((e) => (error = t(...keyedError(e))))}><Icon name="x" size={14} />{t("hosting.wizard.stopInstall")}</button>
        {:else}
          <span class="hint">{t("hosting.wizard.starting")}</span>
        {/if}
      </div>
    {/if}
  {/if}

  {#if error}<p class="error">{error}</p>{/if}
  {#if created}<p class="hint">{t("hosting.wizard.createdHint", { name: name.trim() })}</p>{/if}
  <div class="row nav">
    {#if created}
      <button class="primary" onclick={() => onDone(name.trim())}><Icon name="server" size={14} />{t("hosting.wizard.openServer")}</button>
    {:else if step < STEPS.length - 1}
      <button class="primary" onclick={() => step++} disabled={!stepOk}><Icon name="chevron-right" size={14} />{t("hosting.wizard.next")}</button>
    {:else}
      <button class="primary" onclick={create} disabled={creating || loading || (modsFrom !== "empty" && (loadedFor !== source || (!lines.mods.length && !lines.workshop.length)))}><Icon name="play" size={14} />{t("hosting.wizard.create")}</button>
    {/if}
    {#if step > 0 && !created}<button onclick={() => step--} disabled={creating}><Icon name="chevron-left" size={14} />{t("hosting.wizard.back")}</button>{/if}
    {#if !created}<button class="quiet" onclick={cancel} disabled={creating}><Icon name="x" size={14} />{t("common.cancel")}</button>{/if}
  </div>
</div>

<style>
  .wizard {
    margin: 12px 0;
    padding: 14px 16px;
    background: var(--raised);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  .wizard p {
    margin: 0 0 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 10px 14px;
    align-items: baseline;
  }
  .grid > .seg {
    justify-self: start;
  }
  .grid > label,
  .label {
    color: var(--muted);
  }
  .grid input[type="text"] {
    width: min(100%, 420px);
  }
  .grid > div > .hint,
  .grid > div > .error {
    display: block;
    margin-top: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .row input[type="text"] {
    flex: 1;
  }
  input[type="number"] {
    width: 90px;
    background: var(--raised);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 8px;
    font: inherit;
    color: inherit;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  .cmd {
    flex: 1;
    min-width: 0;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--panel);
    overflow-wrap: anywhere;
  }
  .tips {
    margin: 4px 0;
    padding-left: 18px;
  }
  input[type="range"] {
    accent-color: var(--accent);
  }
  .progress progress {
    flex: 1;
    accent-color: var(--accent);
  }
  .nav {
    margin-top: 14px;
  }
</style>
