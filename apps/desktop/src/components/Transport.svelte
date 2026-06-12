<script lang="ts">
  import { actions, bassFocusOn, muted, openSong, pitch, position } from "../lib/stores";

  const RATE_PRESETS = [0.5, 0.7, 0.85, 1.0];
  const SEMITONE_CHIPS = [-2, -1, 0, 1, 2];

  function fmt(secs: number): string {
    const s = Math.max(secs, 0);
    const m = Math.floor(s / 60);
    const r = s - m * 60;
    return `${String(m).padStart(2, "0")}:${r.toFixed(1).padStart(4, "0")}`;
  }

  function fmtTotal(secs: number): string {
    const m = Math.floor(secs / 60);
    const r = Math.floor(secs % 60);
    return `${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`;
  }

  function onRateInput(e: Event) {
    void actions.setRate(Number((e.currentTarget as HTMLInputElement).value));
  }

  function onCentsInput(e: Event) {
    const cents = Number((e.currentTarget as HTMLInputElement).value);
    void actions.setPitch($pitch.semitones, cents);
  }
</script>

<div class="transport">
  <button class="play" onclick={() => ($position.playing ? actions.pause() : actions.play())}>
    {$position.playing ? "⏸" : "▶"}
  </button>

  <span class="mono time">
    {fmt($position.secs)} / {fmtTotal($openSong?.song.duration_secs ?? 0)}
  </span>

  <span class="group">
    <span class="mono readout">{Math.round($position.rate * 100)}%</span>
    <input
      type="range"
      min="0.25"
      max="2"
      step="0.05"
      value={$position.rate}
      oninput={onRateInput}
    />
    {#each RATE_PRESETS as r (r)}
      <button
        class="chip"
        class:on={Math.abs($position.rate - r) < 0.001}
        onclick={() => actions.setRate(r)}
      >
        {Math.round(r * 100)}
      </button>
    {/each}
  </span>

  <span class="group">
    <span class="label">pitch</span>
    {#each SEMITONE_CHIPS as st (st)}
      <button
        class="chip"
        class:on={$pitch.semitones === st}
        onclick={() => actions.setPitch(st, $pitch.cents)}
      >
        {st > 0 ? `+${st}` : st}
      </button>
    {/each}
    <input
      class="cents"
      type="number"
      min="-100"
      max="100"
      step="5"
      value={$pitch.cents}
      oninput={onCentsInput}
      title="cents"
    />
  </span>

  <span class="group">
    <button class:on={$bassFocusOn} onclick={() => actions.bassFocus(!$bassFocusOn)}>
      BASS FOCUS
    </button>
    <button class:on={$muted} onclick={() => actions.mute(!$muted)}>MUTE</button>
  </span>
</div>

<style>
  .transport {
    display: flex;
    align-items: center;
    gap: calc(var(--space) * 2);
    padding: var(--space) 0;
    border-bottom: 1px solid var(--line);
  }

  .play {
    width: 40px;
    font-size: 14px;
  }

  .time {
    color: var(--fg);
    min-width: 13ch;
  }

  .group {
    display: flex;
    align-items: center;
    gap: calc(var(--space) / 2);
  }

  .label {
    color: var(--muted);
    font-size: 12px;
  }

  .readout {
    color: var(--accent);
    min-width: 4ch;
    text-align: right;
  }

  input[type="range"] {
    width: 120px;
    accent-color: var(--accent);
  }

  .chip {
    font-family: var(--mono);
    font-size: 11px;
    padding: 1px 5px;
  }

  .on {
    color: var(--bg);
    background: var(--accent);
    border-color: var(--accent);
  }

  .cents {
    width: 4.5em;
    font-size: 11px;
    padding: 1px 4px;
  }
</style>
