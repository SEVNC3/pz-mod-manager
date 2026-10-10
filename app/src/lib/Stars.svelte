<script lang="ts">
  import Icon from "./Icon.svelte";

  // Steam's 0-5 star rating; the container sets the unlit colour.
  let { stars }: { stars: number } = $props();
</script>

{#each [1, 2, 3, 4, 5] as n}<span class:lit={n <= stars}><Icon name="star" size={12} fill /></span>{/each}

<style>
  .lit {
    color: var(--accent-text);
  }
</style>
