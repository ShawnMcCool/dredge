<script lang="ts">
  import { actions, openSong, selection } from "../lib/stores";

  interface Row {
    name: string;
    start: number;
    end: number;
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
    await actions.replaceSections(rows.map((r, i) => ({ ...r, position: i })));
    dirty = false;
  }
</script>

<h2>sections</h2>
{#if !$openSong}
  <p class="empty">open a song first</p>
{:else}
  <ul>
    {#each rows as row, i (i)}
      <li class="row">
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
        <button onclick={() => move(i, -1)} title="up">↑</button>
        <button onclick={() => move(i, 1)} title="down">↓</button>
        <button onclick={() => remove(i)} title="delete">×</button>
      </li>
    {/each}
  </ul>
  <div class="bar">
    <button onclick={add}>+ add</button>
    <button class="accent" disabled={!dirty} onclick={save}>save</button>
  </div>
  <p class="note">saving re-derives junction loops</p>
{/if}

<style>
  .empty,
  .note {
    font-size: 11px;
    color: var(--muted);
  }

  .row {
    display: flex;
    gap: calc(var(--space) / 2);
    margin-bottom: calc(var(--space) / 2);
  }

  .row input {
    font-size: 12px;
    padding: 2px 4px;
  }

  .name {
    flex: 1;
    min-width: 0;
  }

  .t {
    width: 4.5em;
  }

  .row button {
    padding: 1px 5px;
    font-size: 11px;
  }

  .bar {
    display: flex;
    gap: calc(var(--space) / 2);
    margin-top: var(--space);
  }
</style>
