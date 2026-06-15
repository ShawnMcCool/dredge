<script lang="ts">
  // Pre-analysis call to action. Shown in place of the stems box while a track
  // has no analysis data yet — it owns the analyze lifecycle (idle CTA → live
  // progress) until the first results land, at which point App swaps in the
  // detail boxes.
  import { actions, analysisError, openSong, prepareState } from "../lib/stores";
  import Box from "../lib/ui/Box.svelte";
  import Button from "../lib/ui/Button.svelte";
  import LiveProgress from "./LiveProgress.svelte";

  let running = $derived($prepareState !== null);
</script>

{#if $openSong}
  <Box label="analyze">
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
  </Box>
{/if}

<style>
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
