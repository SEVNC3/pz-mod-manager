<script lang="ts">
  import type { GameServer, SetupItem } from "./api";
  import { fmtSize } from "./format";
  import { t } from "./i18n.svelte";
  import { thumb } from "./thumb";
  import Icon from "./Icon.svelte";
  import { trap } from "./ui";

  // Setting up a server: its missing mods, read before anything downloads or the mod list changes. Every
  // join that sets a server up asks here (Home, Servers), so mods never land in a list unasked.
  let { server, ids, items, unlisted = 0, onAnswer }: {
    server: GameServer;
    /** The Workshop items to download. */
    ids: string[];
    /** Their details (title, image, size, whether it's a guess); null while asked. */
    items: SetupItem[] | null;
    /** Mods the server runs but doesn't list: nothing here gets them. */
    unlisted?: number;
    /** The items to download and the server's mod IDs to leave out of the profile (the guesses ticked off), or null: cancelled. */
    onAnswer: (answer: { items: string[]; dropMods: string[] } | null) => void;
  } = $props();

  const of = $derived(new Map((items ?? []).map((i) => [i.id, i])));
  /** Probable matches ticked off: not downloaded. */
  let off = $state<Record<string, boolean>>({});
  const chosen = $derived(ids.filter((id) => !off[id]));
  const size = $derived(chosen.reduce((n, id) => n + (of.get(id)?.file_size ?? 0), 0));
  const dropMods = $derived(ids.filter((id) => off[id]).flatMap((id) => of.get(id)?.mods ?? []));
</script>

<div class="backdrop" onclick={() => onAnswer(null)} aria-hidden="true"></div>
<!-- the Join that opened it was disabled while it asked: focus goes back to Join -->
<div class="setup" role="dialog" aria-modal="true" aria-labelledby="setup-title" tabindex="-1" use:trap={{ self: true, fallback: ".playbar .big, .details .plan .primary" }} onkeydown={(e) => e.key === "Escape" && onAnswer(null)}>
  <h2 id="setup-title">{t("serverSetup.heading", { name: server.name })}</h2>
  <p>{t("serverSetup.title", { count: chosen.length })}</p>
  <ul>
    {#each ids as id (id)}
      {@const it = of.get(id)}
      {@const name = it?.title || id}
      <li>
        {#if it?.preview_url}<img use:thumb={t("common.imageFailed")} class="thumb" src={it.preview_url} alt="" loading="lazy" />
        {:else}<span class="thumb blank" aria-hidden="true">{items ? name.charAt(0) : ""}</span>{/if}
        <span class="name" title={name}>{items ? name : "…"}</span>
        {#if it?.likely}
          <!-- a probable match, not the server's named mod: downloaded only while ticked -->
          <label class="guess" title={t("serverSetup.guessTip")}><input type="checkbox" checked={!off[id]} onchange={(e) => (off[id] = !e.currentTarget.checked)} />{t("serverSetup.guess")}</label>
        {/if}
        {#if it?.file_size}<span class="size">{fmtSize(it.file_size)}</span>{/if}
      </li>
    {/each}
  </ul>
  {#if unlisted}<p class="note">{t("serverSetup.unlisted", { count: unlisted })}</p>{/if}
  <p class="note">{t("serverSetup.note")}</p>
  <div class="btns">
    <button class="primary" onclick={() => onAnswer({ items: chosen, dropMods })}><Icon name="download" size={14} />{size ? t("serverSetup.goSize", { size: fmtSize(size) }) : t("serverSetup.go")}</button>
    <button class="quiet" onclick={() => onAnswer(null)}><Icon name="x" size={14} />{t("common.cancel")}</button>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: rgb(0 0 0 / 0.45);
  }
  .setup {
    position: fixed;
    z-index: 21;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    width: min(520px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px 22px;
    border-radius: 10px;
    background: var(--panel);
    box-shadow: var(--shadow);
  }
  .setup:focus {
    outline: none;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  p {
    margin: 0;
  }
  ul {
    flex: 0 1 auto;
    min-height: 0;
    overflow-y: auto;
    margin: 0;
    padding: 4px 0;
    list-style: none;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 0;
  }
  .thumb {
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 6px;
    object-fit: cover;
    background: var(--raised);
  }
  .thumb.blank {
    display: grid;
    place-items: center;
    box-sizing: border-box;
    border: 1px solid var(--line);
    font-family: var(--font-marker);
    font-size: 15px;
    color: var(--muted);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .size,
  .note {
    font-size: 12px;
    color: var(--muted);
  }
  .size,
  .guess {
    flex: none;
  }
  .guess {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--warn);
  }
  .btns {
    display: flex;
    gap: 8px;
  }
</style>
