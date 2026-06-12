<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { actions, due, loopName, openSong, retention, songs } from "../lib/stores";

  onMount(() => {
    void actions.refreshDue();
  });

  function songTitle(songId: number): string {
    return $songs.find((s) => s.id === songId)?.title ?? `song ${songId}`;
  }

  async function jump(songId: number, loopId: number) {
    if (get(openSong)?.song.id !== songId) await actions.openSong(songId);
    const l = get(openSong)?.loops.find((x) => x.id === loopId);
    if (l) await actions.selectLoop(l);
  }
</script>

<h2>due today</h2>
{#if $due.length === 0}
  <p class="empty">nothing due</p>
{:else}
  <ul>
    {#each $due as d (d.loop_id)}
      <li>
        <button class="due-item" onclick={() => jump(d.song_id, d.loop_id)}>
          {d.name}
          <span class="muted">{songTitle(d.song_id)}</span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

{#if $openSong}
  <h2 class="retention-h">retention</h2>
  {#if $retention.length === 0}
    <p class="empty">no retests yet</p>
  {:else}
    <table class="mono">
      <tbody>
        {#each $retention as row, i (i)}
          <tr>
            <td>{loopName(row.loop_id)}</td>
            <td class={row.rating}>{row.rating}</td>
            <td class="muted">{row.at.slice(0, 10)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
{/if}

<p class="why">rotating sections and next-day retests feel worse and work better.</p>

<style>
  .empty {
    font-size: 11px;
    color: var(--muted);
  }

  .due-item {
    display: flex;
    justify-content: space-between;
    width: 100%;
    background: none;
    border: none;
    text-align: left;
    padding: calc(var(--space) / 2);
  }

  .due-item:hover {
    background: var(--bg-raised);
  }

  .muted {
    color: var(--muted);
    font-size: 11px;
  }

  .retention-h {
    margin-top: calc(var(--space) * 2);
  }

  table {
    width: 100%;
    font-size: 11px;
    border-collapse: collapse;
  }

  td {
    padding: 2px 4px;
  }

  .miss {
    color: var(--miss);
  }

  .shaky {
    color: var(--shaky);
  }

  .solid {
    color: var(--solid);
  }

  .why {
    margin-top: calc(var(--space) * 3);
    font-size: 11px;
    color: var(--muted);
    font-style: italic;
  }
</style>
