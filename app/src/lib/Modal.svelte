<script lang="ts">
  import type { Snippet } from "svelte";
  import { t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import { trap } from "./ui";

  /** A window over the page (Check setup, the activity log, Download history, What's new): a title bar
   * with Close, the content, and an optional footer. Escape and the backdrop close it. */
  let { title, width, height, fixed = false, head, foot, children, onClose }: {
    title: string;
    /** Its widest, in px (narrower on a small window). */
    width: number;
    /** Its tallest, in px; `fixed`: always that tall (content scrolls inside). */
    height?: number;
    fixed?: boolean;
    /** More in the title bar, after the title. */
    head?: Snippet;
    foot?: Snippet;
    children: Snippet;
    onClose: () => void;
  } = $props();
  const tall = $derived(height ? `min(${height}px, calc(100vh - 48px))` : "calc(100vh - 48px)");
</script>

<div class="backdrop" onclick={onClose} aria-hidden="true"></div>
<div class="modal" role="dialog" tabindex="-1" aria-modal="true" aria-label={title} use:trap
  style:width="min({width}px, calc(100vw - 48px))" style:max-height={tall} style:height={fixed ? tall : undefined}
  onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), onClose())}>
  <header>
    <h2>{title}</h2>
    {@render head?.()}
    <button class="quiet close" onclick={onClose} aria-label={t("common.close")}><Icon name="x" size={16} /></button>
  </header>
  {@render children()}
  {#if foot}<footer>{@render foot()}</footer>{/if}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    background: rgb(0 0 0 / 0.5);
  }
  .modal {
    position: fixed;
    z-index: 31;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.55);
  }
  header,
  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding: 12px 14px 12px 18px;
  }
  header {
    border-bottom: 1px solid var(--line);
  }
  footer {
    border-top: 1px solid var(--line);
  }
  h2 {
    margin: 0;
    font-size: 1.05em;
  }
  .close {
    margin-left: auto;
  }
</style>
