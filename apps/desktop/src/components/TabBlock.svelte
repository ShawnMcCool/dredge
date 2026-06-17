<script lang="ts">
  // Interactive tab grid. Renders rows bounded by | and an overtype cell editor;
  // a top handle resizes string count (growth prepends higher strings on top),
  // a right handle resizes width (growth appends to the right). All grid math is
  // in lib/notes-doc; this component only maps pointer/keyboard to those calls.
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

  // Drag the top edge: every CELL_H of vertical travel = ±1 string.
  function dragStrings(e: PointerEvent) {
    e.preventDefault();
    const startY = e.clientY;
    const start = block.strings;
    const move = (ev: PointerEvent) => {
      const delta = Math.round((startY - ev.clientY) / CELL_H); // up = more
      onchange(setStrings(block, start + delta));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  // Drag the right edge: every CELL_W of horizontal travel = ±1 column.
  function dragWidth(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const start = block.width;
    const move = (ev: PointerEvent) => {
      const delta = Math.round((ev.clientX - startX) / CELL_W);
      onchange(setWidth(block, start + delta));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
</script>

<div class="tabblock">
  <button class="handle top" onpointerdown={dragStrings} title="drag: add/remove strings" aria-label="resize strings"></button>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="grid mono" tabindex="0" role="grid" onkeydown={onKey}>
    {#each block.rows as row, r (r)}
      <div class="row" role="row">
        <span class="bar">|</span>
        {#each row.split("") as ch, c (c)}
          <button
            class="cell"
            class:active={cursor.row === r && cursor.col === c}
            role="gridcell"
            onclick={() => (cursor = { row: r, col: c })}
          >{ch}</button>
        {/each}
        <span class="bar">|</span>
      </div>
    {/each}
  </div>
  <button class="handle right" onpointerdown={dragWidth} title="drag: add/remove width" aria-label="resize width"></button>
  <button class="del" onclick={ondelete} title="delete tab" aria-label="delete tab">×</button>
</div>

<style>
  .tabblock {
    position: relative;
    display: inline-block;
    padding: 6px 10px;
  }
  .grid {
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
  .handle {
    position: absolute;
    background: none;
    border: none;
    padding: 0;
  }
  .handle.top {
    top: 0;
    left: 10px;
    right: 22px;
    height: 6px;
    cursor: ns-resize;
  }
  .handle.top:hover { background: var(--accent-dim); }
  .handle.right {
    top: 6px;
    bottom: 6px;
    right: 10px;
    width: 6px;
    cursor: ew-resize;
  }
  .handle.right:hover { background: var(--accent-dim); }
  .del {
    position: absolute;
    top: 2px;
    right: 0;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    line-height: 1;
  }
  .del:hover { color: var(--fg); }
</style>
