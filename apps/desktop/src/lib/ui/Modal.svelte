<script lang="ts">
  // Single overlay-dialog primitive — every blocking panel in the app is one
  // of these. Rendered at the root (App.svelte hosts it) behind `{#if open}`
  // so nothing stacks contexts underneath it.
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

  interface Props {
    open?: boolean;
    title?: string;
    /** When true: Esc, click-outside, and an × button all call onclose. */
    closable?: boolean;
    onclose?: () => void;
    children?: Snippet;
  }

  let { open = false, title, closable = false, onclose, children }: Props = $props();

  function onkeydown(e: KeyboardEvent) {
    if (open && closable && e.key === "Escape") {
      // mark it consumed so the global Escape cascade leaves it alone
      e.preventDefault();
      onclose?.();
    }
  }

  function onOverlayClick(e: MouseEvent) {
    if (closable && e.target === e.currentTarget) onclose?.();
  }
</script>

<svelte:window {onkeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay fade-in" onclick={onOverlayClick}>
    <div class="panel" role="dialog" aria-modal="true" aria-label={title}>
      {#if title || closable}
        <header>
          <span class="title">{title}</span>
          {#if closable}
            <Button variant="icon" title="close" onclick={() => onclose?.()}>×</Button>
          {/if}
        </header>
      {/if}
      {@render children?.()}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
  }

  .panel {
    width: min(420px, 90vw);
    padding: calc(var(--space) * 2);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space);
    margin-bottom: var(--space);
  }

  .title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
</style>
