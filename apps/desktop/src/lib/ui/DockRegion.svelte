<script lang="ts">
  // One edge of the workspace: an always-present collapse rail on the outer edge
  // plus the region's Dock when expanded. Both sides use this — the only
  // difference is which edge the rail sits on (left region → rail leftmost;
  // right region → rail rightmost) and the chevron direction. The grid column
  // width + collapse is the shell's concern (App.svelte); this just fills it.
  import type { Component } from "svelte";
  import Dock from "./Dock.svelte";
  import type { DockLayout, RegionId } from "../dock";

  interface Props {
    side: RegionId;
    layout: DockLayout;
    collapsed: boolean;
    views: Record<string, Component>;
    onlayout: (layout: DockLayout) => void;
    ontoggle: () => void;
  }
  let { side, layout, collapsed, views, onlayout, ontoggle }: Props = $props();

  // chevron points outward to collapse when expanded, inward to expand when
  // collapsed (matches the old per-aside handles)
  const chevron = $derived(side === "left" ? (collapsed ? "›" : "‹") : collapsed ? "‹" : "›");
  const label = $derived(side === "left" ? "library" : "panels");
  const hotkey = $derived(side === "left" ? "Ctrl+[" : "Ctrl+]");
</script>

<aside class="region {side}" class:collapsed>
  {#if side === "right" && !collapsed}
    <Dock region={side} {layout} {views} {onlayout} />
  {/if}
  <button
    class="rail"
    onclick={ontoggle}
    title={collapsed ? `show ${label} (${hotkey})` : `hide ${label} (${hotkey})`}
    aria-label={collapsed ? `show ${label}` : `hide ${label}`}>{chevron}</button
  >
  {#if side === "left" && !collapsed}
    <Dock region={side} {layout} {views} {onlayout} />
  {/if}
</aside>

<style>
  /* a flex row: the rail on the outer edge, the Dock filling the rest. The rail
     is the single collapse/expand handle — a slam to the window's outer edge
     lands on it at any height. */
  .region {
    display: flex;
    flex-direction: row;
    min-width: 0;
  }
  .region.left {
    border-right: 1px solid var(--line);
  }
  .region.right {
    border-left: 1px solid var(--line);
  }
  /* collapsed: only the rail remains, so the divider has nothing to separate */
  .region.collapsed {
    border: none;
  }

  /* full-height edge rail — toggles its region. Stays quiet (chevron hidden)
     until hovered. */
  .rail {
    flex: 0 0 var(--rail-w);
    align-self: stretch;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 14px;
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .rail:hover {
    background: var(--bg-raised);
    color: var(--fg);
    opacity: 1;
  }
</style>
