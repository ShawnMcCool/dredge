<script lang="ts">
  // A section label inside a page — the small-caps muted heading that divides a
  // page into groups (OUTPUT / INPUT / APPEARANCE …). Draws the shared
  // `SurfaceHead` row (as an h3) inside a divider chrome. One header definition
  // across the app; the optional `tools` slot is right-aligned on the row.
  import type { Snippet } from "svelte";
  import SurfaceHead from "./SurfaceHead.svelte";

  interface Props {
    children: Snippet;
    /** Optional trailing controls, right-aligned on the heading row. (Named
     *  `tools`, not `actions`, so a page's snippet can still reach its imported
     *  `actions` store inside the slot.) */
    tools?: Snippet;
  }
  let { children, tools }: Props = $props();
</script>

<div class="section-head">
  <SurfaceHead as="h3" {tools}>{@render children()}</SurfaceHead>
</div>

<style>
  .section-head {
    margin: 0 0 var(--space);
    padding-bottom: 5px;
    border-bottom: 1px solid var(--line);
  }
</style>
