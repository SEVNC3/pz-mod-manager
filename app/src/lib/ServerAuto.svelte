<script lang="ts">
  import { track } from "./activity.svelte";
  import { onMount } from "svelte";
  import { api, type ServerAuto, type ServerCard, type ServerEvent24 } from "./api";
  import { fmtDateTime } from "./format";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import { wholeNumbers } from "./ui";

  /** `dirty` (out): changes not saved yet; `saveAll` (out): save them, true only when saved (the leave box's Save and leave). */
  let { server, onSaved, dirty = $bindable(false), saveAll = $bindable() }: { server: ServerCard; onSaved: () => void; dirty?: boolean; saveAll?: () => Promise<boolean> } = $props();

  const UPDATES = ["always", "empty", "scheduled", "notify"] as const;
  const EVENTS = ["started", "stopped", "crashed", "restarting", "updated", "joined", "left", "status"];
  /** "00" to "59": a restart time's hours (the first 24) and minutes. */
  const CLOCK = Array.from({ length: 60 }, (_, i) => String(i).padStart(2, "0"));
  // svelte-ignore state_referenced_locally
  let a = $state<ServerAuto>(structuredClone($state.snapshot(server.auto)));
  // svelte-ignore state_referenced_locally
  let warnText = $state(server.auto.warn_minutes.join(", "));
  let webhook = $state("");
  let botToken = $state("");
  let error = $state("");
  let done = $state("");
  let working = $state(false);
  let checked = $state<string[] | null>(null);
  let history = $state<ServerEvent24[]>([]);

  const warnings = $derived(warnText.split(/[,\s]+/).map(Number).filter((n) => n > 0));
  const edited = $derived(JSON.stringify({ ...a, warn_minutes: warnings }) !== JSON.stringify(server.auto));
  $effect(() => {
    dirty = edited;
  });

  /** Whether it was done. */
  async function run(f: () => Promise<unknown>, ok: string) {
    error = done = "";
    working = true;
    try {
      await f();
      done = ok;
      onSaved();
      return true;
    } catch (e) {
      error = t(...keyedError(e));
      return false;
    } finally {
      working = false;
    }
  }

  const save = () =>
    run(
      () => api.hostAutoSet(server.name, { ...a, warn_minutes: warnings, restarts: a.restarts.filter(Boolean), broadcasts: a.broadcasts.filter((b) => b.text.trim()) }),
      t("hosting.auto.saved"),
    );
  saveAll = save;

  onMount(() => {
    api.hostHistory(server.name).then((h) => (history = h)).catch(() => {});
  });

  async function checkNow() {
    error = "";
    working = true;
    try {
      checked = await api.hostCheckUpdates(server.name);
      onSaved();
    } catch (e) {
      error = t(...keyedError(e));
    } finally {
      working = false;
    }
  }

  function setWebhook(url: string) {
    run(async () => {
      await api.hostDiscordSet(server.name, url);
      a.discord.saved = !!url.trim();
      webhook = "";
    }, url.trim() ? t("hosting.auto.webhookSaved") : t("hosting.auto.webhookForgotten"));
  }

  function setBot(token: string) {
    run(async () => {
      await api.hostDiscordBotSet(server.name, token);
      a.discord.bot = !!token.trim();
      botToken = "";
    }, token.trim() ? t("hosting.auto.botSaved") : t("hosting.auto.botForgotten"));
  }

  const toggle = (e: string, on: boolean) => (a.discord.events = on ? [...a.discord.events, e] : a.discord.events.filter((x) => x !== e));

  // the activity bar moves while this tab works
  $effect(() => {
    if (working) return track();
  });
</script>

