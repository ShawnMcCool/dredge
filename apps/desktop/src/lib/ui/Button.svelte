<script lang="ts">
  // Single button primitive — every control-style button in the app is one
  // of these. One height token (--control-h); labels never wrap.
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  interface Props extends HTMLButtonAttributes {
    variant?: "default" | "chip" | "toggle" | "icon";
    active?: boolean;
    accent?: boolean;
    children?: Snippet;
  }

  let {
    variant = "default",
    active = false,
    accent = false,
    children,
    ...rest
  }: Props = $props();
</script>

<button class="btn {variant}" class:active class:accent {...rest}>
  {@render children?.()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    height: var(--control-h);
    padding: 0 var(--space);
    white-space: nowrap;
    font: inherit;
    color: var(--fg);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    cursor: pointer;
  }

  .btn:hover {
    border-color: var(--muted);
  }

  .btn:focus-visible {
    outline: 1px solid var(--accent-dim);
    outline-offset: -1px;
  }

  .btn:disabled {
    color: var(--muted);
    cursor: default;
  }

  .chip {
    font-family: var(--mono);
    font-size: 11px;
    height: calc(var(--control-h) - 8px);
    padding: 0 6px;
  }

  .icon {
    width: var(--control-h);
    padding: 0;
  }

  .toggle {
    font-size: 12px;
    letter-spacing: 0.02em;
  }

  .active,
  .accent {
    color: var(--bg);
    background: var(--accent);
    border-color: var(--accent);
  }

  .accent:disabled {
    color: var(--muted);
    background: var(--bg-raised);
    border-color: var(--line);
  }
</style>
