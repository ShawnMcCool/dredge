<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade } from "svelte/transition";

  // Reveals a cluster of glyph buttons over a waveform region (a selection or a
  // loop) only while that region is hovered. When the region is too narrow to
  // hold the buttons, they sit just outside its edge and the hover hit-zone
  // grows to a full-height band spanning region + buttons, so reaching for them
  // keeps them shown. Hover is driven by the parent's pointer tracking (a plain
  // {x,y}, no blocking overlay div) so the canvas drag/click underneath still
  // works; `onpointerenter/leave` on the cluster itself covers the moment the
  // cursor is over the buttons (where the canvas stops reporting moves).
  const BTN = 24; // glyph button size (px)
  const GAP = 4; // gap between buttons
  const PAD = 6; // breathing room from the region edge / band bottom

  let {
    left,
    right,
    bandTop,
    bandHeight,
    viewWidth,
    pointer,
    count,
    children,
  }: {
    /** Region x-bounds in waveform px. */
    left: number;
    right: number;
    /** Full-height hover band (the wave body): top + height in px. */
    bandTop: number;
    bandHeight: number;
    /** Waveform width in px — decides whether buttons can sit outside-right. */
    viewWidth: number;
    /** Cursor position in waveform px, or null when outside the waveform. */
    pointer: { x: number; y: number } | null;
    /** Number of buttons in the cluster (for width math). */
    count: number;
    children?: Snippet;
  } = $props();

  let hoveringButtons = $state(false);

  let groupW = $derived(count * BTN + (count - 1) * GAP);

  // cluster placement: inside the region's right edge when it fits, else just
  // outside the right edge, else (no room) outside the left edge.
  let placeLeft = $derived.by(() => {
    if (right - left >= groupW + PAD * 2) return right - PAD - groupW;
    if (right + PAD + groupW <= viewWidth) return right + PAD;
    return left - PAD - groupW;
  });
  let top = $derived(bandTop + bandHeight - BTN - PAD);

  // hit-zone: horizontal union of region + cluster, full band height
  let hitLeft = $derived(Math.min(left, placeLeft));
  let hitRight = $derived(Math.max(right, placeLeft + groupW));

  let revealed = $derived.by(() => {
    if (hoveringButtons) return true;
    if (!pointer) return false;
    return (
      pointer.x >= hitLeft &&
      pointer.x <= hitRight &&
      pointer.y >= bandTop &&
      pointer.y <= bandTop + bandHeight
    );
  });
</script>

{#if revealed}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="hover-actions"
    style="left: {placeLeft}px; top: {top}px; gap: {GAP}px;"
    transition:fade={{ duration: 120 }}
    onpointerenter={() => (hoveringButtons = true)}
    onpointerleave={() => (hoveringButtons = false)}
  >
    {@render children?.()}
  </div>
{/if}

<style>
  .hover-actions {
    position: absolute;
    display: flex;
    z-index: 3;
  }
</style>
