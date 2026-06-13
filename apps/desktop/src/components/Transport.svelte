<script lang="ts">
  import { actions, bassFocus, muted, openSong, pitch, playbackVolume, position } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";

  const RATE_PRESETS = [0.5, 0.7, 0.85, 1.0];

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

  // pitch stepper: ± a semitone; scroll over it for ±5 cents
  const clampSt = (st: number) => Math.max(-12, Math.min(12, st));
  function stepPitch(d: number) {
    void actions.setPitch(clampSt($pitch.semitones + d), $pitch.cents);
  }
  function pitchWheel(e: WheelEvent) {
    e.preventDefault();
    const cents = Math.max(-100, Math.min(100, $pitch.cents + (e.deltaY < 0 ? 5 : -5)));
    void actions.setPitch($pitch.semitones, cents);
  }

  let pitchLabel = $derived(
    `${$pitch.semitones > 0 ? "+" : ""}${$pitch.semitones} st` +
      ($pitch.cents ? ` ${$pitch.cents > 0 ? "+" : ""}${$pitch.cents}¢` : ""),
  );
</script>

<div class="transport">
  <div class="bar">
    <!-- primary: the constantly-touched controls, given room -->
    <div class="grp primary">
      <Button
        variant="icon"
        style="width: 50px; height: 38px; font-size: 18px;"
        onclick={() => ($position.playing ? actions.pause() : actions.play())}
      >
        {$position.playing ? "⏸" : "▶"}
      </Button>
      <span class="readout time">
        {fmt($position.secs)} <span class="dim">/ {fmtTotal($openSong?.song.duration_secs ?? 0)}</span>
      </span>

      <span class="vsep"></span>

      <button
        class="spk"
        class:muted={$muted}
        onclick={() => actions.mute(!$muted)}
        title={$muted ? "unmute" : "mute"}
        aria-label={$muted ? "unmute" : "mute"}
      >
        {$muted ? "🔇" : "🔊"}
      </button>
      <span class="vol">
        <Fader
          value={$playbackVolume}
          min={0}
          max={1.5}
          step={0.05}
          onchange={(v) => void actions.setVolume(v)}
          format={(v) => `volume ${Math.round(v * 100)}%`}
        />
      </span>
      <span class="readout volpct">{Math.round($playbackVolume * 100)}%</span>

      <span class="vsep"></span>

      <Button variant="chip" active={$bassFocus} onclick={() => actions.bassFocus(!$bassFocus)}>
        bass
      </Button>
    </div>

    <!-- tools: occasional controls, compact -->
    <div class="grp tools">
      <span class="lbl">speed</span>
      <span class="presets">
        {#each RATE_PRESETS as r (r)}
          <Button
            variant="chip"
            active={Math.abs($position.rate - r) < 0.001}
            onclick={() => actions.setRate(r)}
          >
            {Math.round(r * 100)}
          </Button>
        {/each}
      </span>
      <span class="speed-fader">
        <Fader
          value={$position.rate}
          min={0.25}
          max={2}
          step={0.05}
          accent
          onchange={(v) => void actions.setRate(v)}
          format={(v) => `speed ${Math.round(v * 100)}%`}
        />
      </span>
      <span class="readout rate">{Math.round($position.rate * 100)}%</span>

      <span class="vsep"></span>

      <span class="lbl">pitch</span>
      <span class="stepper" onwheel={pitchWheel} title="± semitone · scroll for cents">
        <Button variant="chip" onclick={() => stepPitch(-1)}>−</Button>
        <span class="readout pitchval">{pitchLabel}</span>
        <Button variant="chip" onclick={() => stepPitch(1)}>+</Button>
      </span>
    </div>
  </div>
</div>

<style>
  .transport {
    flex: 0 0 auto;
    padding: var(--space) 0;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  /* one row that wraps the tools group to a second tier when width runs out */
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 18px;
  }

  .grp {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .primary,
  .tools {
    flex: 1 1 auto;
  }

  .vsep {
    width: 1px;
    align-self: stretch;
    min-height: 22px;
    background: var(--line);
    flex: 0 0 auto;
  }

  /* volume is generous (used often); speed slider stays compact (presets carry it) */
  .vol {
    display: flex;
    flex: 1 1 120px;
    max-width: 240px;
  }

  .speed-fader {
    display: flex;
    flex: 0 0 auto;
    width: 80px;
  }

  .presets {
    display: inline-flex;
    gap: calc(var(--space) / 2);
  }

  .spk {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 16px;
    padding: 0 2px;
    line-height: 1;
  }
  .spk:hover {
    color: var(--fg);
  }
  .spk.muted {
    color: var(--accent);
  }

  .lbl {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    flex: 0 0 auto;
  }

  .time {
    color: var(--fg);
    flex: 0 0 auto;
  }
  .time .dim {
    color: var(--muted);
  }

  .rate {
    color: var(--accent);
    min-width: 4ch;
    text-align: right;
  }

  .volpct {
    color: var(--muted);
    min-width: 4ch;
    text-align: right;
  }

  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .pitchval {
    min-width: 5ch;
    text-align: center;
    color: var(--fg);
  }
</style>
