<script lang="ts">
  import { untrack } from "svelte";
  import {
    actions,
    analysisError,
    openSong,
    selection,
    type AnalysisSection,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Modal from "../lib/ui/Modal.svelte";

  interface Row {
    name: string;
    start: number;
    end: number;
    /** Came from analysis and is not saved yet — muted accent in the UI. */
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

  /** Compact m:ss for the read-only display. */
  function fmtT(s: number): string {
    const m = Math.floor(s / 60);
    const r = Math.round(s % 60);
    return `${m}:${String(r).padStart(2, "0")}`;
  }

  /** Click a section in display mode → highlight its span on the waveform. */
  function highlight(row: Row) {
    selection.set({ start: row.start, end: row.end });
  }

  let hasAnalysis = $derived(($openSong?.analysis?.sections?.length ?? 0) > 0);

  let engineLabel = $derived.by(() => {
    const e = $openSong?.analysis?.engine;
    if (!e) return null;
    if (e === "songformer") return "SongFormer";
    if (e.includes("novelty")) return "novelty (SongFormer unavailable)";
    return e;
  });

  // revert the editor to the cached analysis — no model rerun needed, the
  // SongFormer result lives in the DB. Replaces the current (unsaved) edits.
  function revertToAnalysis() {
    rows = ($openSong?.analysis?.sections ?? []).map(toRow);
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
  <h2>sections</h2>
  {#if $openSong}
    <button class="edit-toggle" onclick={() => (editing = !editing)}>
      {editing ? "done" : "edit"}
    </button>
  {/if}
</div>
{#if !$openSong}
  <p class="empty">open a song first</p>
{:else}
  {#if engineLabel}
    <p class="engine mono" class:fallback={engineLabel.startsWith("novelty")}>
      sections: {engineLabel}
    </p>
  {/if}

  {#if editing}
    <ul>
      {#each rows as row, i (i)}
        <li class="row" class:suggested={row.suggested}>
          <input class="name" bind:value={row.name} oninput={touch} />
          <input class="mono t" type="number" step="0.1" min="0" bind:value={row.start} oninput={touch} />
          <input class="mono t" type="number" step="0.1" min="0" bind:value={row.end} oninput={touch} />
          <Button variant="chip" onclick={() => move(i, -1)} title="up">↑</Button>
          <Button variant="chip" onclick={() => move(i, 1)} title="down">↓</Button>
          <Button variant="chip" onclick={() => remove(i)} title="delete">×</Button>
        </li>
      {/each}
    </ul>
    <div class="bar">
      <Button onclick={add}>+ add</Button>
      {#if hasAnalysis}
        <Button onclick={revertToAnalysis} title="discard edits, reload the analyzed structure">
          revert to analysis
        </Button>
      {/if}
      <Button onclick={() => (confirmReanalyze = true)}>re-analyze</Button>
      <Button accent disabled={!dirty} onclick={save}>save</Button>
    </div>
    {#if $analysisError}
      <p class="error">{$analysisError}</p>
    {/if}
  {:else if rows.length === 0}
    <p class="empty">no sections yet — hit edit to add them, or analyze the song</p>
  {:else}
    <ul class="display">
      {#each rows as row, i (i)}
        <li class="drow" class:suggested={row.suggested}>
          <button class="row-btn" onclick={() => highlight(row)} title="highlight on the waveform">
            <span class="sec-name">{row.name}</span>
            <span class="sec-time mono">{fmtT(row.start)}–{fmtT(row.end)}</span>
          </button>
        </li>
      {/each}
    </ul>
    {#if dirty}<p class="note unsaved">unsaved edits — open edit to save</p>{/if}
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
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space);
  }

  .edit-toggle {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 2px 4px;
  }
  .edit-toggle:hover {
    color: var(--accent);
  }

  .empty,
  .note {
    font-size: 11px;
    color: var(--muted);
  }
  .note.unsaved {
    color: var(--shaky);
  }

  .engine {
    font-size: 10px;
    color: var(--muted);
    margin-bottom: calc(var(--space) / 2);
  }
  .engine.fallback {
    color: var(--shaky);
  }

  .row {
    display: flex;
    align-items: center;
    gap: calc(var(--space) / 2);
    margin-bottom: calc(var(--space) / 2);
    min-width: 0;
  }

  .row input {
    font-size: 12px;
    padding: 2px 4px;
    min-width: 0;
  }

  /* analysis suggestions: visibly provisional until saved */
  .row.suggested input {
    color: var(--muted);
    border-color: var(--accent-dim);
  }

  /* read-only display rows */
  .display {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .drow {
    padding: 0;
    min-width: 0;
  }
  .drow.suggested {
    color: var(--muted);
  }
  /* the whole row is the click target (highlight the section) */
  .row-btn {
    display: flex;
    width: 100%;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space);
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
    padding: 2px 4px;
    border-radius: var(--radius);
    min-width: 0;
  }
  .row-btn:hover {
    background: var(--bg-raised);
  }
  .row-btn:hover .sec-name {
    color: var(--accent);
  }
  .sec-name {
    font-size: 13px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sec-time {
    flex: 0 0 auto;
    font-size: 11px;
    color: var(--muted);
  }

  .error {
    font-size: 11px;
    color: var(--accent);
    max-width: 60ch;
  }

  .name {
    flex: 1;
    min-width: 0;
  }

  .t {
    width: 4.5em;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: calc(var(--space) / 2);
    margin-top: var(--space);
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space);
    margin-top: var(--space);
  }
</style>
