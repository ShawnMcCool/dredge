<script lang="ts">
  import { fmtClock } from "../lib/format";
  import { actions, activeLoop, countIn, countInAvailable, muted, openSong, pitch, playbackVolume, position } from "../lib/stores";
  import { stepCountInBeats } from "../lib/count-in";
  import Fader from "../lib/ui/Fader.svelte";

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

  function stepCount(d: number) {
    void actions.setCountIn({ beats: stepCountInBeats($countIn.beats, d) });
  }
  let countLabel = $derived(`${$countIn.beats} beat${$countIn.beats === 1 ? "" : "s"}`);
</script>

<div class="transport">
  <div class="bar">
    <!-- player: the whole cell toggles play -->
    <button
      class="seg player"
      onclick={() => ($position.playing ? actions.pause() : actions.play())}
      title={$position.playing ? "pause (Space)" : "play (Space)"}
      aria-label={$position.playing ? "pause" : "play"}
    >
      <span class="play">
        {#if $position.playing}
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true">
            <line x1="8.5" y1="5" x2="8.5" y2="19" />
            <line x1="15.5" y1="5" x2="15.5" y2="19" />
          </svg>
        {:else}
          <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M7 4.5 19 12 7 19.5z" />
          </svg>
        {/if}
      </span>
      <span class="time">
        <span class="now mono">{fmtClock($position.secs)}</span>
        <span class="total mono">/ {fmtClock($openSong?.song.duration_secs ?? 0, 0)}</span>
      </span>
    </button>

    <span class="vsep"></span>

    <!-- volume -->
    <div class="seg">
      <span class="mlabel">volume <span class="val accent">{Math.round($playbackVolume * 100)}%</span></span>
      <div class="mbody">
        <button
          class="iconbtn"
          class:muted={$muted}
          onclick={() => actions.mute(!$muted)}
          title={$muted ? "unmute" : "mute"}
          aria-label={$muted ? "unmute" : "mute"}
        >
          {#if $muted}
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path fill="currentColor" d="M3 9v6h4l5 4V5L7 9z" />
              <path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M16 9.5 21 14.5M21 9.5 16 14.5" />
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path fill="currentColor" d="M3 9v6h4l5 4V5L7 9z" />
              <path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M15.5 8.5a5 5 0 0 1 0 7" />
              <path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M18.5 6a9 9 0 0 1 0 12" />
            </svg>
          {/if}
        </button>
        <span class="fader">
          <Fader
            value={$playbackVolume}
            min={0}
            max={1.5}
            step={0.05}
            accent
            onchange={(v) => void actions.setVolume(v)}
            format={(v) => `volume ${Math.round(v * 100)}%`}
          />
        </span>
      </div>
    </div>

    <span class="vsep"></span>

    <!-- speed -->
    <div class="seg">
      <span class="mlabel">speed <span class="val">{Math.round($position.rate * 100)}%</span></span>
      <div class="mbody">
        <span class="fader">
          <Fader
            value={$position.rate}
            min={0.25}
            max={2}
            step={0.05}
            onchange={(v) => void actions.setRate(v)}
            format={(v) => `speed ${Math.round(v * 100)}%`}
          />
        </span>
      </div>
    </div>

    <span class="vsep"></span>

    <!-- pitch -->
    <div class="seg">
      <span class="mlabel">pitch</span>
      <div class="mbody">
        <span class="stepper" onwheel={pitchWheel} title="± semitone · scroll for cents">
          <button onclick={() => stepPitch(-1)} aria-label="pitch down">−</button>
          <span class="pval mono">{pitchLabel}</span>
          <button onclick={() => stepPitch(1)} aria-label="pitch up">+</button>
        </span>
      </div>
    </div>

    {#if $countInAvailable}
      <span class="vsep"></span>

      <!-- count in: beats of clicks before playback -->
      <div class="seg">
        <span class="mlabel">count in</span>
        <div class="mbody">
          <button
            class="toggle"
            class:on={$countIn.enabled}
            onclick={() => actions.setCountIn({ enabled: !$countIn.enabled })}
            title="count in before playback"
          >{$countIn.enabled ? "on" : "off"}</button>
          <span class="stepper" class:off={!$countIn.enabled} title="beats before playback">
            <button onclick={() => stepCount(-1)} aria-label="fewer count-in beats">−</button>
            <span class="pval mono">{countLabel}</span>
            <button onclick={() => stepCount(1)} aria-label="more count-in beats">+</button>
          </span>
          {#if $countIn.enabled && $activeLoop}
            <button
              class="loopmode"
              onclick={() =>
                actions.setCountIn({ loopMode: $countIn.loopMode === "first" ? "every" : "first" })}
              title="count in on the first pass, or before every loop"
            >{$countIn.loopMode}</button>
          {/if}
        </div>
      </div>
    {/if}

    <button
      class="reset"
      onclick={() => actions.resetWorkspace()}
      title="reset workspace — fit zoom, clear selection, loop & playhead"
      aria-label="reset workspace"
    >
      ⟲
    </button>
  </div>
</div>

<style>
  .transport {
    flex: 0 0 auto;
    padding: var(--space) 0;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  /* a row of cells divided by the hairline separator; wraps when width runs out */
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
  }

  .seg {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  /* the play cell is itself a single button — strip the default button chrome
     so it reads as a plain cell */
  .seg.player {
    background: none;
    border: none;
    border-radius: 0;
    padding: 0;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }

  .vsep {
    width: 1px;
    align-self: stretch;
    min-height: 38px;
    background: var(--line);
    flex: 0 0 auto;
  }

  /* player: amber play disc + the time, inline; whole cell is the hit target */
  .player {
    flex-direction: row;
    align-items: center;
    gap: 12px;
  }

  .play {
    width: 36px;
    height: 36px;
    flex: 0 0 auto;
    border-radius: 50%;
    background: var(--accent);
    color: var(--bg);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .player:hover .play {
    filter: brightness(1.1);
  }
  .play svg {
    width: 18px;
    height: 18px;
  }

  .time {
    display: flex;
    flex-direction: column;
    line-height: 1.15;
  }
  .now {
    font-size: 15px;
    color: var(--fg);
    font-variant-numeric: tabular-nums;
  }
  .total {
    font-size: 11px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  /* module label + optional live value */
  .mlabel {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .mlabel .val {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--muted);
    text-transform: none;
    letter-spacing: 0;
  }
  .mlabel .val.accent {
    color: var(--accent);
  }

  .mbody {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
  }

  .fader {
    display: flex;
    width: 120px;
  }

  .iconbtn {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    display: flex;
    flex: 0 0 auto;
  }
  .iconbtn:hover {
    color: var(--fg);
  }
  .iconbtn.muted {
    color: var(--accent);
  }
  .iconbtn svg {
    width: 18px;
    height: 18px;
  }

  /* pitch stepper: bare − value + — no surrounding box or internal rules */
  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .stepper button {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 2px 4px;
    font-size: 15px;
    line-height: 1;
  }
  .stepper button:hover {
    color: var(--fg);
  }
  .pval {
    min-width: 5ch;
    text-align: center;
    font-size: 13px;
    color: var(--fg);
  }

  /* count-in on/off: a small pill, accent when on, so the beat count is kept
     while disabled */
  .toggle {
    background: none;
    border: 1px solid var(--line);
    color: var(--muted);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
  }
  .toggle:hover {
    color: var(--fg);
  }
  .toggle.on {
    border-color: var(--accent);
    color: var(--accent);
  }

  /* the beats stepper dims while count-in is off, but stays adjustable */
  .stepper.off {
    opacity: 0.4;
  }

  /* count-in loop mode: a small first/every toggle, on while a loop is active */
  .loopmode {
    background: none;
    border: 1px solid var(--line);
    color: var(--muted);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
  }
  .loopmode:hover {
    color: var(--fg);
  }

  /* quiet recovery affordance, pushed to the far end */
  .reset {
    margin-left: auto;
    align-self: center;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 4px;
    font-size: 17px;
    line-height: 1;
    opacity: 0.6;
    flex: 0 0 auto;
  }
  .reset:hover {
    color: var(--fg);
    opacity: 1;
  }
</style>
