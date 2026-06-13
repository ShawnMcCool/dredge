<script lang="ts">
  import { onMount } from "svelte";
  import { actions, loopName, openSong, retention } from "../lib/stores";

  onMount(() => {
    void actions.refreshRetention();
  });
</script>

<h2>retention</h2>
{#if !$openSong}
  <p class="empty">open a song first</p>
{:else if $retention.length === 0}
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

<p class="why">rotating sections and next-day retests feel worse and work better.</p>

<style>
  .empty {
    font-size: 11px;
    color: var(--muted);
  }

  .muted {
    color: var(--muted);
    font-size: 11px;
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
