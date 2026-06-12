<script lang="ts">
  import { actions, currentLoop, openSong } from "../lib/stores";

  let tail = $state(2.0);
  let head = $state(2.0);
  let renamingId = $state<number | null>(null);
  let renameValue = $state("");

  function fmt(secs: number): string {
    return secs.toFixed(1);
  }

  function startRename(id: number, name: string) {
    renamingId = id;
    renameValue = name;
  }

  async function commitRename() {
    if (renamingId === null) return;
    const id = renamingId;
    renamingId = null;
    if (renameValue.trim()) await actions.updateLoop(id, { name: renameValue.trim() });
  }
</script>

<h2>loops</h2>
{#if !$openSong}
  <p class="empty">open a song first</p>
{:else}
  <ul>
    {#each $openSong.loops as l (l.id)}
      <li class="row" class:current={$currentLoop?.id === l.id}>
        {#if renamingId === l.id}
          <input
            class="name"
            bind:value={renameValue}
            onblur={commitRename}
            onkeydown={(e) => e.key === "Enter" && commitRename()}
          />
        {:else}
          <button class="name pick" onclick={() => actions.selectLoop(l)}>
            {l.name}
            {#if l.kind.kind === "junction"}<span class="badge">J</span>{/if}
          </button>
        {/if}
        <span class="mono span">{fmt(l.start)}–{fmt(l.end)}</span>
        <button onclick={() => startRename(l.id, l.name)} title="rename">✎</button>
        <button onclick={() => actions.deleteLoop(l.id)} title="delete">×</button>
      </li>
    {/each}
  </ul>
  <div class="derive">
    <button onclick={() => actions.deriveJunctions(tail, head)}>derive junctions</button>
    <label>tail <input class="mono t" type="number" step="0.5" min="0.5" bind:value={tail} /></label>
    <label>head <input class="mono t" type="number" step="0.5" min="0.5" bind:value={head} /></label>
  </div>
{/if}

<style>
  .empty {
    font-size: 11px;
    color: var(--muted);
  }

  .row {
    display: flex;
    align-items: center;
    gap: calc(var(--space) / 2);
    margin-bottom: calc(var(--space) / 2);
  }

  .row.current .pick {
    color: var(--accent);
  }

  .name {
    flex: 1;
    min-width: 0;
  }

  .pick {
    background: none;
    border: none;
    text-align: left;
    padding: 2px 4px;
  }

  .pick:hover {
    background: var(--bg-raised);
  }

  .badge {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--accent);
    border: 1px solid var(--accent-dim);
    border-radius: var(--radius);
    padding: 0 3px;
    margin-left: 4px;
  }

  .span {
    font-size: 11px;
    color: var(--muted);
  }

  .row button[title] {
    padding: 1px 5px;
    font-size: 11px;
  }

  .derive {
    display: flex;
    align-items: center;
    gap: calc(var(--space) / 2);
    margin-top: var(--space);
    font-size: 11px;
    color: var(--muted);
  }

  .t {
    width: 3.5em;
    font-size: 11px;
    padding: 1px 4px;
  }
</style>
