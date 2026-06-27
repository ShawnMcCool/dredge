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
  <Box id="isolation" label="isolation" grow={!hasStems}>
    {#snippet tools()}
      {#if hasStems}
        <button
          onclick={() => void actions.resetStemMix()}
          title="reset stems — all faders to 100%, clear mute/solo"
          aria-label="reset stems"
        >
          ⟲
        </button>
      {/if}
    {/snippet}

    <!-- tier 0: bass focus — always available, no separation needed -->
    <div class="focus">
      <span class="focus-label mono">bass focus</span>
      <Button
        variant="toggle"
        active={$bassFocus}
        onclick={() => actions.bassFocus(!$bassFocus)}
        title="bass focus — low-pass the mix so the bassline reads"
        aria-pressed={$bassFocus}
      >
        {$bassFocus ? "on" : "off"}
      </Button>
    </div>

    <div class="rule"></div>

    <!-- tier 1: separated stems, or the path to them -->
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
      <LiveProgress />
    {:else if $openSong.analysis}
      <p class="status mono">no stems for this track</p>
    {:else}
      <div class="cta">
        <span class="copy">separate into vocals / drums / bass / other to isolate parts</span>
        <Button accent onclick={() => void actions.prepare()}>Analyze track</Button>
      </div>
    {/if}

    {#if $stemsError}<p class="error">{$stemsError}</p>{/if}
    {#if $analysisError}<p class="error">{$analysisError}</p>{/if}
  </Box>
{/if}

<style>
  .focus {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .focus-label {
    font-size: 11px;
    letter-spacing: 0.05em;
    color: var(--muted);
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

  .status {
    font-size: 11px;
    color: var(--muted);
  }

  .error {
    font-size: 11px;
    color: var(--miss);
    max-width: 60ch;
    margin: 6px 0 0;
  }
</style>
