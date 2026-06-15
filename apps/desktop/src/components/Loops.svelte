<script lang="ts">
  import { actions, currentLoop, openSong } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import EmptyState from "../lib/ui/EmptyState.svelte";

  let renamingId = $state<number | null>(null);
  let renameValue = $state("");

  function startRename(id: number) {
    renamingId = id;
    renameValue = ""; // empty + submit reverts to the dynamic name
  }

  async function commitRename() {
    if (renamingId === null) return;
    const id = renamingId;
    renamingId = null;
    // empty string clears the override server-side (back to the dynamic name)
    await actions.updateLoop(id, { name: renameValue.trim() });
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
  <EmptyState>open a song first</EmptyState>
{:else}
  <ul>
    {#each $openSong.loops as l (l.id)}
      <li class="row" class:current={$currentLoop?.id === l.id}>
        {#if renamingId === l.id}
          <input
            class="name"
            use:focusNode
            bind:value={renameValue}
            placeholder={l.name}
            onblur={() => commitRename()}
            onkeydown={(e) => {
              if (e.key === "Enter") commitRename();
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
          </button>
        {/if}
        {#if $openSong.sections.length > 0}
          <Button variant="chip" onclick={() => actions.fitLoop(l.id)} title="snap edges to sections">fit</Button>
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
{/if}

<style>

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

  .t {
    width: 4em;
    font-size: 11px;
    padding: 1px 4px;
  }
</style>
