<script lang="ts">
  // Stem mixer strip: four channels (vocals/drums/bass/other) with level,
  // mute, solo — all changes collapse into one stems.gains call. When the
  // song has no cached stems yet, a single quiet "Separate stems" button.
  import {
    actions,
    BASS_STEM,
    openSong,
    STEM_LABELS,
    stemMix,
    stemsError,
    stemsRunning,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";
  import Toolbar from "../lib/ui/Toolbar.svelte";
</script>

{#if $openSong}
  <div class="stems">
    <Toolbar>
      {#if $openSong.stems}
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
      {:else if $stemsRunning}
        <span class="status mono">separating stems…</span>
      {:else}
        <Button variant="chip" onclick={() => actions.separateStems()}>Separate stems</Button>
      {/if}
      {#if $stemsError}
        <span class="error">{$stemsError}</span>
      {/if}
    </Toolbar>
  </div>
{/if}

<style>
  .stems {
    padding: var(--space) 0;
    border-bottom: 1px solid var(--line);
    min-height: 32px;
    min-width: 0;
  }

  .channel {
    display: flex;
    flex-direction: column;
    align-items: center;
    flex: 0 0 auto;
    gap: 4px;
  }

  .fader {
    height: 96px;
  }

  .name {
    font-size: 10px;
    letter-spacing: 0.06em;
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
    color: var(--accent);
    max-width: 60ch;
  }
</style>
