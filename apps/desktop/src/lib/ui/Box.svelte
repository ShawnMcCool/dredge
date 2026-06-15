<script lang="ts">
  // Shared stage-box shell: a labelled header (with optional right-aligned
  // actions) over a padded body. Every box under the waveform — stems, tuner,
  // analyze — uses this so their headers are guaranteed identical.
  import type { Snippet } from "svelte";

  interface Props {
    label: string;
    /** Soften the whole box (e.g. the tuner while powered off). */
    dim?: boolean;
    /** Grow to fill the row (default), or lock the box to its content width. */
    grow?: boolean;
    /** Prefer the full row: a large flex-basis so the box wraps to its own line
     *  except on a very wide stage (where it can still pair with a sibling). */
    wide?: boolean;
    /** Right-aligned header controls. */
    tools?: Snippet;
    children: Snippet;
  }

  let { label, dim = false, grow = true, wide = false, tools, children }: Props = $props();
</script>

<section class="box" class:dim class:nogrow={!grow} class:wide>
  <header class="head">
    <span class="lbl">{label}</span>
    {#if tools}<span class="head-actions">{@render tools()}</span>{/if}
  </header>
  <div class="body">{@render children()}</div>
</section>

<style>
  .box {
    /* fill the row by default; wraps once it can't hold its basis */
    flex: 1 1 240px;
    min-width: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--bg-raised);
    display: flex;
    flex-direction: column;
  }
  /* lock to content width instead of growing (e.g. the stems box) */
  .box.nogrow {
    flex: 0 0 auto;
  }
  /* prefer a full row: a large basis forces a wrap unless the stage is wide */
  .box.wide {
    flex: 1 1 480px;
  }
  .box.dim {
    opacity: 0.8;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-height: 32px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--line);
  }

  .lbl {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  /* uniform header tools across every box: a plain muted glyph, no border —
     so the header height (and the icons) match whatever box you're looking at */
  .head-actions :global(button) {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    font-size: 0.95rem;
    line-height: 1;
  }
  .head-actions :global(button:hover) {
    color: var(--fg);
  }

  .body {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    padding: 10px;
    min-width: 0;
  }
</style>
