<script lang="ts">
  // Labeled control cluster. Never wraps internally; moves between toolbar
  // rows as one unit. `grow` marks the single elastic group per toolbar.
  import type { Snippet } from "svelte";

  let {
    label,
    grow = false,
    children,
  }: { label?: string; grow?: boolean; children?: Snippet } = $props();
</script>

<span class="group" class:grow>
  {#if label}<span class="label">{label}</span>{/if}
  {@render children?.()}
</span>

<style>
  .group {
    display: inline-flex;
    align-items: center;
    flex: 0 0 auto;
    flex-wrap: nowrap;
    gap: calc(var(--space) / 2);
    min-width: 0;
  }

  .group.grow {
    flex: 1 1 auto;
  }

  .label {
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
  }
</style>
