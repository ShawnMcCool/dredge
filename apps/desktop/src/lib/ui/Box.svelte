<script lang="ts">
  // Shared stage-box shell: a labelled header (with optional right-aligned
  // actions) over a padded body. Every box under the waveform — stems, tuner,
  // analyze — uses this so their headers are guaranteed identical. The box is
  // the managed unit of the stage flow: its `id` keys order + collapse + hidden,
  // and the header is one surface — a short tap collapses it to the header strip,
  // a drag reorders the flow. A hover-revealed × hides it from the stage. The
  // canonical label comes from the box id (override only for a dynamic header,
  // e.g. notes). The stage-flow controller comes from context (inert if absent).
  import type { Snippet } from "svelte";
  import SurfaceHead from "./SurfaceHead.svelte";
  import { getStageFlow } from "../stage-flow.svelte";
  import { BOX_LABELS, type BoxId } from "../stage";

  interface Props {
    /** Stable flow id — keys this box in the stage order + collapse + hidden sets. */
    id: BoxId;
    /** Header label override; defaults to the box's canonical name (`BOX_LABELS`). */
    label?: string;
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

  let { id, label, dim = false, grow = true, wide = false, tools, children }: Props = $props();

  const flow = getStageFlow();
  const collapsed = $derived(flow.isCollapsed(id));
  const dragging = $derived(flow.dragId === id);
  const heading = $derived(label ?? BOX_LABELS[id]);
</script>

<section class="box" class:dim class:nogrow={!grow} class:wide class:collapsed class:dragging data-box={id}>
  <!-- the header is one surface: tap to collapse, drag to reorder (threshold).
       A pointerdown that lands on a button (tools / hide-×) is ignored by the
       controller, so those still click. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header
    class="head"
    onpointerdown={(e) => flow.onHeadDown(e, id)}
    onpointermove={(e) => flow.onHeadMove(e)}
    onpointerup={(e) => flow.onHeadUp(e)}
    onpointercancel={() => flow.onHeadUp()}
    onlostpointercapture={() => flow.onHeadUp()}
  >
    <SurfaceHead {tools}>{heading}</SurfaceHead>
    <button class="hide-x" title="hide from stage" aria-label="hide from stage" onclick={() => flow.hide(id)}>×</button>
  </header>
  {#if !collapsed}
    <div class="body">{@render children()}</div>
  {/if}
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
  /* the box being dragged reads as a lifted placeholder: dimmed + dashed, still
     holding its footprint so the row doesn't reflow under the drag */
  .box.dragging {
    opacity: 0.4;
    border-style: dashed;
  }
  .box.dragging .head {
    cursor: grabbing;
    border-bottom-style: dashed;
  }

  .head {
    display: flex;
    align-items: center;
    min-height: 32px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--line);
    cursor: grab; /* the header is the reorder drag surface (tap = collapse) */
    touch-action: none;
  }
  /* collapsed → just the header strip (no body, no divider) */
  .box.collapsed .head {
    border-bottom: none;
  }
  /* hide-from-stage ×: quiet until the header is hovered (or it's focused) */
  .hide-x {
    flex: 0 0 auto;
    margin-left: 6px;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    font-size: 1rem;
    line-height: 1;
    opacity: 0;
    transition:
      opacity 100ms ease,
      color 100ms ease;
  }
  .head:hover .hide-x {
    opacity: 0.65;
  }
  .hide-x:hover,
  .hide-x:focus-visible {
    color: var(--fg);
    opacity: 1;
  }

  .body {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    padding: 10px;
    min-width: 0;
    /* Hard backstop: a control row can never visually bleed past the box border.
       Combined with wrapping rows (Group/Toolbar wrap), content reflows within
       the box rather than overflowing. No control box uses absolute popovers, so
       clipping is safe here. */
    overflow: hidden;
  }
</style>
