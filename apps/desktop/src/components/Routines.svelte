<script lang="ts">
  import { actions, activeRoutine, openSong, STEM_LABELS } from "../lib/stores";
  import type { Block, Mix, Routine } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import EmptyState from "../lib/ui/EmptyState.svelte";

  let renamingId = $state<number | null>(null);
  let renameValue = $state("");

  function isActive(r: Routine): boolean {
    return $activeRoutine?.running === true && $activeRoutine.routine_id === r.id;
  }

  /** Short, human label derived from a block's mix — the fallback block name. */
  function mixLabel(mix: Mix): string {
    if (mix.bass_focus) return "bass focus";
    if (mix.stems.every((g) => g >= 0.99)) return "full band";
    const up = mix.stems.map((g, i) => ({ g, i })).filter((s) => s.g > 0.5);
    if (up.length === 1) return STEM_LABELS[up[0].i].toLowerCase();
    const down = mix.stems.map((g, i) => ({ g, i })).filter((s) => s.g < 0.01);
    if (down.length === 1) return `no ${STEM_LABELS[down[0].i].toLowerCase()}`;
    return "custom mix";
  }

  function blockName(b: Block): string {
    return b.name?.trim() || mixLabel(b.mix);
  }

  function patchBlock(r: Routine, idx: number, patch: Partial<Block>) {
    const blocks = r.blocks.map((b, i) => (i === idx ? { ...b, ...patch } : b));
    void actions.saveRoutine({ ...r, blocks });
  }
  function removeBlock(r: Routine, idx: number) {
    void actions.saveRoutine({ ...r, blocks: r.blocks.filter((_, i) => i !== idx) });
  }

  // ── Drag-to-reorder ─────────────────────────────────────────────────────────
  // The lifted block tracks the pointer 1:1; siblings glide to open its slot;
  // on release it eases into the slot, then the reorder is committed silently.
  let dragRoutine = $state<number | null>(null);
  let dragFrom = $state(0);
  let dragTo = $state(0);
  let dragDy = $state(0);
  let settling = $state(false);
  let dragStep = 0; // px between block centres (row height + gap)
  let dragStartY = 0;

  function startDrag(e: PointerEvent, r: Routine, i: number) {
    if (dragRoutine !== null || settling) return;
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    const row = handle.closest(".block") as HTMLElement;
    dragStep = row.offsetHeight + 2; // .blocks gap
    dragStartY = e.clientY;
    dragRoutine = r.id;
    dragFrom = i;
    dragTo = i;
    dragDy = 0;
    // Capture keeps move/up flowing if the pointer leaves the handle; a failure
    // (e.g. an odd pointer id) must not abort the drag.
    try {
      handle.setPointerCapture(e.pointerId);
    } catch {
      /* non-fatal */
    }
  }
  function moveDrag(e: PointerEvent, r: Routine) {
    if (dragRoutine !== r.id || settling) return;
    dragDy = e.clientY - dragStartY;
    const last = r.blocks.length - 1;
    dragTo = Math.max(0, Math.min(last, dragFrom + Math.round(dragDy / dragStep)));
  }
  function endDrag(r: Routine) {
    if (dragRoutine !== r.id || settling) return;
    const from = dragFrom;
    const to = dragTo;
    // Ease the lifted block into its resting slot, then commit.
    settling = true;
    dragDy = (to - from) * dragStep;
    setTimeout(() => {
      settling = false;
      dragRoutine = null;
      if (from !== to) {
        const blocks = r.blocks.slice();
        const [m] = blocks.splice(from, 1);
        blocks.splice(to, 0, m);
        void actions.saveRoutine({ ...r, blocks });
      }
    }, 180);
  }

  /** The transform that opens the dragged block's slot and lifts it. */
  function blockShift(r: Routine, i: number): number {
    if (dragRoutine !== r.id) return 0;
    if (i === dragFrom) return dragDy;
    if (dragFrom < dragTo && i > dragFrom && i <= dragTo) return -dragStep;
    if (dragFrom > dragTo && i < dragFrom && i >= dragTo) return dragStep;
    return 0;
  }
  function dragging(r: Routine, i: number): boolean {
    return dragRoutine === r.id && i === dragFrom;
  }

  function startRename(r: Routine) {
    renamingId = r.id;
    renameValue = r.name;
  }
  function commitRename(r: Routine) {
    const name = renameValue.trim();
    renamingId = null;
    if (name && name !== r.name) void actions.saveRoutine({ ...r, name });
  }
  function focusNode(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<div class="hdr">
  <h2>routines</h2>
  {#if $openSong}
    <Button variant="chip" onclick={() => void actions.newRoutine()} title="capture the current span + mix as a new routine">
      ＋ new
    </Button>
  {/if}
</div>

{#if !$openSong}
  <EmptyState>open a song first</EmptyState>
{:else if $openSong.routines.length === 0}
  <EmptyState>no routines yet</EmptyState>
{:else}
  <ul class="routines">
    {#each $openSong.routines as r (r.id)}
      <li class="routine" class:active={isActive(r)}>
        <div class="rhdr">
          {#if renamingId === r.id}
            <input
              class="rname"
              use:focusNode
              bind:value={renameValue}
              onblur={() => commitRename(r)}
              onkeydown={(e) => {
                if (e.key === "Enter") commitRename(r);
                else if (e.key === "Escape") renamingId = null;
              }}
            />
          {:else}
            <button class="rname pick" ondblclick={() => startRename(r)} title="double-click to rename">
              {r.name}
            </button>
          {/if}
          {#if isActive(r)}
            <span class="indicator" title="active block">
              block {$activeRoutine!.block_index + 1}/{$activeRoutine!.block_count}
            </span>
            <Button variant="chip" onclick={() => void actions.stopRoutine()} title="stop advancing">stop</Button>
          {:else}
            <Button
              variant="chip"
              onclick={() => void actions.startRoutine(r.id)}
              disabled={r.blocks.length === 0}
              title="launch this routine"
            >
              play
            </Button>
          {/if}
          <Button variant="chip" onclick={() => void actions.deleteRoutine(r.id)} title="delete routine">×</Button>
        </div>

        <ol class="blocks" class:dragging={dragRoutine === r.id}>
          {#each r.blocks as b, i (i)}
            <li
              class="block"
              class:on={isActive(r) && $activeRoutine!.block_index === i}
              class:lift={dragging(r, i)}
              class:settle={settling && dragRoutine === r.id && i === dragFrom}
              style="transform: translateY({blockShift(r, i)}px)"
            >
              <button
                class="grip"
                title="drag to reorder"
                aria-label="drag to reorder"
                onpointerdown={(e) => startDrag(e, r, i)}
                onpointermove={(e) => moveDrag(e, r)}
                onpointerup={() => endDrag(r)}
                onpointercancel={() => endDrag(r)}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <circle cx="9" cy="6" r="1.6" /><circle cx="15" cy="6" r="1.6" />
                  <circle cx="9" cy="12" r="1.6" /><circle cx="15" cy="12" r="1.6" />
                  <circle cx="9" cy="18" r="1.6" /><circle cx="15" cy="18" r="1.6" />
                </svg>
              </button>
              <button class="bn" title="play from this block" onclick={() => void actions.startRoutine(r.id, i)}>
                {i + 1}. {blockName(b)}
              </button>
              <button class="mv" onclick={() => removeBlock(r, i)} title="remove block">×</button>
              <div class="bctl">
                <label class="f" title="playback speed">
                  ×<input
                    class="t speed"
                    type="number"
                    step="0.05"
                    min="0.25"
                    max="2"
                    value={b.speed}
                    onchange={(e) => patchBlock(r, i, { speed: Number(e.currentTarget.value) })}
                  />
                </label>
                <label class="f" title="loop passes on this block">
                  ↻<input
                    class="t"
                    type="number"
                    step="1"
                    min="1"
                    value={b.passes}
                    onchange={(e) => patchBlock(r, i, { passes: Math.max(1, Math.round(Number(e.currentTarget.value))) })}
                  />
                </label>
                <label class="f" title="lead-in beats (run-up before the span)">
                  ⇤<input
                    class="t"
                    type="number"
                    step="1"
                    min="0"
                    value={b.lead_in_beats}
                    onchange={(e) =>
                      patchBlock(r, i, { lead_in_beats: Math.max(0, Math.round(Number(e.currentTarget.value))) })}
                  />
                </label>
                <label class="f" title="count-in beats (0 = off)">
                  ⏱<input
                    class="t"
                    type="number"
                    step="1"
                    min="0"
                    value={b.count_in.beats}
                    onchange={(e) =>
                      patchBlock(r, i, {
                        count_in: {
                          ...b.count_in,
                          beats: Math.max(0, Math.round(Number(e.currentTarget.value))),
                        },
                      })}
                  />
                </label>
                <button
                  class="every"
                  class:on={b.count_in.loop_mode === "every"}
                  title="count-in every pass (else only on block entry)"
                  onclick={() =>
                    patchBlock(r, i, {
                      count_in: { ...b.count_in, loop_mode: b.count_in.loop_mode === "every" ? "first" : "every" },
                    })}
                >
                  every
                </button>
              </div>
            </li>
          {/each}
        </ol>

        <Button variant="chip" onclick={() => void actions.addBlock(r)} title="capture the current span + mix as a block">
          ＋ block from current
        </Button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .hdr {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space);
  }

  .routines {
    display: flex;
    flex-direction: column;
    gap: var(--space);
  }

  .routine {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: calc(var(--space) / 2);
  }
  .routine.active {
    border-color: var(--accent-dim);
  }

  .rhdr {
    display: flex;
    align-items: center;
    gap: calc(var(--space) / 2);
    margin-bottom: calc(var(--space) / 2);
  }
  .rname {
    flex: 1;
    min-width: 0;
  }
  .pick {
    background: none;
    border: none;
    text-align: left;
    padding: 2px 4px;
    color: var(--fg);
    font-weight: 600;
  }
  .pick:hover {
    background: var(--bg-raised);
  }
  .indicator {
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--accent);
  }

  .blocks {
    list-style: none;
    margin: 0 0 calc(var(--space) / 2);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .block {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    column-gap: 4px;
    row-gap: 3px;
    padding: 3px 4px;
    border-radius: var(--radius);
    font-size: 11px;
    background: var(--bg);
    /* siblings glide to open the dragged block's slot */
    transition: transform 180ms cubic-bezier(0.2, 0, 0, 1);
  }
  .block.on {
    background: var(--bg-raised);
  }
  .block.on .bn {
    color: var(--accent);
  }
  /* the lifted block tracks the pointer 1:1 — no transition while held */
  .block.lift {
    position: relative;
    z-index: 3;
    transition: none;
    background: var(--bg-raised);
    box-shadow: 0 6px 16px rgb(0 0 0 / 0.4);
    cursor: grabbing;
  }
  /* …but eases into its slot on release */
  .block.settle {
    transition: transform 180ms cubic-bezier(0.2, 0, 0, 1);
  }

  .grip {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    padding: 2px;
    background: none;
    border: none;
    color: var(--muted);
    cursor: grab;
    touch-action: none;
  }
  .grip:hover {
    color: var(--fg);
  }
  .grip svg {
    width: 13px;
    height: 13px;
    fill: currentColor;
  }

  .bn {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    background: none;
    border: none;
    color: var(--fg);
    padding: 2px 2px;
    cursor: pointer;
  }
  .bn:hover {
    color: var(--accent);
  }
  /* second row: the per-block knobs, spanning the full width under the header */
  .bctl {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    padding-left: 18px; /* align under the name, past the grip */
  }
  .f {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    color: var(--muted);
  }
  .t {
    width: 2.6em;
    font-size: 11px;
    padding: 1px 3px;
  }
  .t.speed {
    width: 4em;
  }
  .every,
  .mv {
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius);
    color: var(--muted);
    font-size: 10px;
    padding: 1px 4px;
    cursor: pointer;
  }
  .every:hover,
  .mv:hover:not(:disabled) {
    color: var(--fg);
  }
  .every.on {
    color: var(--accent);
    border-color: var(--accent-dim);
  }
  .mv:disabled {
    opacity: 0.3;
    cursor: default;
  }
</style>
