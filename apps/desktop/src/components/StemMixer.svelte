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
</script>

{#if $openSong}
  <div class="stems">
    {#if $openSong.stems}
      {#each STEM_LABELS as label, i (label)}
        <div class="channel" class:bass={i === BASS_STEM}>
          <input
            class="fader"
            type="range"
            min="0"
            max="100"
            value={$stemMix.levels[i]}
            oninput={(e) => actions.setStemLevel(i, Number(e.currentTarget.value))}
            title="{label} level"
          />
          <span class="name mono">{label}</span>
          <span class="toggles">
            <button
              class="chip"
              class:on={$stemMix.mutes[i]}
              onclick={() => actions.toggleStemMute(i)}
              title="mute {label}"
            >
              M
            </button>
            <button
              class="chip"
              class:on={$stemMix.solos[i]}
              onclick={() => actions.toggleStemSolo(i)}
              title="solo {label}"
            >
              S
            </button>
          </span>
        </div>
      {/each}
    {:else if $stemsRunning}
      <span class="status mono">separating stems…</span>
    {:else}
      <button class="separate" onclick={() => actions.separateStems()}>Separate stems</button>
    {/if}
    {#if $stemsError}
      <span class="error">{$stemsError}</span>
    {/if}
  </div>
{/if}

<style>
  .stems {
    display: flex;
    align-items: center;
    gap: calc(var(--space) * 2);
    padding: var(--space) 0;
    border-bottom: 1px solid var(--line);
    min-height: 32px;
  }

  .channel {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .fader {
    writing-mode: vertical-lr;
    direction: rtl;
    height: 64px;
    width: 16px;
    accent-color: var(--muted);
  }

  .channel.bass .fader {
    accent-color: var(--accent);
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

  .chip {
    font-family: var(--mono);
    font-size: 10px;
    padding: 0 4px;
  }

  .on {
    color: var(--bg);
    background: var(--accent);
    border-color: var(--accent);
  }

  .separate {
    font-size: 11px;
    color: var(--muted);
    background: none;
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