<div class="auto" use:wholeNumbers>
  <p class="hint"><Icon name="info" size={14} /> {t("hosting.auto.whileOpen")} {t("hosting.auto.trayHint")}</p>

  <section>
    <h3>{t("hosting.auto.updatesTitle")}</h3>
    <div class="radios" role="radiogroup" aria-label={t("hosting.auto.updatesTitle")}>
      {#each UPDATES as u}
        <label class="check"><input type="radio" name="updates" value={u} bind:group={a.updates} />{t(`hosting.auto.updates.${u}`)}</label>
      {/each}
    </div>
    {#if a.updates === "scheduled" && !a.restarts.length}<p class="hint warn"><Icon name="alert" size={14} /> {t("hosting.auto.noTimes")}</p>{/if}
    <div class="row">
      <label for="a-check">{t("hosting.auto.checkEvery")}</label>
      <input id="a-check" type="number" min="0" max="1440" bind:value={a.check_minutes} />
      <span class="hint">{t("hosting.auto.minutesOff")}</span>
      <button onclick={checkNow} disabled={working}><Icon name="refresh" size={14} />{t("hosting.auto.checkNow")}</button>
    </div>
    {#if checked}
      <p class="hint" role="status">{checked.length ? t("hosting.auto.updatedNow", { count: checked.length, titles: new Intl.ListFormat(intlLang(), { type: "conjunction" }).format(checked) }) : t("hosting.auto.allCurrent")}</p>
    {/if}
    <div class="row">
      <label for="a-warn">{t("hosting.auto.warnAt")}</label>
      <input id="a-warn" type="text" class="short" bind:value={warnText} />
      <span class="hint">{t("hosting.auto.warnHint")}</span>
    </div>
  </section>

  <section>
    <h3>{t("hosting.auto.scheduleTitle")}</h3>
    <div class="row">
      <span>{t("hosting.auto.restartsAt")}</span>
      {#each a.restarts as time, i}
        {@const [h, m] = time.split(":")}
        <span class="time" role="group" aria-label={t("hosting.auto.restartTime")}>
          <select value={h} onchange={(e) => (a.restarts[i] = `${e.currentTarget.value}:${m}`)} aria-label={t("hosting.auto.restartHour")}>
            {#each CLOCK.slice(0, 24) as v}<option>{v}</option>{/each}
          </select>:<select value={m} onchange={(e) => (a.restarts[i] = `${h}:${e.currentTarget.value}`)} aria-label={t("hosting.auto.restartMinute")}>
            {#each CLOCK as v}<option>{v}</option>{/each}
          </select>
          <button class="quiet" onclick={() => a.restarts.splice(i, 1)} title={t("common.remove")} aria-label={t("common.remove")}><Icon name="x" size={14} /></button>
        </span>
      {/each}
      <button onclick={() => a.restarts.push("06:00")}><Icon name="plus" size={14} />{t("hosting.auto.addTime")}</button>
    </div>
    <div class="row">
      <label for="a-every">{t("hosting.auto.restartEvery")}</label>
      <input id="a-every" type="number" min="0" max="168" bind:value={a.restart_hours} />
      <span class="hint">{t("hosting.auto.everyHint")}</span>
    </div>
    <div class="row">
      <label for="a-save">{t("hosting.auto.saveEvery")}</label>
      <input id="a-save" type="number" min="0" max="1440" bind:value={a.save_minutes} />
      <span class="hint">{t("hosting.auto.saveHint")}</span>
    </div>
    <div class="row">
      <label for="a-backup">{t("hosting.auto.backupEvery")}</label>
      <input id="a-backup" type="number" min="0" max="168" bind:value={a.backup_hours} />
      <span class="hint">{t("hosting.auto.hoursOff")}</span>
    </div>
    <div class="row">
      <label for="a-keep">{t("hosting.auto.keep")}</label>
      <input id="a-keep" type="number" min="1" max="1000" bind:value={a.backup_keep} />
      <label for="a-max">{t("hosting.auto.maxMb")}</label>
      <input id="a-max" type="number" min="0" step="512" bind:value={a.backup_max_mb} />
      <span class="hint">{t("hosting.auto.mbOff")}</span>
    </div>
    <label class="check"><input type="checkbox" bind:checked={a.backup_on_start} />{t("hosting.auto.backupOnStart")}</label>
  </section>

  <section>
    <h3>{t("hosting.auto.messagesTitle")}</h3>
    {#each a.broadcasts as b, i}
      <div class="row">
        <input type="text" class="grow" bind:value={b.text} maxlength="200" placeholder={t("hosting.auto.messageText")} aria-label={t("hosting.auto.messageText")} />
        <label for="a-msg-{i}">{t("hosting.auto.messageEvery")}</label>
        <input id="a-msg-{i}" type="number" min="1" max="1440" bind:value={b.minutes} />
        <button class="quiet" onclick={() => a.broadcasts.splice(i, 1)} title={t("common.remove")} aria-label={t("common.remove")}><Icon name="x" size={14} /></button>
      </div>
    {/each}
    <div class="row">
      <button onclick={() => a.broadcasts.push({ minutes: 60, text: "" })}><Icon name="plus" size={14} />{t("hosting.auto.addMessage")}</button>
      <span class="hint">{t("hosting.auto.messageHint")}</span>
    </div>
  </section>

  <section>
    <h3>{t("hosting.auto.crashTitle")}</h3>
    <div class="row">
      <label for="a-crash">{t("hosting.auto.crashRestarts")}</label>
      <input id="a-crash" type="number" min="0" max="20" bind:value={a.crash_restarts} />
      <span class="hint">{t("hosting.auto.crashHint")}</span>
    </div>
  </section>

  <section>
    <h3>{t("hosting.auto.discordTitle")}</h3>
    <div class="row">
      <input type="password" class="grow" bind:value={webhook} autocomplete="off" spellcheck="false" aria-label={t("hosting.auto.webhook")}
        placeholder={a.discord.saved ? t("hosting.auto.webhookSavedPlaceholder") : t("hosting.auto.webhookPlaceholder")} />
      <button onclick={() => setWebhook(webhook)} disabled={working || !webhook.trim()}><Icon name="check" size={14} />{t("hosting.auto.webhookSave")}</button>
      {#if a.discord.saved}
        <button onclick={() => run(() => api.hostDiscordTest(server.name), t("hosting.auto.testSent"))} disabled={working}><Icon name="message" size={14} />{t("hosting.auto.test")}</button>
        <button class="danger" onclick={() => setWebhook("")} disabled={working}><Icon name="trash" size={14} />{t("hosting.auto.webhookForget")}</button>
      {/if}
    </div>
    <p class="hint">{t("hosting.auto.webhookHint")}</p>
    <div class="events">
      {#each EVENTS as e}
        <label class="check"><input type="checkbox" checked={a.discord.events.includes(e)} onchange={(ev) => toggle(e, ev.currentTarget.checked)} />{t(`hosting.auto.event.${e}`)}</label>
      {/each}
    </div>
    <h4>{t("hosting.auto.botTitle")}</h4>
    <p class="hint">{t("hosting.auto.botIntro")}</p>
    <div class="row">
      <input type="password" class="grow" bind:value={botToken} autocomplete="off" spellcheck="false" aria-label={t("hosting.auto.botToken")}
        placeholder={a.discord.bot ? t("hosting.auto.botSavedPlaceholder") : t("hosting.auto.botToken")} />
      <button onclick={() => setBot(botToken)} disabled={working || !botToken.trim()}><Icon name="check" size={14} />{t("hosting.auto.botSave")}</button>
      {#if a.discord.bot}
        <button onclick={() => run(() => api.hostDiscordBotTest(server.name), t("hosting.auto.testSent"))} disabled={working || edited || !a.discord.channel} title={edited ? t("hosting.auto.botSaveFirst") : ""}><Icon name="message" size={14} />{t("hosting.auto.test")}</button>
        <button class="danger" onclick={() => setBot("")} disabled={working}><Icon name="trash" size={14} />{t("hosting.auto.webhookForget")}</button>
      {/if}
    </div>
    <div class="row">
      <label for="a-channel">{t("hosting.auto.botChannel")}</label>
      <input id="a-channel" type="text" class="id" bind:value={a.discord.channel} inputmode="numeric" spellcheck="false" autocomplete="off" />
      <label for="a-role">{t("hosting.auto.botRole")}</label>
      <input id="a-role" type="text" class="id" bind:value={a.discord.role} inputmode="numeric" spellcheck="false" autocomplete="off" />
    </div>
    <p class="hint">{t("hosting.auto.botHint")}</p>
    <label class="check" title={server.shares_logs ? t("hosting.auto.botRelayShared") : ""}><input type="checkbox" bind:checked={a.discord.relay} disabled={server.shares_logs} />{t("hosting.auto.botRelay")}</label>
    <p class="hint">{t("hosting.auto.botRelayHint")}</p>
  </section>

  <div class="row">
    <button class="primary" onclick={save} disabled={working || !edited}><Icon name="check" size={14} />{t("hosting.auto.save")}</button>
    {#if done}<span class="hint" role="status">{done}</span>{/if}
  </div>
  {#if error}<p class="error">{error}</p>{/if}

  <section class="history">
    <h3>{t("hosting.auto.historyTitle")}</h3>
    {#if !history.length}
      <p class="hint">{t("hosting.auto.historyNone")}</p>
    {:else}
      <ul aria-label={t("hosting.auto.historyTitle")}>
        {#each history as h}
          <li><span class="hint">{fmtDateTime(h.time)}</span> {h.kind === "restart" && h.text ? t("hosting.auto.history.restartTook", { text: h.text }) : t(`hosting.auto.history.${h.kind}`, { text: h.text })}</li>
        {/each}
      </ul>
    {/if}
  </section>
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
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 6px 0;
  }
  .radios,
  .events {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 18px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  input[type="number"] {
    width: 80px;
  }
  .short {
    width: 110px;
  }
  .id {
    width: 190px;
  }
  h4 {
    margin: 14px 0 6px;
    font-size: 14px;
  }
  .grow {
    flex: 1 1 320px;
  }
  .time {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .warn {
    color: var(--danger);
  }
  .history {
    margin-top: 16px;
    border-bottom: none;
  }
  .history ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .history li {
    padding: 2px 0;
    overflow-wrap: anywhere;
  }
</style>
