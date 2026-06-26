<script lang="ts">
  import { actions, activeRoutine, openSong, STEM_LABELS } from "../lib/stores";
  import type { Block, Mix, Routine } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import EmptyState from "../lib/ui/EmptyState.svelte";

  // One routine is editable at a time. Edit operates on a `draft` clone and only
  // commits on Save — interaction mode (the default) never mutates anything.
  let editingId = $state<number | null>(null);
  let draft = $state<Routine | null>(null);
  let dirty = $state(false);

  function isActive(r: Routine): boolean {
    return $activeRoutine?.running === true && $activeRoutine.routine_id === r.id;
  }
  function isEditing(r: Routine): boolean {
    return editingId === r.id;
  }
  function activeBlock(r: Routine): number {
    return isActive(r) ? $activeRoutine!.block_index : -1;
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

  // ── edit lifecycle ──────────────────────────────────────────────────────────
  function startEdit(r: Routine) {
    draft = JSON.parse(JSON.stringify(r)) as Routine; // plain deep clone
    editingId = r.id;
    dirty = false;
  }
  function cancelEdit() {
    editingId = null;
    draft = null;
    dirty = false;
  }
  async function saveEdit() {
    if (!draft) return;
    await actions.saveRoutine(draft);
    cancelEdit();
  }
  async function createAndEdit() {
    const saved = await actions.saveRoutine({ id: 0, name: "routine", blocks: [actions.captureBlock()] });
    if (saved) startEdit(saved);
  }
  async function removeRoutine(id: number) {
    if (editingId === id) cancelEdit();
    await actions.deleteRoutine(id);
  }

  // ── draft mutations (edit mode) ─────────────────────────────────────────────
  function patchBlock(i: number, patch: Partial<Block>) {
    if (!draft) return;
    draft.blocks[i] = { ...draft.blocks[i], ...patch };
    dirty = true;
  }
  function removeBlock(i: number) {
    if (!draft) return;
    draft.blocks.splice(i, 1);
    dirty = true;
  }
  function addBlock() {
    if (!draft) return;
    draft.blocks.push(actions.captureBlock());
    dirty = true;
  }

  // ── drag-to-reorder (edit mode) ─────────────────────────────────────────────
  // Lifted block tracks the pointer; siblings glide to open its slot; on release
  // it eases into place. Reorders the draft only — saved on Save like any edit.
  let dragFrom = $state(0);
  let dragTo = $state(0);
  let dragDy = $state(0);
  let dragging = $state(false);
  let settling = $state(false);
  let dragStep = 0;
  let dragStartY = 0;

  function startDrag(e: PointerEvent, i: number) {
    if (dragging || settling) return;
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    const row = handle.closest(".block") as HTMLElement;
    dragStep = row.offsetHeight + 2;
    dragStartY = e.clientY;
    dragFrom = i;
    dragTo = i;
    dragDy = 0;
    dragging = true;
    try {
      handle.setPointerCapture(e.pointerId);
    } catch {
      /* non-fatal */
    }
  }
  function moveDrag(e: PointerEvent) {
    if (!dragging || settling || !draft) return;
    dragDy = e.clientY - dragStartY;
    const last = draft.blocks.length - 1;
    dragTo = Math.max(0, Math.min(last, dragFrom + Math.round(dragDy / dragStep)));
  }
  function endDrag() {
    if (!dragging || settling) return;
    const from = dragFrom;
    const to = dragTo;
    settling = true;
    dragDy = (to - from) * dragStep;
    setTimeout(() => {
      settling = false;
      dragging = false;
      if (from !== to && draft) {
        const [m] = draft.blocks.splice(from, 1);
        draft.blocks.splice(to, 0, m);
        dirty = true;
      }
    }, 180);
  }
  function blockShift(i: number): number {
    if (!dragging) return 0;
    if (i === dragFrom) return dragDy;
    if (dragFrom < dragTo && i > dragFrom && i <= dragTo) return -dragStep;
    if (dragFrom > dragTo && i < dragFrom && i >= dragTo) return dragStep;
    return 0;
  }
</script>

{#if $openSong}
  <div class="hdr">
    <Button variant="chip" onclick={() => void createAndEdit()} title="capture the current span + mix as a new routine">
      ＋ new routine
    </Button>
  </div>
{/if}

{#if !$openSong}
  <EmptyState>open a song first</EmptyState>
{:else if $openSong.routines.length === 0}
  <EmptyState>no routines yet</EmptyState>
{:else}
  <ul class="routines">
    {#each $openSong.routines as r (r.id)}
      <li class="routine" class:active={isActive(r)} class:editing={isEditing(r)}>
        {#if isEditing(r) && draft}
          <!-- ===== edit mode: staged form + save ===== -->
          <div class="rhdr">
            <input class="rname-inp" bind:value={draft.name} oninput={() => (dirty = true)} aria-label="routine name" />
            <Button variant="chip" accent disabled={!dirty} onclick={() => void saveEdit()} title="save changes">save</Button>
            <Button variant="chip" onclick={cancelEdit} title="discard changes">cancel</Button>
          </div>

          <ol class="blocks">
            {#each draft.blocks as b, i (i)}
              <li
                class="block edit"
                class:lift={dragging && i === dragFrom}
                class:settle={settling && i === dragFrom}
                style="transform: translateY({blockShift(i)}px)"
              >
                <button
                  class="grip"
                  title="drag to reorder"
                  aria-label="drag to reorder"
                  onpointerdown={(e) => startDrag(e, i)}
                  onpointermove={moveDrag}
                  onpointerup={endDrag}
                  onpointercancel={endDrag}
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <circle cx="9" cy="6" r="1.6" /><circle cx="15" cy="6" r="1.6" />
                    <circle cx="9" cy="12" r="1.6" /><circle cx="15" cy="12" r="1.6" />
                    <circle cx="9" cy="18" r="1.6" /><circle cx="15" cy="18" r="1.6" />
                  </svg>
                </button>
                <input
                  class="bname-inp"
                  placeholder={mixLabel(b.mix)}
                  value={b.name ?? ""}
                  oninput={(e) => patchBlock(i, { name: e.currentTarget.value || null })}
                  aria-label="block name"
                />
                <button class="mv" onclick={() => removeBlock(i)} title="remove block">×</button>
                <div class="bctl">
                  <label class="f" title="playback speed">
                    ×<input class="t speed" type="number" step="0.05" min="0.25" max="2" value={b.speed}
                      onchange={(e) => patchBlock(i, { speed: Number(e.currentTarget.value) })} />
                  </label>
                  <label class="f" title="loop passes on this block">
                    ↻<input class="t" type="number" step="1" min="1" value={b.passes}
                      onchange={(e) => patchBlock(i, { passes: Math.max(1, Math.round(Number(e.currentTarget.value))) })} />
                  </label>
                  <label class="f" title="lead-in beats (run-up before the span)">
                    ⇤<input class="t" type="number" step="1" min="0" value={b.lead_in_beats}
                      onchange={(e) => patchBlock(i, { lead_in_beats: Math.max(0, Math.round(Number(e.currentTarget.value))) })} />
                  </label>
                  <label class="f" title="count-in beats (0 = off)">
                    ⏱<input class="t" type="number" step="1" min="0" value={b.count_in.beats}
                      onchange={(e) => patchBlock(i, { count_in: { ...b.count_in, beats: Math.max(0, Math.round(Number(e.currentTarget.value))) } })} />
                  </label>
                  <button class="every" class:on={b.count_in.loop_mode === "every"}
                    title="count-in every pass (else only on block entry)"
                    onclick={() => patchBlock(i, { count_in: { ...b.count_in, loop_mode: b.count_in.loop_mode === "every" ? "first" : "every" } })}
                  >every</button>
                </div>
              </li>
            {/each}
          </ol>

          <Button variant="chip" onclick={addBlock} title="capture the current span + mix as a block">
            ＋ block from current
          </Button>
        {:else}
          <!-- ===== interaction mode: play / track ===== -->
          <div class="rhdr">
            <span class="rname">{r.name}</span>
            {#if isActive(r)}
              <span class="indicator">block {$activeRoutine!.block_index + 1}/{$activeRoutine!.block_count}</span>
              <Button variant="chip" onclick={() => void actions.stopRoutine()} title="stop">stop</Button>
            {:else}
              <Button variant="chip" onclick={() => void actions.startRoutine(r.id)} disabled={r.blocks.length === 0} title="play from the top">
                play
              </Button>
            {/if}
            <button class="link" onclick={() => startEdit(r)} title="edit this routine">edit</button>
            <button class="mv" onclick={() => void removeRoutine(r.id)} title="delete routine">×</button>
          </div>

          {#if r.blocks.length > 0}
            <ol class="blocks play">
              {#each r.blocks as b, i (i)}
                <button
                  class="block play"
                  class:on={activeBlock(r) === i}
                  onclick={() => void actions.startRoutine(r.id, i)}
                  title="play from this block"
                >
                  <span class="num">{i + 1}</span>
                  <span class="bn">{blockName(b)}</span>
                  {#if b.speed !== 1}<span class="badge">{Math.round(b.speed * 100)}%</span>{/if}
                  {#if b.passes > 1}<span class="badge dim">×{b.passes}</span>{/if}
                </button>
              {/each}
            </ol>
          {/if}
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .hdr {
    display: flex;
    justify-content: flex-end;
    margin-bottom: var(--space);
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
  .routine.editing {
    border-color: var(--line);
    background: var(--bg-raised);
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
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rname-inp {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .indicator {
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .link {
    background: none;
    border: none;
    color: var(--muted);
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    padding: 1px 4px;
    cursor: pointer;
  }
  .link:hover {
    color: var(--fg);
  }

  .blocks {
    list-style: none;
    margin: 0 0 calc(var(--space) / 2);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* ── interaction-mode block: one big clickable play target ── */
  .block.play {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    text-align: left;
    background: var(--bg);
    border: 1px solid transparent;
    border-radius: var(--radius);
    padding: 4px 6px;
    color: var(--fg);
    font-size: 11px;
    cursor: pointer;
  }
  .block.play:hover {
    border-color: var(--line);
  }
  .block.play.on {
    background: var(--bg-raised);
    border-color: var(--accent-dim);
  }
  .block.play.on .bn {
    color: var(--accent);
  }
  .num {
    flex: 0 0 auto;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .bn {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    flex: 0 0 auto;
    font-size: 10px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .badge.dim {
    opacity: 0.7;
  }

  /* ── edit-mode block: grip + name + controls ── */
  .block.edit {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    column-gap: 4px;
    row-gap: 3px;
    padding: 3px 4px;
    border-radius: var(--radius);
    font-size: 11px;
    background: var(--bg);
    transition: transform 180ms cubic-bezier(0.2, 0, 0, 1);
  }
  .block.edit.lift {
    position: relative;
    z-index: 3;
    transition: none;
    background: var(--bg-raised);
    box-shadow: 0 6px 16px rgb(0 0 0 / 0.4);
  }
  .block.edit.settle {
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
  .bname-inp {
    min-width: 0;
    font-size: 11px;
    padding: 1px 4px;
  }
  .bctl {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    padding-left: 18px;
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
  .mv:hover {
    color: var(--fg);
  }
  .every.on {
    color: var(--accent);
    border-color: var(--accent-dim);
  }
</style>
