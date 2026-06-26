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
  function moveBlock(r: Routine, idx: number, dir: -1 | 1) {
    const j = idx + dir;
    if (j < 0 || j >= r.blocks.length) return;
    const blocks = r.blocks.slice();
    [blocks[idx], blocks[j]] = [blocks[j], blocks[idx]];
    void actions.saveRoutine({ ...r, blocks });
  }
  function removeBlock(r: Routine, idx: number) {
    void actions.saveRoutine({ ...r, blocks: r.blocks.filter((_, i) => i !== idx) });
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

        <ol class="blocks">
          {#each r.blocks as b, i (i)}
            <li class="block" class:on={isActive(r) && $activeRoutine!.block_index === i}>
              <span class="bn" title={mixLabel(b.mix)}>{i + 1}. {blockName(b)}</span>
              <label class="f" title="playback speed">
                ×<input
                  class="t"
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
              <span class="spacer"></span>
              <button class="mv" disabled={i === 0} onclick={() => moveBlock(r, i, -1)} title="move up">↑</button>
              <button class="mv" disabled={i === r.blocks.length - 1} onclick={() => moveBlock(r, i, 1)} title="move down">↓</button>
              <button class="mv" onclick={() => removeBlock(r, i)} title="remove block">×</button>
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
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 4px;
    border-radius: var(--radius);
    font-size: 11px;
  }
  .block.on {
    background: var(--bg-raised);
  }
  .block.on .bn {
    color: var(--accent);
  }
  .bn {
    min-width: 5.5em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .f {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    color: var(--muted);
  }
  .t {
    width: 3.2em;
    font-size: 11px;
    padding: 1px 3px;
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
