<script lang="ts">
  // Interactive tab grid (edit mode only). Left-click a cell to position the
  // overtype cursor; type/arrow/backspace to edit. RIGHT-click + drag anywhere
  // resizes — it grabs the nearest boundary (top edge → strings, right edge →
  // width) from anywhere on the grid, the same "grab the nearest edge" feel as
  // the waveform's right-drag loop resize. All grid math is in lib/notes-doc.
  import {
    clearCell,
    moveCursor,
    setCell,
    setStrings,
    setWidth,
    type Cursor,
    type TabBlock,
  } from "../lib/notes-doc";

  let { block, onchange, ondelete }: {
    block: TabBlock;
    onchange: (b: TabBlock) => void;
    ondelete: () => void;
  } = $props();

  let cursor = $state<Cursor>({ row: 0, col: 0 });
  let gridEl: HTMLDivElement | undefined;
  // boundary currently grabbed for resize, and the one a hover would grab; the
  // shown highlight/cursor prefers an active grab over the hover hint.
  let grabbed = $state<"top" | "right" | null>(null);
  let hovered = $state<"top" | "right" | null>(null);
  let shown = $derived(grabbed ?? hovered);

  const CELL_W = 11; // px per column; keep in sync with .cell width
  const CELL_H = 20; // px per row

  // How far each handle's grab band reaches INSIDE the grid from its edge (the
  // outer reach is the wrapper padding). Bands, not halves — otherwise "nearest
  // edge" hands the top handle most of a wide grid.
  const RIGHT_REACH = 10; // px left of the | edge
  const TOP_REACH = 9; // px below the first row

  /** Which boundary a right-click here would grab, or null when over the grid
   *  body (not on a handle band). Each band spans its edge ± reach/margin; the
   *  overlapping corner picks the nearer edge. */
  function grabAxis(clientX: number, clientY: number): "top" | "right" | null {
    const rect = gridEl!.getBoundingClientRect();
    const nearRight = clientX >= rect.right - RIGHT_REACH; // band around the right edge
    const nearTop = clientY <= rect.top + TOP_REACH; // band around the top edge
    if (nearRight && nearTop) {
      return Math.abs(clientX - rect.right) <= Math.abs(clientY - rect.top) ? "right" : "top";
    }
    if (nearRight) return "right";
    if (nearTop) return "top";
    return null;
  }

  function onPointerMove(e: PointerEvent) {
    if (grabbed || !gridEl) return; // dragging — the grabbed highlight already shows
    hovered = grabAxis(e.clientX, e.clientY);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowUp") { cursor = moveCursor(block, cursor, "up"); e.preventDefault(); return; }
    if (e.key === "ArrowDown") { cursor = moveCursor(block, cursor, "down"); e.preventDefault(); return; }
    if (e.key === "ArrowLeft") { cursor = moveCursor(block, cursor, "left"); e.preventDefault(); return; }
    if (e.key === "ArrowRight") { cursor = moveCursor(block, cursor, "right"); e.preventDefault(); return; }
    if (e.key === "Backspace") {
      onchange(clearCell(block, cursor.row, cursor.col));
      cursor = moveCursor(block, cursor, "left");
      e.preventDefault();
      return;
    }
    if (e.key.length === 1 && /[0-9a-zA-Z/\\~().]/.test(e.key)) {
      onchange(setCell(block, cursor.row, cursor.col, e.key));
      cursor = moveCursor(block, cursor, "right");
      e.preventDefault();
    }
  }

  // Right-drag resize: grab the nearer of the top edge (strings) or right edge
  // (width) from wherever the press lands, then drag that boundary. Listeners
  // live on the captured grid element so they tear down on unmount — no leak.
  function onPointerDown(e: PointerEvent) {
    if (e.button !== 2 || !gridEl) return;
    e.preventDefault();
    const axis = grabAxis(e.clientX, e.clientY);
    grabbed = axis;
    const el = gridEl;
    el.setPointerCapture(e.pointerId);
    const anchorX = e.clientX;
    const anchorY = e.clientY;
    const start = axis === "top" ? block.strings : block.width;
    const move = (ev: PointerEvent) => {
      if (axis === "top") {
        onchange(setStrings(block, start + Math.round((anchorY - ev.clientY) / CELL_H))); // up = more
      } else {
        onchange(setWidth(block, start + Math.round((ev.clientX - anchorX) / CELL_W)));
      }
    };
    const up = () => {
      grabbed = null;
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }
</script>

<div
  class="tabblock"
  class:cursor-top={shown === "top"}
  class:cursor-right={shown === "right"}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerleave={() => (hovered = null)}
  oncontextmenu={(e) => e.preventDefault()}
>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="grid mono"
    tabindex="0"
    role="grid"
    bind:this={gridEl}
    onkeydown={onKey}
  >
    {#each block.rows as row, r (r)}
      <div class="row" role="row">
        <span class="bar">|</span>
        {#each row.split("") as ch, c (c)}
          <button
            class="cell"
            class:active={cursor.row === r && cursor.col === c}
            role="gridcell"
            onclick={() => { cursor = { row: r, col: c }; gridEl?.focus(); }}
          >{ch}</button>
        {/each}
        <span class="bar">|</span>
      </div>
    {/each}
    {#if shown === "top"}<span class="boundary top" class:active={grabbed === "top"}></span>{/if}
    {#if shown === "right"}<span class="boundary right" class:active={grabbed === "right"}></span>{/if}
  </div>
  <button class="del" onclick={ondelete} title="delete tab" aria-label="delete tab">×</button>
</div>

<style>
  .tabblock {
    position: relative;
    display: inline-flex;
    align-items: flex-start;
    align-self: flex-start; /* shrink-wrap the ASCII; don't stretch to the box */
    /* the padding doubles as the resize grab margin: you can right-press in this
       band just outside the ASCII (above the top row / right of the | edge) and
       it still snaps to that boundary. The top and right bands overlap in the
       corner, where the nearer boundary wins. */
    padding: 15px 30px 15px 12px;
  }
  .tabblock.cursor-top { cursor: ns-resize; }
  .tabblock.cursor-right { cursor: ew-resize; }
  .grid {
    position: relative;
    display: flex;
    flex-direction: column;
    outline: none;
  }
  .grid:focus-visible {
    outline: 1px solid var(--accent-dim);
    outline-offset: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    height: 20px;
  }
  .bar {
    color: var(--muted);
  }
  .cell {
    width: 11px;
    text-align: center;
    background: none;
    border: none;
    color: var(--fg);
    cursor: text;
    padding: 0;
    font: inherit;
    line-height: 20px;
  }
  .cell.active {
    background: var(--accent);
    color: var(--bg);
  }
  /* the boundary a right-click would grab: dim on hover, bright while dragging */
  .boundary {
    position: absolute;
    background: var(--accent-dim);
    pointer-events: none;
  }
  .boundary.active {
    background: var(--accent);
  }
  .boundary.top {
    top: -1px;
    left: 0;
    right: 0;
    height: 2px;
  }
  .boundary.right {
    top: 0;
    bottom: 0;
    right: -1px;
    width: 2px;
  }
  .del {
    align-self: flex-start;
    margin-left: 2px;
    padding: 0;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    line-height: 1;
  }
  .del:hover { color: var(--fg); }
</style>
