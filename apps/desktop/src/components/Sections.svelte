<script lang="ts">
  import { untrack } from "svelte";
  import { fmtDur } from "../lib/format";
  import {
    actions,
    analysisError,
    openSong,
    prepareState,
    selection,
    type AnalysisSection,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import EmptyState from "../lib/ui/EmptyState.svelte";
  import Modal from "../lib/ui/Modal.svelte";

  interface Row {
    name: string;
    start: number;
    end: number;
    /** Came from analysis and is not saved yet — muted in the UI. */
    suggested?: boolean;
  }

  let rows = $state<Row[]>([]);
  let dirty = $state(false);
  let editing = $state(false);
  let lastSongId: number | null = null;

  // Mirror the store into `rows` unless there are unsaved edits. The only
  // external trigger is `$openSong`; everything else (rows/dirty/editing/
  // lastSongId) is read AND written here, so it must be untracked — otherwise
  // writing `dirty` re-fires the effect on its own output, an infinite loop
  // (Svelte's effect_update_depth_exceeded) that wedges the UI on song load.
  $effect(() => {
    const open = $openSong;
    untrack(() => {
      if (!open) {
        rows = [];
        dirty = false;
        lastSongId = null;
        return;
      }
      const switching = open.song.id !== lastSongId;
      if (switching || !dirty) {
        lastSongId = open.song.id;
        if (switching) editing = false;
        if (open.sections.length > 0) {
          rows = open.sections.map((s) => ({ name: s.name, start: s.start, end: s.end }));
          if (switching) dirty = false;
        } else {
          // no saved sections yet (e.g. a song analyzed before auto-save landed) —
          // seed the editor from cached analysis so the structure is editable
          // without re-running the model (provisional rows until saved)
          rows = (open.analysis?.sections ?? []).map(toRow);
          dirty = rows.length > 0;
        }
      }
    });
  });

  function toRow(s: AnalysisSection): Row {
    return {
      name: s.label,
      start: Math.round(s.start * 10) / 10,
      end: Math.round(s.end * 10) / 10,
      suggested: true,
    };
  }

  /** Click a section in display mode → highlight its span on the waveform. */
  function highlight(row: Row) {
    selection.set({ start: row.start, end: row.end });
  }

  /** Loop a section directly — transient, nothing saved (mirrors the ⟳ control). */
  function loop(row: Row) {
    void actions.setTransportLoop(row.start, row.end);
  }

  let analysis = $derived($openSong?.analysis ?? null);
  let running = $derived($prepareState !== null);
  let hasAnalysis = $derived((analysis?.sections?.length ?? 0) > 0);

  // time signature ≈ beats per bar (beats / downbeats), when it's sane
  let meter = $derived.by(() => {
    const a = analysis;
    if (!a?.beats?.length || !a?.downbeats?.length) return null;
    const per = Math.round(a.beats.length / a.downbeats.length);
    return per >= 2 && per <= 12 ? `${per}/4` : null;
  });

  // the subordinate stats line: only the facts we actually have
  let metaParts = $derived.by(() => {
    const a = analysis;
    if (!a) return [];
    const parts: string[] = [];
    if (a.bpm) parts.push(`${Math.round(a.bpm)} BPM`);
    if (meter) parts.push(meter);
    if (a.downbeats?.length) parts.push(`${a.downbeats.length} bars`);
    if (a.beats?.length) parts.push(`${a.beats.length} beats`);
    return parts;
  });

  // revert the editor to the cached analysis — no model rerun needed, the
  // SongFormer result lives in the DB. Replaces the current (unsaved) edits.
  function revertToAnalysis() {
    rows = (analysis?.sections ?? []).map(toRow);
    touch();
  }

  function touch() {
    dirty = true;
  }

  function add() {
    const sel = $selection;
    const lastEnd = rows[rows.length - 1]?.end ?? 0;
    rows.push({
      name: `section ${rows.length + 1}`,
      start: sel?.start ?? lastEnd,
      end: sel?.end ?? lastEnd + 4,
    });
    touch();
  }

  function remove(i: number) {
    rows.splice(i, 1);
    touch();
  }

  function move(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= rows.length) return;
    [rows[i], rows[j]] = [rows[j], rows[i]];
    touch();
  }

  async function save() {
    await actions.replaceSections(
      rows.map((r, i) => ({ name: r.name, start: r.start, end: r.end, position: i })),
    );
    dirty = false;
    editing = false;
  }

  let confirmReanalyze = $state(false);

  async function reanalyze() {
    confirmReanalyze = false;
    await actions.reanalyze();
  }
