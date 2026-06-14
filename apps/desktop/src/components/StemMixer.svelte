<script lang="ts">
  // Stem mixer box: four channels (vocals/drums/bass/other) with level, mute,
  // solo — all changes collapse into one stems.gains call. Sits beside the
  // structure box; when there are no cached stems yet, points at analyze.
  import { actions, BASS_STEM, openSong, STEM_LABELS, stemMix, stemsError } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";
</script>

{#if $openSong}
  <section class="box">
    <div class="head">
      <span class="lbl">stems</span>
      {#if $openSong.stems}
        <Button
          variant="icon"
          onclick={() => void actions.resetStemMix()}
          title="reset stems — all faders to 100%, clear mute/solo"
          aria-label="reset stems"
        >
          ⟲
        </Button>
      {/if}
    </div>
    <div class="body">
      {#if $openSong.stems}
        <div class="channels">
          {#each STEM_LABELS as label, i (label)}
            <div class="channel" class:bass={i === BASS_STEM}>
              <div class="fader">
                <Fader
                  orientation="vertical"
                  value={$stemMix.levels[i] / 100}
                  min={0}
                  max={1}
                  step={0.01}
                  accent={i === BASS_STEM}
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
      {:else}
        <p class="status mono">no stems yet — analyze the track</p>
      {/if}
      {#if $stemsError}
        <p class="error">{$stemsError}</p>
      {/if}
    </div>
  </section>
{/if}

<style>
  .box {
    flex: 0 0 auto;
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
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .lbl {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .body {
    padding: 10px;
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

  .channel.bass .name {
    color: var(--accent);
  }

  .toggles {
    display: flex;
    gap: 2px;
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
