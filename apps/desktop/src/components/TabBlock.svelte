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
  // which boundary is currently grabbed for resize (null = not resizing)
  let grabbed = $state<"top" | "right" | null>(null);

  const CELL_W = 11; // px per column; keep in sync with .cell width
  const CELL_H = 20; // px per row

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
    const rect = gridEl.getBoundingClientRect();
    // Zone-based: the right margin always resizes width, the top margin always
    // resizes strings; inside the grid the nearer edge wins. Keeps a wide right
    // grab band unambiguous — raw nearest-distance would flip to "top" for a
    // mid-height press far to the right.
    let axis: "top" | "right";
    if (e.clientX > rect.right) axis = "right";
    else if (e.clientY < rect.top) axis = "top";
    else axis = rect.right - e.clientX <= e.clientY - rect.top ? "right" : "top";
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
  class:grab-top={grabbed === "top"}
  class:grab-right={grabbed === "right"}
  onpointerdown={onPointerDown}
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
    {#if grabbed === "top"}<span class="boundary top"></span>{/if}
    {#if grabbed === "right"}<span class="boundary right"></span>{/if}
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
       it still snaps to that boundary. Right is deliberately overshot for now —
       dial back once the feel is right. */
    padding: 16px 72px 16px 10px;
  }
  .tabblock.grab-top { cursor: ns-resize; }
  .tabblock.grab-right { cursor: ew-resize; }
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
  /* the grabbed boundary, highlighted while dragging */
  .boundary {
    position: absolute;
    background: var(--accent);
    pointer-events: none;
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