</script>

<div class="head">
  <h2>sections{#if editing}<span class="sub"> · editing</span>{/if}</h2>
  {#if $openSong}
    <div class="head-actions">
      <button class="txt-btn" class:active={editing} onclick={() => (editing = !editing)}>
        {editing ? "done" : "edit"}
      </button>
      {#if hasAnalysis || rows.length > 0}
        <button class="txt-btn" onclick={() => (confirmReanalyze = true)}>re-analyze</button>
      {/if}
    </div>
  {/if}
</div>

{#if !$openSong}
  <EmptyState>open a song first</EmptyState>
{:else}
  {#if metaParts.length}
    <div class="meta mono">
      {#each metaParts as p, i (p)}
        {#if i > 0}<span class="sep">·</span>{/if}<span>{p}</span>
      {/each}
    </div>
  {/if}

  {#if editing}
    <ol class="sections">
      {#each rows as row, i (i)}
        <li class="row edit" class:suggested={row.suggested}>
          <div class="reorder">
            <button onclick={() => move(i, -1)} title="move up" aria-label="move up">▲</button>
            <button onclick={() => move(i, 1)} title="move down" aria-label="move down">▼</button>
          </div>
          <div class="fields">
            <input class="name-inp" bind:value={row.name} oninput={touch} aria-label="section name" />
            <div class="time-line">
              <input class="time-inp mono" type="number" step="0.1" min="0" bind:value={row.start} oninput={touch} aria-label="start" />
              <span class="dash">–</span>
              <input class="time-inp mono" type="number" step="0.1" min="0" bind:value={row.end} oninput={touch} aria-label="end" />
              <button class="del" onclick={() => remove(i)}>delete</button>
            </div>
          </div>
        </li>
      {/each}
    </ol>
    <div class="edit-footer">
      <button class="add-section" onclick={add}>+ add section</button>
      <div class="edit-actions">
        {#if dirty}<span class="unsaved mono">unsaved edits</span>{/if}
        {#if hasAnalysis}
          <Button onclick={revertToAnalysis} title="discard edits, reload the analyzed structure">revert</Button>
        {/if}
        <Button accent disabled={!dirty} onclick={save}>save</Button>
      </div>
    </div>
    {#if $analysisError}<p class="error">{$analysisError}</p>{/if}
  {:else if rows.length === 0}
    <EmptyState title="not analyzed yet">
      detect beats, bars, and song sections — the structure shows up here once it's done.
      {#snippet action()}
        {#if running}
          <span class="analyzing mono">analyzing…</span>
        {:else}
          <Button accent onclick={() => void actions.prepare()}>Analyze track</Button>
        {/if}
        {#if $analysisError}<p class="error">{$analysisError}</p>{/if}
      {/snippet}
    </EmptyState>
  {:else}
    <ol class="sections">
      {#each rows as row, i (i)}
        <li
          class="row"
          class:suggested={row.suggested}
          class:active={$selection?.start === row.start && $selection?.end === row.end}
        >
          <button class="open" onclick={() => highlight(row)} title="highlight on the waveform">
            <span class="ord mono">{i + 1}</span>
            <span class="name">{row.name}</span>
            <span class="range mono">{fmtDur(row.start, true)}–{fmtDur(row.end, true)}</span>
          </button>
          <button class="loop txt-btn" onclick={() => loop(row)} title="loop this section">loop</button>
        </li>
      {/each}
    </ol>
    {#if dirty}<p class="note unsaved mono">unsaved edits — open edit to save</p>{/if}
  {/if}

  <Modal open={confirmReanalyze} title="re-analyze" closable onclose={() => (confirmReanalyze = false)}>
    <p>Discard the cached beat grid and section suggestions and run analysis again?</p>
    <div class="modal-actions">
      <Button onclick={() => (confirmReanalyze = false)}>cancel</Button>
      <Button accent onclick={reanalyze}>re-analyze</Button>
    </div>
  </Modal>
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space);
    min-height: 22px;
    margin-bottom: var(--space);
  }
  .head h2 {
    margin: 0;
  }
  .head h2 .sub {
    color: var(--accent-dim);
  }
  .head-actions {
    display: flex;
    gap: var(--space);
  }

  .txt-btn {
    background: none;
    border: none;
    color: var(--muted);
    font: inherit;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
    padding: 2px 0;
  }
  .txt-btn:hover {
    color: var(--fg);
  }
  .txt-btn.active {
    color: var(--accent);
  }

  /* subordinate stats line — present but clearly a byline, not a dashboard */
  .meta {
    font-size: 11px;
    color: var(--muted);
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 0 6px;
    margin-bottom: calc(var(--space) * 2.5);
  }
  .meta .sep {
    color: var(--line);
  }

  /* ===== the section list — the hero ===== */
  .sections {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .row {
    display: flex;
    align-items: baseline;
    min-width: 0;
    border-left: 2px solid transparent;
    border-radius: var(--radius);
  }
  .row:hover {
    background: var(--bg-raised);
    border-left-color: var(--accent-dim);
  }
  .row:hover .ord {
    color: var(--muted);
  }
  /* click-to-highlight marks the row active (its span owns the selection) */
  .row.active {
    background: var(--bg-raised);
    border-left-color: var(--accent);
  }
  .row.active .name {
    color: var(--accent);
  }
  .row.active .ord {
    color: var(--accent-dim);
  }

  /* the row body is the click target (highlight the section's span) */
  .open {
    flex: 1;
    display: grid;
    grid-template-columns: 18px 1fr auto;
    align-items: baseline;
    column-gap: 10px;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
    padding: 5px 8px;
    min-width: 0;
  }
  .ord {
    font-size: 10px;
    color: var(--line);
    text-align: right;
    transition: color 0.12s;
  }
  .name {
    font-size: 15px;
    color: var(--fg);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.2;
  }
  .range {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
  }

  /* analysis suggestions: visibly provisional until saved */
  .row.suggested .name {
    color: var(--muted);
  }

  /* per-row loop affordance — holds its slot so the range never shifts, but
     stays invisible until the row is hovered (or the button is focused) */
  .loop {
    flex: 0 0 auto;
    margin-right: 6px;
    opacity: 0;
    transition: opacity 0.12s;
    pointer-events: none;
  }
  .row:hover .loop,
  .loop:focus-visible {
    opacity: 1;
    pointer-events: auto;
  }
  .loop:hover {
    color: var(--accent);
  }

  /* ===== edit mode ===== */
  .row.edit {
    display: grid;
    grid-template-columns: 14px 1fr;
    column-gap: 10px;
    align-items: center;
    padding: 6px 0;
  }
  .reorder {
    display: flex;
    flex-direction: column;
    gap: 1px;
    align-items: center;
  }
  .reorder button {
    background: none;
    border: none;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 9px;
    line-height: 1;
    cursor: pointer;
    padding: 1px;
  }
  .reorder button:hover {
    color: var(--fg);
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }
  .name-inp {
    width: 100%;
    font-size: 14px;
  }
  .time-line {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .time-inp {
    width: 7em;
    font-size: 11px;
    text-align: center;
    padding: 3px 4px;
  }
  .dash {
    color: var(--muted);
    font-family: var(--mono);
  }
  .del {
    margin-left: auto;
    background: none;
    border: none;
    color: var(--muted);
    font: inherit;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
    padding: 0;
  }
  .del:hover {
    color: var(--miss);
  }
  .row.edit.suggested .name-inp {
    color: var(--muted);
    border-color: var(--accent-dim);
  }

  .edit-footer {
    margin-top: calc(var(--space) * 2);
    display: flex;
    flex-direction: column;
    gap: var(--space);
  }
  .add-section {
    background: none;
    border: 1px dashed var(--line);
    border-radius: var(--radius);
    color: var(--muted);
    font: inherit;
    font-size: 11px;
    padding: 7px;
    width: 100%;
    cursor: pointer;
    text-align: center;
  }
  .add-section:hover {
    border-color: var(--muted);
    color: var(--fg);
  }
  .edit-actions {
    display: flex;
    align-items: center;
    gap: var(--space);
  }
  .unsaved {
    font-size: 10px;
    color: var(--accent);
    margin-right: auto;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .unsaved::before {
    content: "";
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
  }
  .note.unsaved {
    margin-top: var(--space);
  }
  .note.unsaved::before {
    display: none;
  }

  /* ===== unanalyzed ===== */
  .analyzing {
    font-size: 11px;
    color: var(--muted);
    font-style: italic;
  }

  .error {
    font-size: 11px;
    color: var(--miss);
    max-width: 60ch;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space);
    margin-top: var(--space);
  }
</style>
