<script lang="ts">
  import {
    actions,
    analysisError,
    openSong,
    selection,
    suggestedSections,
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
  let lastSongId: number | null = null;

  // mirror the store unless there are unsaved edits
  $effect(() => {
    const open = $openSong;
    if (!open) {
      rows = [];
      dirty = false;
      lastSongId = null;
      return;
    }
    const switching = open.song.id !== lastSongId;
    if (switching || !dirty) {
      lastSongId = open.song.id;
      if (open.sections.length > 0) {
        rows = open.sections.map((s) => ({ name: s.name, start: s.start, end: s.end }));
        if (switching) dirty = false;
      } else {
        // no saved sections yet — seed the editor from cached analysis so the
        // structure is editable without re-running the model (provisional rows)
        rows = (open.analysis?.sections ?? []).map(toRow);
        dirty = rows.length > 0;
      }
    }
  });

  // fresh analysis suggestions land as prefilled UNSAVED rows: appended
  // below any existing sections, persisted only by the normal save
  $effect(() => {
    const suggestions = $suggestedSections;
    if (!suggestions || !$openSong) return;
    suggestedSections.set(null);
    rows = [...rows.filter((r) => !r.suggested), ...suggestions.map(toRow)];
    if (rows.some((r) => r.suggested)) touch();
  });

  function toRow(s: AnalysisSection): Row {
    return {
      name: s.label,
      start: Math.round(s.start * 10) / 10,
      end: Math.round(s.end * 10) / 10,
      suggested: true,
    };
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
  }

  let confirmReanalyze = $state(false);

  async function reanalyze() {
    confirmReanalyze = false;
    await actions.reanalyze();
  }
</script>

<h2>sections</h2>
{#if !$openSong}
  <p class="empty">open a song first</p>
{:else}
  {#if engineLabel}
    <p class="engine mono" class:fallback={engineLabel.startsWith("novelty")}>
      sections: {engineLabel}
    </p>
  {/if}
  <ul>
    {#each rows as row, i (i)}
      <li class="row" class:suggested={row.suggested}>
        <input class="name" bind:value={row.name} oninput={touch} />
        <input
          class="mono t"
          type="number"
          step="0.1"
          min="0"
          bind:value={row.start}
          oninput={touch}
        />
        <input
          class="mono t"
          type="number"
          step="0.1"
          min="0"
          bind:value={row.end}
          oninput={touch}
        />
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

  <Modal open={confirmReanalyze} title="re-analyze" closable onclose={() => (confirmReanalyze = false)}>
    <p>Discard the cached beat grid and section suggestions and run analysis again?</p>
    <div class="modal-actions">
      <Button onclick={() => (confirmReanalyze = false)}>cancel</Button>
      <Button accent onclick={reanalyze}>re-analyze</Button>
    </div>
  </Modal>
  <p class="note">saving re-derives junction loops (bar-aware once analyzed)</p>
{/if}

<style>
  .empty,
  .note {
    font-size: 11px;
    color: var(--muted);
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
