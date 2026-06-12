<script lang="ts">
  import {
    actions,
    analysisError,
    analysisRunning,
    openSong,
    selection,
    suggestedSections,
    type AnalysisSection,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";

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
    if (open.song.id !== lastSongId || !dirty) {
      lastSongId = open.song.id;
      rows = open.sections.map((s) => ({ name: s.name, start: s.start, end: s.end }));
      if (open.song.id !== lastSongId) dirty = false;
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

  let hasSaved = $derived(rows.some((r) => !r.suggested));
  let hasSuggested = $derived(rows.some((r) => r.suggested));

  function replaceWithSuggestions() {
    rows = rows.filter((r) => r.suggested);
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
</script>

<h2>sections</h2>
{#if !$openSong}
  <p class="empty">open a song first</p>
{:else}
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
    {#if $analysisRunning}
      <span class="status mono">analyzing…</span>
    {:else}
      <Button onclick={() => actions.runAnalysis()}>Analyze</Button>
    {/if}
    {#if hasSaved && hasSuggested}
      <Button onclick={replaceWithSuggestions}>replace with suggestions</Button>
    {/if}
    <Button accent disabled={!dirty} onclick={save}>save</Button>
  </div>
  {#if $analysisError}
    <p class="error">{$analysisError}</p>
  {/if}
  <p class="note">saving re-derives junction loops (bar-aware once analyzed)</p>
{/if}

<style>
  .empty,
  .note {
    font-size: 11px;
    color: var(--muted);
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

  .status {
    font-size: 11px;
    color: var(--muted);
    align-self: center;
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
</style>
