<script lang="ts">
  // Isolation box: ways to hear less of the mix so one part reads clearly.
  // Two tiers, low to high capability:
  //   - bass focus  — a low-pass on the whole mix; works on any track, instantly.
  //   - stem channels — true vocals/drums/bass/other separation, once Demucs has
  //     run (all level/mute/solo changes collapse into one stems.gains call).
  // The box is always present while a song is open; before separation it still
  // offers bass focus and invites the analyze run that unlocks the stems.
  import {
    actions,
    analysisError,
    bassFocus,
    openSong,
    prepareState,
    STEM_LABELS,
    stemMix,
    stemsError,
  } from "../lib/stores";
  import Box from "../lib/ui/Box.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";
  import LiveProgress from "./LiveProgress.svelte";

  let hasStems = $derived(!!$openSong?.stems);
  let analyzing = $derived($prepareState !== null);
</script>

{#if $openSong}
  <!-- while a prepare run is live the box goes wide: the analyzing readout
       (step rows + meter traces) needs a full row to breathe, not a 240px
       share of one -->
  <Box id="isolation" grow={!hasStems} wide={analyzing}>
    <!-- tier 0: bass focus — always available, no separation needed. The label
         IS the toggle; the stem reset rides the empty space on its right. -->
    <div class="focus">
      <Button
        variant="toggle"
        active={$bassFocus}
        onclick={() => actions.bassFocus(!$bassFocus)}
        title="bass focus — low-pass the mix so the bassline reads"
        aria-pressed={$bassFocus}
      >
        bass focus
      </Button>
      {#if hasStems}
        <button
          class="reset"
          onclick={() => void actions.resetStemMix()}
          title="reset stems — all faders to 100%, clear mute/solo"
          aria-label="reset stems"
        >
          ⟲
        </button>
      {/if}
    </div>

    <div class="rule"></div>

    <!-- tier 1: separated stems, or the path to them. The analyzing readout
         renders below whichever state is showing (LiveProgress self-gates on
         prepareState), so a force re-separation of a song that still has
         stems stays visible instead of hiding behind the faders. -->
    {#if hasStems}
      <div class="channels">
        {#each STEM_LABELS as label, i (label)}
          <div class="channel">
            <div class="fader">
              <Fader
                orientation="vertical"
                value={$stemMix.levels[i] / 100}
                min={0}
                max={1}
                step={0.01}
                onchange={(v) => void actions.setStemLevel(i, Math.round(v * 100))}
                format={(v) => `${label} ${Math.round(v * 100)}%`}
              />
            </div>
            <span class="name mono">{label}</span>
            <span class="toggles">
              <Button
                variant="chip"
                active={$stemMix.mutes[i]}
                onclick={() => actions.toggleStemMute(i)}
                title="mute {label}"
              >
                M
              </Button>
              <Button
                variant="chip"
                active={$stemMix.solos[i]}
                onclick={() => actions.toggleStemSolo(i)}
                title="solo {label}"
              >
                S
              </Button>
            </span>
          </div>
        {/each}
      </div>
    {:else if analyzing}
      <!-- the readout below is the whole state while separating pre-stems -->
    {:else if $openSong.analysis}
      <!-- dead end: analyzed but no stems (failed run, deleted cache, copied
           bundle) — the only state that offers a rerun, so the happy path
           stays quiet -->
      <div class="cta">
        <span class="copy">no stems for this track</span>
        <Button accent onclick={() => void actions.prepare()}>Separate stems</Button>
      </div>
    {:else}
      <div class="cta">
        <span class="copy">separate into vocals / drums / bass / other to isolate parts</span>
        <Button accent onclick={() => void actions.prepare()}>Analyze track</Button>
      </div>
    {/if}

    <LiveProgress />

    {#if $stemsError}<p class="error">{$stemsError}</p>{/if}
    {#if $analysisError}<p class="error">{$analysisError}</p>{/if}
  </Box>
{/if}

<style>
  .focus {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  /* stem reset — a quiet glyph riding the right end of the bass-focus row */
  .reset {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 16px;
    line-height: 1;
    padding: 2px;
  }
  .reset:hover {
    color: var(--fg);
  }

  .rule {
    height: 1px;
    background: var(--line);
    margin: 10px 0;
  }

  .channels {
    display: flex;
    gap: 18px;
  }

  .channel {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .fader {
    height: 92px;
  }

  .name {
    font-size: 10px;
    letter-spacing: 0.05em;
    color: var(--muted);
  }

  .toggles {
    display: flex;
    gap: 2px;
  }

  .cta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .cta .copy {
    font-size: 11px;
    color: var(--muted);
    min-width: 0;
  }

  .error {
    font-size: 11px;
    color: var(--miss);
    max-width: 60ch;
    margin: 6px 0 0;
  }
</style>
