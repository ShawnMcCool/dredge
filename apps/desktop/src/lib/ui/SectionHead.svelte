<script lang="ts">
  // A section label inside a page — the small-caps muted heading that divides a
  // page into groups (OUTPUT / INPUT / APPEARANCE …). One definition shared by
  // every page, with an optional right-aligned slot for a section action.
  import type { Snippet } from "svelte";

  interface Props {
    /** The label text. */
    children: Snippet;
    /** Optional trailing controls, right-aligned on the heading row. (Named
     *  `tools`, not `actions`, so a page's snippet can still reach its imported
     *  `actions` store inside the slot.) */
    tools?: Snippet;
  }
  let { children, tools }: Props = $props();
</script>

<div class="section-head" class:has-tools={tools}>
  <h3>{@render children()}</h3>
  {#if tools}
    <div class="tools">{@render tools()}</div>
  {/if}
</div>

<style>
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space);
    margin: 0 0 var(--space);
    padding-bottom: 5px;
    border-bottom: 1px solid var(--line);
  }
  h3 {
    margin: 0;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
    line-height: 1.4;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: calc(var(--space) / 2);
    flex: 0 0 auto;
  }
</style>
