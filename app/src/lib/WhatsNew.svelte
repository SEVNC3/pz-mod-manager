<script lang="ts" module>
  /** The release whatsNew.notes describe (in every locale): bump it with them. Another version shows no notes. */
  export const NOTES_FOR = "1.2.8";
</script>

<script lang="ts">
  import { api } from "./api";
  import { t } from "./i18n.svelte";
  import Modal from "./Modal.svelte";

  let { version, onClose }: { version: string; onClose: () => void } = $props();

  /** PZMM - Client on the Workshop: where a thumbs-up helps. */
  const STEAM = "https://steamcommunity.com/sharedfiles/filedetails/?id=3818460225";
  const notes = $derived(t("whatsNew.notes").split("\n").filter(Boolean));
</script>

<Modal title={t("whatsNew.title", { version })} width={520} {onClose}>
<div class="body">
  <ul>{#each notes as n}<li>{n}</li>{/each}</ul>
  <button class="banner" onclick={() => api.openUrl(STEAM)} title={t("whatsNew.upvoteButton")}>
    <span class="tape">{t("whatsNew.enjoying")}</span>
    <span class="ask">{t("whatsNew.upvote")}</span>
    <span class="click" aria-hidden="true">{t("whatsNew.clickMe")}</span>
    <!-- a zombie heart: bitten, stitched, still bleeding -->
    <svg class="heart" viewBox="0 0 100 100" aria-hidden="true">
      <defs>
        <mask id="bite">
          <rect width="100" height="100" fill="white" />
          <circle cx="92" cy="20" r="9" fill="black" />
          <circle cx="83" cy="9" r="8" fill="black" />
          <circle cx="97" cy="33" r="7" fill="black" />
        </mask>
      </defs>
      <path d="M50 90C20 68 4 50 4 31 4 16 15 6 28 6c10 0 18 6 22 14 4-8 12-14 22-14 13 0 24 10 24 25 0 19-16 37-46 59Z" fill="#6f8a4c" stroke="#141414" stroke-width="4" mask="url(#bite)" />
      <path d="M28 40l22 14 18-8" fill="none" stroke="#141414" stroke-width="3" stroke-linecap="round" />
      <path d="M33 37l-3 7M41 46l-3 7M49 51l3 6M57 49l-1 7M64 45l2 6" stroke="#141414" stroke-width="2.5" stroke-linecap="round" />
      <circle cx="30" cy="24" r="4" fill="#141414" />
      <path d="M50 90c0 5 3 7 3 10-3 0-5-3-3-10Z" fill="#8b0f0f" />
      <circle cx="74" cy="22" r="2.5" fill="#8b0f0f" /><circle cx="68" cy="15" r="1.6" fill="#8b0f0f" />
    </svg>
  </button>
</div>
</Modal>

<style>
  .body {
    overflow-y: auto;
    min-height: 0;
    padding: 14px 18px 16px;
  }
  ul {
    margin: 0 0 14px;
    padding-left: 18px;
  }
  li {
    margin: 2px 0;
  }
  /* the promo banners' look: dark grid, a strip of tape, marker writing, blood */
  .banner {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    width: 100%;
    padding: 14px 110px 14px 14px;
    border: 0;
    border-radius: 8px;
    text-align: left;
    color: #e8e8e6;
    background-color: #1c1c1c;
    background-image: linear-gradient(rgb(255 255 255 / 0.04) 1px, transparent 1px), linear-gradient(90deg, rgb(255 255 255 / 0.04) 1px, transparent 1px);
    background-size: 14px 14px;
    overflow: hidden;
    cursor: pointer;
  }
  .banner::after {
    /* a red slash behind the heart, like the skull's */
    content: "";
    position: absolute;
    right: 92px;
    top: -10px;
    bottom: -10px;
    width: 4px;
    background: #8b0f0f;
    rotate: 12deg;
  }
  .tape {
    padding: 3px 14px;
    font-family: var(--font-marker);
    font-size: 1.25em;
    color: #1c1c1c;
    background: #e6d6a6;
    rotate: -2deg;
    clip-path: polygon(2% 0, 100% 6%, 97% 100%, 0 92%);
  }
  .ask {
    font-size: 0.95em;
  }
  /* scrawled in the space beside the tape */
  .click {
    position: absolute;
    top: 14px;
    right: 116px;
    font-family: var(--font-marker);
    font-size: 1.1em;
    color: #c42020;
    rotate: -6deg;
  }
  .heart {
    position: absolute;
    right: 8px;
    top: 50%;
    width: 80px;
    height: 80px;
    translate: 0 -50%;
    rotate: 8deg;
  }
</style>
