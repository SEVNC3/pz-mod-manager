<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type Ftp, type FtpEntry, type ServerBackup, type ServerCard } from "./api";
  import { confirmed } from "./ask.svelte";
  import { fmtDateTime, fmtNum, fmtSize } from "./format";
  import { keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";

  let { server, onChanged }: { server: ServerCard; onChanged: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let f = $state<Ftp>(server.ftp ? { ...server.ftp } : { host: (server.remote ?? "").replace(/:\d+$/, ""), port: 21, user: "", plain: false, folder: "", server: "servertest", pulled: 0, fingerprint: "" });
  let password = $state("");
  let dir = $state<string | null>(null);
  let entries = $state<FtpEntry[]>([]);
  let working = $state("");
  let error = $state("");
  /** The certificate the host showed when PZMM couldn't trust it: offered to trust. */
  let offered = $state("");
  /** The pinned certificate, when the host's changed from it: then trusting the new one asks first. */
  let pinned = $state("");
  let done = $state("");
  let progress = $state<[number, number] | null>(null);
  let backup = $state<ServerBackup | null>(null);

  const saved = $derived(!!server.ftp);
  const parent = (d: string) => d.replace(/\/[^/]+\/?$/, "") || "/";
  const into = (d: string, name: string) => (d.endsWith("/") ? d + name : `${d}/${name}`);

  async function run(what: string, job: () => Promise<unknown>, ok = "") {
    error = done = offered = pinned = "";
    working = what;
    try {
      await job();
      done = ok;
    } catch (e) {
      const [key, vars] = keyedError(e);
      error = t(key, vars);
      if (key === "errors.ftpUntrusted" || key === "errors.ftpCertChanged") offered = String(vars.fingerprint);
      if (key === "errors.ftpCertChanged") pinned = String(vars.old);
    } finally {
      working = "";
      progress = null;
    }
  }

  const connect = () =>
    run("connect", async () => {
      [dir, entries] = await api.hostFtpSet(server.name, { ...f, host: f.host.trim(), user: f.user.trim(), server: f.server.trim() }, password);
      password = "";
      onChanged();
    });

  const trust = async () => {
    if (pinned && !(await confirmed(t("hosting.files.trustNewTitle", { host: f.host }), { label: t("hosting.files.trustNewDo"), icon: "check" }, [t("hosting.files.trustOld", { fingerprint: pinned }), t("hosting.files.trustShown", { fingerprint: offered }), t("hosting.files.trustOnlyIf")]))) return;
    f.fingerprint = offered;
    connect();
  };

  const browse = (to: string) => run("browse", async () => ([dir, entries] = await api.hostFtpBrowse(server.name, to)));

  const useFolder = (d: string) =>
    run("connect", async () => {
      f.folder = d;
      await api.hostFtpSet(server.name, f, "");
      onChanged();
    }, t("hosting.files.folderSet", { folder: d }));

  const forget = () =>
    run("forget", async () => {
      await api.hostFtpForget(server.name);
      dir = null;
      onChanged();
    });

  /** Once got, PZMM's copies may hold edits not sent yet: ask before replacing them. */
  const pull = async () =>
    (!server.ftp?.pulled || (await confirmed(t("hosting.files.pullAsk"), { label: t("hosting.files.pull"), icon: "download" }, [t("hosting.files.pullLine")]))) &&
    run("pull", async () => {
      const n = await api.hostFtpPull(server.name);
      done = t("hosting.files.pulled", { count: n });
      onChanged();
    });

  const push = () =>
    run("push", async () => {
      const b = await api.hostFtpPush(server.name);
      done = b ? t("hosting.files.pushedBackup") : t("hosting.files.pushed");
    });

  const world = () => run("world", async () => (backup = await api.hostFtpWorld(server.name)));

  onMount(() => {
    const unlisten = listen<[number, number]>("progress", (e) => working === "world" && (progress = e.payload));
    return () => unlisten.then((u) => u());
  });
</script>

<div class="files">
  <section>
    <h3>{t("hosting.files.title")}</h3>
    <p class="hint">{t("hosting.files.intro")}</p>
    <form class="grid" onsubmit={(e) => { e.preventDefault(); connect(); }}>
      <label>{t("hosting.files.host")}<input type="text" bind:value={f.host} required spellcheck="false" /></label>
      <label>{t("hosting.files.port")}<input type="number" bind:value={f.port} min="1" max="65535" required /></label>
      <label>{t("hosting.files.user")}<input type="text" bind:value={f.user} required spellcheck="false" autocomplete="off" /></label>
      <label>{t("hosting.files.password")}<input type="password" bind:value={password} required={!saved} autocomplete="off" placeholder={saved ? t("hosting.files.passwordSaved") : ""} /></label>
      <label>{t("hosting.files.server")}<input type="text" bind:value={f.server} required spellcheck="false" /></label>
      <label class="check wide"><input type="checkbox" bind:checked={f.plain} />{t("hosting.files.plain")}</label>
      {#if f.plain}<p class="hint warn wide"><Icon name="alert" size={14} /> {t("hosting.files.plainWarn")}</p>{/if}
      <p class="hint wide">{t("hosting.files.serverHint")}</p>
      <div class="row wide">
        <button class="primary" type="submit" disabled={!!working}><Icon name="server" size={14} />{working === "connect" ? t("hosting.files.connecting") : t("hosting.files.connect")}</button>
        {#if saved}<button class="quiet danger" type="button" onclick={forget} disabled={!!working}><Icon name="trash" size={14} />{t("hosting.files.forget")}</button>{/if}
      </div>
    </form>
  </section>

  {#if dir !== null}
    <section>
      <h3>{t("hosting.files.folderTitle")}</h3>
      <p class="hint">{t("hosting.files.folderHint")}</p>
      <div class="row">
        <span class="mono">{dir}</span>
        <button onclick={() => dir && browse(parent(dir))} disabled={!!working || dir === "/"}><Icon name="chevron-up" size={14} />{t("hosting.files.up")}</button>
        <button onclick={() => dir && useFolder(dir)} disabled={!!working}><Icon name="check" size={14} />{t("hosting.files.useFolder")}</button>
      </div>
      <ul class="entries" aria-label={t("hosting.files.folderTitle")}>
        {#each entries as e}
          <li>
            {#if e.dir}
              <button class="quiet" onclick={() => dir && browse(into(dir, e.name))} disabled={!!working}><Icon name="folder" size={14} />{e.name}</button>
            {:else}
              <span class="hint file">{e.name} · {fmtSize(e.size)}</span>
            {/if}
          </li>
        {:else}
          <li class="hint">{t("hosting.files.emptyFolder")}</li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if server.ftp?.folder}
    <section>
      <h3>{t("hosting.files.settingsTitle")}</h3>
      <p class="hint">{t("hosting.files.settingsHint", { folder: server.ftp.folder })}</p>
      <div class="row">
        <button onclick={pull} disabled={!!working}><Icon name="download" size={14} />{t("hosting.files.pull")}</button>
        <button onclick={push} disabled={!!working || !server.ftp.pulled}><Icon name="upload" size={14} />{t("hosting.files.push")}</button>
        {#if server.ftp.pulled}<span class="hint">{t("hosting.files.lastPulled", { time: fmtDateTime(server.ftp.pulled) })}</span>{/if}
      </div>
      <p class="hint">{t("hosting.files.stopFirst")}</p>
    </section>
    <section>
      <h3>{t("hosting.files.worldTitle")}</h3>
      <p class="hint">{t("hosting.files.worldHint")}</p>
      <div class="row">
        <button onclick={world} disabled={!!working}><Icon name="download" size={14} />{t("hosting.files.world")}</button>
        {#if working === "world"}
          <button onclick={() => api.hostFtpStop().catch((e) => (error = t(...keyedError(e))))}><Icon name="x" size={14} />{t("hosting.files.stop")}</button>
          {#if progress}<span class="hint" role="status">{t("hosting.files.progress", { done: fmtNum(progress[0]), total: fmtNum(progress[1]) })}</span>{/if}
        {/if}
      </div>
      {#if backup}
        <div class="row" role="status">
          <span>{t("hosting.files.worldDone", { size: fmtSize(backup.size) })}</span>
          <button class="quiet" onclick={() => backup && api.openPath(backup.file.replace(/[\\/][^\\/]+$/, ""))}><Icon name="folder" size={14} />{t("hosting.files.openFolder")}</button>
        </div>
      {/if}
    </section>
  {/if}

  {#if done}<p class="hint" role="status">{done}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
  {#if offered}<button onclick={trust} disabled={!!working}><Icon name="check" size={14} />{t(pinned ? "hosting.files.trustNew" : "hosting.files.trust")}</button>{/if}
</div>

<style>
  section {
    margin-bottom: 16px;
    padding-bottom: 12px;
    border-bottom: 1px solid color-mix(in oklab, var(--line) 60%, transparent);
  }
  h3 {
    margin: 0 0 8px;
    font-size: 15px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 8px 12px;
    align-items: end;
  }
  .grid label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .grid .check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  .wide {
    grid-column: 1 / -1;
    margin: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 6px 0;
  }
  .warn {
    color: var(--danger);
  }
  .entries {
    list-style: none;
    padding: 0;
    margin: 6px 0 0;
    max-height: 260px;
    overflow: auto;
  }
  .entries button {
    justify-content: flex-start;
    gap: 6px;
  }
  /* a certificate's fingerprint is one long word */
  .error {
    overflow-wrap: anywhere;
  }
  .file {
    display: block;
    padding: 4px 8px;
  }
</style>
