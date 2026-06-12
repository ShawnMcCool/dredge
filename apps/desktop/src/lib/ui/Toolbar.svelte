<script lang="ts">
  // Horizontal control container. Children are Groups; when space runs out
  // whole Groups wrap to the next row — never overflow, never split.
  import type { Snippet } from "svelte";

  let { children }: { children?: Snippet } = $props();
</script>

<div class="toolbar">
  {@render children?.()}
</div>

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    column-gap: calc(var(--space) * 2);
    row-gap: var(--space);
    min-width: 0;
    overflow: hidden;
  }
</style>
