<script lang="ts">
  // Pre-analysis call to action. Shown in place of the stems + structure boxes
  // while a track has no analysis data yet — it owns the analyze lifecycle (idle
  // CTA → live progress) until the first results land, at which point App swaps
  // in the detail boxes. Both of those boxes are products of this one action.
  import { actions, analysisError, openSong, prepareState } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import LiveProgress from "./LiveProgress.svelte";

  let running = $derived($prepareState !== null);
</script>

{#if $openSong}
  <section class="box">
    <div class="head"><span class="lbl">analyze</span></div>
    <div class="body">
      {#if running}
        <LiveProgress />
      {:else}
        <div class="cta">
          <div class="copy">
            <span class="big">this track hasn’t been analyzed yet</span>
            <span class="sub"
              >detect beats, downbeats &amp; sections, and split into vocals / drums / bass / other stems</span
            >
          </div>
          <Button accent onclick={() => void actions.prepare()}>Analyze track</Button>
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
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
  }

  .lbl {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .body {
    padding: 10px;
    min-width: 0;
  }

  .cta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .copy {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .copy .big {
    font-size: 13px;
    color: var(--fg);
  }
  .copy .sub {
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
