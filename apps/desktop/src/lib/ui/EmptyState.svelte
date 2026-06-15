<script lang="ts">
  import type { Snippet } from "svelte";

  // Two shapes in one: a bare muted line (no title) for "nothing here yet"
  // placeholders, or a call-to-action column (with a title and optional action
  // snippet) for prompts like the unanalyzed-track CTA.
  let {
    title,
    children,
    action,
  }: { title?: string; children?: Snippet; action?: Snippet } = $props();
</script>

{#if title || action}
  <div class="cta">
    {#if title}<div class="cta-title">{title}</div>{/if}
    {#if children}<p class="cta-sub">{@render children()}</p>{/if}
    {#if action}{@render action()}{/if}
  </div>
{:else}
  <p class="empty">{@render children?.()}</p>
{/if}

<style>
  .empty {
    font-size: 11px;
    color: var(--muted);
  }
  .cta {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: calc(var(--space) * 1.5);
    padding: var(--space) 0;
  }
  .cta-title {
    font-size: 14px;
    color: var(--fg);
  }
  .cta-sub {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.5;
    max-width: 240px;
    margin: 0;
  }
</style>
