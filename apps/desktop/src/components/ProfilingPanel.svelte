<script lang="ts">
  import { profiles, songs } from "../lib/stores";
  import EmptyState from "../lib/ui/EmptyState.svelte";

  function songTitle(id?: number): string {
    if (id == null) return "";
    return $songs.find((s) => s.id === id)?.title ?? `song ${id}`;
  }

  function secs(ms: number): string {
    return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
  }
</script>

<h2>profiling</h2>
{#if $profiles.length === 0}
  <EmptyState>no runs yet</EmptyState>
{:else}
  <ul>
    {#each $profiles as run, i (i)}
      <li class="run" class:failed={!run.ok}>
        <div class="head">
          <span class="op">{run.op}</span>
          <span class="title">{songTitle(run.song_id)}</span>
          <span class="total mono">{secs(run.total_ms)}</span>
        </div>
        <div class="badges">
          {#if run.device}<span class="badge dev">{run.device}</span>{/if}
          {#if run.engine}<span class="badge eng">{run.engine}</span>{/if}
          {#if !run.ok}<span class="badge err">failed</span>{/if}
        </div>
        {#if run.stages.length}
          <div class="stages">
            {#each run.stages as st (st.name)}
              <div class="stage" title={st.note ?? ""}>
                <span class="sname mono">{st.name}</span>
                <span class="sms mono">{secs(st.ms)}</span>
              </div>
            {/each}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .run { padding: calc(var(--space) / 2) 0; border-bottom: 1px solid var(--bg-raised); }
  .head { display: flex; align-items: baseline; gap: var(--space); }
  .op { font-size: 12px; }
  .title { flex: 1; min-width: 0; font-size: 11px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .total { font-size: 11px; }
  .badges { display: flex; gap: 4px; margin-top: 2px; }
  .badge { font-size: 9px; padding: 1px 5px; border-radius: 8px; background: var(--bg-raised); color: var(--muted); }
  .badge.eng { color: var(--accent); }
  .badge.err { color: var(--miss); }
  .stages { margin-top: 4px; display: flex; flex-direction: column; gap: 2px; }
  .stage { display: flex; align-items: baseline; gap: 6px; }
  .sname { font-size: 10px; color: var(--muted); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sms { font-size: 10px; color: var(--muted); width: 4em; text-align: right; flex: 0 0 auto; }
</style>
