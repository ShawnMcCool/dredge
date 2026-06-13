<script lang="ts">
  import { actions, currentLoop, openSong } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";

  let tail = $state(2.0);
  let head = $state(2.0);
  let renamingId = $state<number | null>(null);
  let renameValue = $state("");

  // clearing a name reverts to the positional auto label
  const autoName = (i: number) => `loop ${i + 1}`;

  function startRename(id: number) {
    renamingId = id;
    renameValue = ""; // starts empty — type to set, leave blank for the auto label
  }

  async function commitRename(i: number) {
    if (renamingId === null) return;
    const id = renamingId;
    renamingId = null;
    await actions.updateLoop(id, { name: renameValue.trim() || autoName(i) });
  }

  function focusNode(node: HTMLInputElement) {
    node.focus();
  }

  async function setStart(id: number, v: number) {
    if (Number.isFinite(v)) await actions.updateLoop(id, { start: v });
  }
  async function setEnd(id: number, v: number) {
    if (Number.isFinite(v)) await actions.updateLoop(id, { end: v });
  }
</script>

<h2>loops</h2>
{#if !$openSong}
  <p class="empty">open a song first</p>
{:else}
  <ul>
    {#each $openSong.loops as l, i (l.id)}
      <li class="row" class:current={$currentLoop?.id === l.id}>
        {#if renamingId === l.id}
          <input
            class="name"
            use:focusNode
            bind:value={renameValue}
            placeholder={l.name}
            onblur={() => commitRename(i)}
            onkeydown={(e) => {
              if (e.key === "Enter") commitRename(i);
              else if (e.key === "Escape") renamingId = null;
            }}
          />
        {:else}
          <button
            class="name pick"
            onclick={() => actions.selectLoop(l)}
            ondblclick={() => startRename(l.id)}
            title="click to load · double-click to rename"
          >
            {l.name}
            {#if l.kind.kind === "junction"}<span class="badge">J</span>{/if}
          </button>
        {/if}
        <input
          class="mono t"
          type="number"
          step="0.1"
          min="0"
          value={l.start}
          onchange={(e) => setStart(l.id, Number(e.currentTarget.value))}
        />
        <input
          class="mono t"
          type="number"
          step="0.1"
          min="0"
          value={l.end}
          onchange={(e) => setEnd(l.id, Number(e.currentTarget.value))}
        />
        <Button variant="chip" onclick={() => actions.deleteLoop(l.id)} title="remove">×</Button>
      </li>
    {/each}
  </ul>
  <div class="derive">
    <Button onclick={() => actions.deriveJunctions(tail, head)}>derive junctions</Button>
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
    min-width: 0;
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

  .derive {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: calc(var(--space) / 2);
    margin-top: var(--space);
    font-size: 11px;
    color: var(--muted);
    min-width: 0;
  }

  .derive label {
    display: flex;
    align-items: center;
    gap: 3px;
    white-space: nowrap;
  }

  .t {
    width: 4em;
    font-size: 11px;
    padding: 1px 4px;
  }
</style>
