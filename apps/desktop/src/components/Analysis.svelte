<script lang="ts">
  // Structure box: owns the analyze lifecycle. Empty → CTA, running → live
  // progress, done → a musical summary (results, not perf stats). Sits beside
  // the stem mixer; both are products of the one analyze action.
  import { actions, analysisError, openSong, prepareState, sectionsOpen } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import LiveProgress from "./LiveProgress.svelte";

  let analysis = $derived($openSong?.analysis ?? null);
  let running = $derived($prepareState !== null);

  // time signature ≈ beats per bar (beats / downbeats), when it's sane
  let meter = $derived.by(() => {
    const a = analysis;
    if (!a?.beats?.length || !a?.downbeats?.length) return null;
    const per = Math.round(a.beats.length / a.downbeats.length);
    return per >= 2 && per <= 12 ? `${per}/4` : null;
  });
</script>

{#if $openSong}
  <section class="box">
    <div class="head">
      <span class="lbl">structure</span>
      {#if analysis && !running}
        <button class="mini" onclick={() => void actions.reanalyze()} title="re-run analysis">
          re-analyze
        </button>
      {/if}
    </div>
    <div class="body">
      {#if running}
        <LiveProgress />
      {:else if analysis}
        <div class="stats">
          {#if analysis.bpm}
            <span class="stat"><b>{Math.round(analysis.bpm)}</b><span class="k">bpm</span></span>
          {/if}
          {#if meter}
            <span class="stat"><b>{meter}</b><span class="k">meter</span></span>
          {/if}
          <span class="stat"><b>{analysis.beats.length}</b><span class="k">beats</span></span>
          <span class="stat"><b>{analysis.downbeats.length}</b><span class="k">bars</span></span>
          <button class="stat link" onclick={() => sectionsOpen.set(true)} title="edit in the sections tab">
            <b>{analysis.sections.length}</b><span class="k">sections</span>
          </button>
        </div>
        <p class="foot mono">{analysis.engine}</p>
      {:else}
        <div class="empty">
          <span class="big">this track hasn’t been analyzed</span>
          <Button accent onclick={() => void actions.prepare()}>Analyze track</Button>
          <span class="tiny">structure + stems</span>
        </div>
      {/if}
      {#if $analysisError}<p class="err mono">{$analysisError}</p>{/if}
    </div>
  </section>
{/if}

<style>
  .box {
    flex: 1 1 0;
    min-width: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--bg-raised);
    display: flex;
    flex-direction: column;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
  }

  .lbl {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .mini {
    background: none;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    color: var(--muted);
    font-size: 11px;
    padding: 1px 8px;
    cursor: pointer;
  }
  .mini:hover {
    color: var(--fg);
    border-color: var(--muted);
  }

  .body {
    padding: 10px;
    min-width: 0;
  }

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    align-items: baseline;
  }

  .stat {
    display: flex;
    align-items: baseline;
    gap: 5px;
  }
  .stat b {
    font-family: var(--mono);
    font-size: 18px;
    color: var(--fg);
    font-weight: 600;
  }
  .stat .k {
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  /* the sections count doubles as the jump-to-sections-tab affordance */
  .stat.link {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
  }
  .stat.link:hover b,
  .stat.link:hover .k {
    color: var(--accent);
  }

  .foot {
    margin: 8px 0 0;
    font-size: 10px;
    color: var(--muted);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  .empty .big {
    font-size: 13px;
    color: var(--muted);
  }
  .empty .tiny {
    font-size: 11px;
    color: var(--muted);
  }

  .err {
    font-size: 11px;
    color: var(--miss);
    margin: 8px 0 0;
    max-width: 60ch;
  }
</style>
