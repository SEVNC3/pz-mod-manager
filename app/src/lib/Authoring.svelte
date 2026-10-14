<script lang="ts">
  import { track } from "./activity.svelte";
  import { onDestroy } from "svelte";
  import { api, type AuthoringReport, type Built, type Finding, type Upload } from "./api";
  import { intlLang, keyedError, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";

  const VISIBILITY = ["public", "friendsOnly", "private", "unlisted"];
  /** For comparing Windows paths. */
  const norm = (p: string) => p.replaceAll("\\", "/").toLowerCase();
  /** A typed or pasted path without its surrounding whitespace and quotes ("Copy as path" adds them). */
  const unquote = (p: string) => p.trim().replace(/^"(.*)"$/, "$1").trim();
  /** Drive (C:\), UNC (\\server) or Unix root: a relative path would resolve against the app's own folder. */
  const absolute = (p: string) => /^([a-z]:[\\/]|\\\\|\/)/i.test(p);

  let dir = $state("");
  /** The folder `report` is for; Make a mod builds this one, not whatever the path box holds now. */
  let checked = $state("");
  let parent = $state("");
  let report = $state<AuthoringReport | null>(null);
  let upload = $state<Upload>({ title: "", description: "", tags: [], visibility: "public", id: "" });
  let tagText = $state("");
  /** The mod's poster made 256×256 for preview.png, and a URL to show it. */
  let preview = $state<{ bytes: number[]; url: string } | null>(null);
  let built = $state<Built | null>(null);
  let error = $state("");
  let working = $state(false);

  async function pickMod() {
    const p = await api.pickFolder(dir || report?.workshop || "");
    if (p) {
      dir = p;
      await check();
    }
  }

  async function check() {
    dir = unquote(dir);
    report = null;
    built = null;
    if (!absolute(dir)) return void (error = t("authoring.notAbsolute"));
    working = true;
    error = "";
    try {
      report = await api.authoringCheck(dir);
      checked = dir;
      upload = report.draft;
      if (!parent && report.workshop) parent = report.workshop;
      tagText = upload.tags.join("; ");
      await makePreview();
    } catch (e) {
      error = t(...keyedError(e));
    }
    working = false;
  }

  /** The poster, cropped to a square from its centre and scaled to 256×256 (the size the uploader demands). */
  async function makePreview() {
    if (preview) URL.revokeObjectURL(preview.url);
    preview = null;
    if (!report?.has_poster) return;
    const buf = await api.authoringPoster(checked);
    if (!buf.byteLength) return;
    let img: ImageBitmap;
    try {
      img = await createImageBitmap(new Blob([buf]));
    } catch {
      error = t("authoring.posterUnreadable");
      return;
    }
    const side = Math.min(img.width, img.height);
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 256;
    canvas.getContext("2d")!.drawImage(img, (img.width - side) / 2, (img.height - side) / 2, side, side, 0, 0, 256, 256);
    const blob = await new Promise<Blob | null>((ok) => canvas.toBlob(ok, "image/png"));
    if (blob) preview = { bytes: [...new Uint8Array(await blob.arrayBuffer())], url: URL.createObjectURL(blob) };
  }
  onDestroy(() => preview && URL.revokeObjectURL(preview.url));

  async function pickParent() {
    const p = await api.pickFolder(parent || report?.workshop || "");
    if (p) parent = p;
  }

  function toggleTag(tag: string) {
    upload.tags = upload.tags.includes(tag) ? upload.tags.filter((x) => x !== tag) : [...upload.tags, tag];
  }

  async function build() {
    parent = unquote(parent);
    if (!absolute(parent)) return void (error = t("authoring.notAbsolute"));
    working = true;
    error = "";
    try {
      const tags = report?.tags.length ? upload.tags : tagText.split(";").map((s) => s.trim()).filter(Boolean);
      built = await api.authoringBuild(checked, parent, { ...upload, tags }, preview?.bytes ?? null);
    } catch (e) {
      error = t(...keyedError(e));
    }
    working = false;
  }

  // the activity bar moves while this tab works
  $effect(() => {
    if (working) return track();
  });
</script>

{#snippet findings(list: Finding[])}
  {#if list.length}
    <ul class="findings">
      {#each list as f}
        <li class={f.level}><Icon name={f.level === "error" ? "error" : "alert"} size={14} /><span><span class="mono">{f.subject}</span> {t(`authoring.check.${f.key}`, f.vars)}</span></li>
      {/each}
    </ul>
  {:else}
    <p class="ok"><Icon name="check" size={14} /> {t("authoring.noProblems")}</p>
  {/if}
{/snippet}

<section class="authoring blueprint" aria-label={t("authoring.title")}>
  <div class="inner">
    <h2>{t("authoring.title")}</h2>
    <p class="lede">{t("authoring.lede")}</p>

    <!-- a typed or pasted path is checked on Enter -->
    <form class="row" onsubmit={(e) => (e.preventDefault(), unquote(dir) && check())}>
      <!-- an edited path drops the old folder's report, so nothing below can act on a folder the box no longer names -->
      <input type="text" class="mono path" bind:value={dir} oninput={() => unquote(dir) !== checked ? (report = built = null) : null} spellcheck="false" placeholder={t("authoring.modPlaceholder")} aria-label={t("authoring.modLabel")} />
      <button type="button" class={dir ? "" : "primary"} onclick={pickMod} disabled={working}><Icon name="folder" size={14} />{t("authoring.chooseMod")}</button>
      {#if dir}<button type="submit" disabled={working}><Icon name="refresh" size={14} />{t("authoring.checkAgain")}</button>{/if}
    </form>
    {#if error}<p class="error">{error}</p>{/if}

    {#if report}
      <h3>{t(report.upload ? "authoring.uploadCheck" : "authoring.modCheck")}</h3>
      {@render findings(report.findings)}

      {#if !report.upload}
        <h3>{t("authoring.uploadTitle")}</h3>
        <p class="hint">{t("authoring.uploadIntro")}</p>
        <div class="form">
          <label for="ws-title">{t("authoring.itemTitle")}</label>
          <input id="ws-title" type="text" bind:value={upload.title} maxlength="128" />
          <label for="ws-desc">{t("authoring.description")}</label>
          <textarea id="ws-desc" rows="6" bind:value={upload.description} maxlength="8000"></textarea>
          <span class="label" id="ws-tags">{t("authoring.tags")}</span>
          {#if report.tags.length}
            <div class="tags" role="group" aria-labelledby="ws-tags">
              {#each report.tags as tag}
                <label class="tag"><input type="checkbox" checked={upload.tags.includes(tag)} onchange={() => toggleTag(tag)} />{tag}</label>
              {/each}
            </div>
          {:else}
            <input type="text" bind:value={tagText} aria-labelledby="ws-tags" placeholder={t("authoring.tagsPlaceholder")} />
          {/if}
          <label for="ws-vis">{t("authoring.visibility")}</label>
          <select id="ws-vis" bind:value={upload.visibility}>
            {#each VISIBILITY as v}<option value={v}>{t(`authoring.vis.${v}`)}</option>{/each}
          </select>
          <label for="ws-id">{t("authoring.itemId")}</label>
          <div>
            <input id="ws-id" class="mono" type="text" inputmode="numeric" bind:value={upload.id} />
            <p class="hint">{t("authoring.itemIdHint")}</p>
          </div>
          <span class="label">{t("authoring.preview")}</span>
          <div>
            {#if preview}
              <img src={preview.url} width="128" height="128" alt={t("authoring.previewAlt")} />
              <p class="hint">{t("authoring.previewFromPoster")}</p>
            {:else}
              <p class="hint">{t("authoring.noPoster")}</p>
            {/if}
          </div>
          <span class="label">{t("authoring.saveIn")}</span>
          <div class="row">
            <input type="text" class="mono path" bind:value={parent} spellcheck="false" placeholder={t("authoring.saveInPlaceholder")} aria-label={t("authoring.saveIn")} />
            <button onclick={pickParent} disabled={working}><Icon name="folder" size={14} />{t("authoring.choose")}</button>
          </div>
        </div>
        <div class="row">
          <button class="primary" onclick={build} disabled={working || !parent || !upload.title.trim()}><Icon name="upload" size={14} />{t("authoring.build")}</button>
        </div>
        {#if built}
          <p class="ok">
            <Icon name="check" size={14} /> {t("authoring.built", { folder: built.folder })}
            <button class="quiet link" onclick={() => api.openPath(built!.folder)}>{t("authoring.showFolder")}</button>
          </p>
          {#if built.skipped.length}
            <p class="hint">{t("authoring.skipped", { count: built.skipped.length, files: new Intl.ListFormat(intlLang(), { type: "unit" }).format(built.skipped) })}</p>
          {/if}
          {@render findings(built.findings)}
          {#if report.workshop && !norm(built.folder).startsWith(norm(report.workshop))}
            <p class="error">{t("authoring.notInWorkshop", { workshop: report.workshop })}</p>
          {/if}
          <p class="hint">{t("authoring.nextStep")}</p>
        {/if}
      {/if}
    {/if}
  </div>
</section>

<style>
  .authoring {
    overflow-y: auto;
    min-height: 0;
  }
  .inner {
    width: min(1180px, calc(100% - 32px));
    margin: 24px auto 40px;
    padding: 22px 26px 24px;
    background: var(--panel);
    border-radius: 8px;
    box-shadow: var(--card-shadow);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 22px;
  }
  h3 {
    margin: 22px 0 8px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    margin: 10px 0;
  }
  .path {
    flex: 1;
    min-width: 240px;
  }
  .hint {
    color: var(--muted);
    margin: 4px 0;
  }
  .findings {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 6px;
  }
  .findings li {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    overflow-wrap: anywhere;
  }
  .findings li :global(.icon) {
    margin-top: 3px;
  }
  .findings li.error :global(.icon) {
    color: var(--danger);
  }
  .findings li.warning :global(.icon),
  .ok :global(.icon) {
    color: var(--muted);
  }
  .form {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 10px 14px;
    align-items: start;
  }
  .form > label,
  .label {
    padding-top: 5px;
    font-weight: 700;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
  }
  .tag {
    display: flex;
    gap: 5px;
    align-items: center;
  }
  img {
    border-radius: 4px;
    border: 1px solid var(--line);
  }
</style>
