<script lang="ts">
  // The one labelled-surface header row: a small-caps muted label, an optional
  // right-aligned tools slot, and an optional leading collapse caret. Backs both
  // the stage box header (Box, a span label) and the in-page group heading
  // (SectionHead, an h3) so every label header in the app is drawn once. Outer
  // chrome (card border vs page divider) and the drag surface belong to the
  // caller; this is only the row.
  import type { Snippet } from "svelte";

  interface Props {
    /** The label content (text or markup). */
    children: Snippet;
    /** Right-aligned trailing controls. */
    tools?: Snippet;
    /** Label element — `span` for stage boxes, `h3` for page section headings. */
    as?: "span" | "h3";
    /** Show a collapse caret before the label (stage boxes only). */
    collapsible?: boolean;
    collapsed?: boolean;
    oncollapse?: () => void;
  }
  let { children, tools, as = "span", collapsible = false, collapsed = false, oncollapse }: Props = $props();
</script>

<span class="surface-head">
  {#if collapsible}
    <button
      class="caret"
      onclick={oncollapse}
      title={collapsed ? "expand" : "collapse"}
      aria-label={collapsed ? "expand" : "collapse"}>{collapsed ? "›" : "⌄"}</button
    >
  {/if}
  <svelte:element this={as} class="lbl">{@render children()}</svelte:element>
  {#if tools}<span class="tools">{@render tools()}</span>{/if}
</span>

<style>
  .surface-head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1 1 auto;
  }
  .lbl {
    margin: 0;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    line-height: 1.4;
    min-width: 0;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    flex: 0 0 auto;
  }
  /* uniform header tools: plain muted glyph buttons, matching height everywhere */
  .tools :global(button) {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    font-size: 0.95rem;
    line-height: 1;
  }
  .tools :global(button:hover) {
    color: var(--fg);
  }
  .caret {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    font-size: 0.95rem;
    line-height: 1;
    flex: 0 0 auto;
  }
  .caret:hover {
    color: var(--fg);
  }
</style>
