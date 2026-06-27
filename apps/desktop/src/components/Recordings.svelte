<script lang="ts">
  // Recordings box: capture your own input over the track as additive layers.
  // DAW-style flow — this box ARMS a take (span + input); the transport's record
  // button triggers it and becomes stop. One row per take below: name, level,
  // mute, nudge, delete. Recording covers one pass over the span, after count-in.
  import {
    actions,
    openSong,
    recordingActive,
    recordArmed,
    recordSpan,
    recordInput,
    recordings,
    selection,
    currentLoop,
    type AudioDevice,
  } from "../lib/stores";
  import { framesToMs, msToFrames } from "../lib/recording-math";
  import { cmd } from "../lib/ipc";
  import { traceErr } from "../lib/trace";
  import Box from "../lib/ui/Box.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";

  let devices = $state<AudioDevice[]>([]);

  $effect(() => {
    void cmd<AudioDevice[]>("device.inputs")
      .then((d) => {
        devices = d;
      })
      .catch((e) => traceErr("recordings", `device.inputs failed: ${e}`));
  });
</script>

{#if $openSong}
  <Box id="recordings" wide>
    <div class="bar">
      <select bind:value={$recordSpan} disabled={$recordingActive} aria-label="recording span">
        <option value="song">full song</option>
        <option value="playhead">from playhead</option>
        <option value="selection" disabled={!$selection}>selection</option>
        <option value="loop" disabled={!$currentLoop}>loop</option>
      </select>
      <select bind:value={$recordInput} disabled={$recordingActive} aria-label="input device">
        <option value="default">default (follow devices)</option>
        {#each devices as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
      </select>
      <Button
        variant="toggle"
        active={$recordArmed}
        disabled={$recordingActive}
        onclick={() => recordArmed.set(!$recordArmed)}
      >
        {$recordArmed ? "armed ✓" : "arm"}
      </Button>
    </div>
    {#if $recordArmed && !$recordingActive}
      <p class="hint">record from the transport</p>
    {/if}

    {#each $recordings as r (r.id)}
      <div class="row">
        <span class="name mono">{r.name}</span>
        <div class="fader">
          <Fader
            orientation="horizontal"
            value={r.gain}
            min={0}
            max={1.5}
            step={0.01}
            onchange={(v) => void actions.setRecordingGain(r.id, v)}
            format={(v) => `${r.name} ${Math.round(v * 100)}%`}
          />
        </div>
        <Button
          variant="chip"
          active={r.muted}
          aria-pressed={r.muted}
          onclick={() => void actions.toggleRecordingMute(r.id)}
          title="mute"
        >M</Button>
        <input
          class="nudge mono"
          type="number"
          step="1"
          value={Math.round(framesToMs(r.nudge_frames))}
          onchange={(e) => void actions.setRecordingNudge(r.id, msToFrames(+e.currentTarget.value))}
          title="nudge (ms)"
        />
        <button
          type="button"
          class="del"
          onclick={() => void actions.deleteRecording(r.id)}
          title="delete"
          aria-label="delete recording"
        >✕</button>
      </div>
    {/each}
  </Box>
{/if}

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  select {
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    height: var(--control-h);
    padding: 0 6px;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    /* Share the row and shrink with a native ellipsis instead of pushing the
       record button off the edge. The device list is the greedy one. */
    flex: 1 1 8em;
    min-width: 0;
    max-width: 100%;
  }

  select:disabled {
    color: var(--muted);
    cursor: default;
  }

  .hint {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--muted);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    margin-top: 6px;
  }

  .name {
    font-size: 11px;
    color: var(--muted);
    min-width: 6ch;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .fader {
    flex: 1;
    min-width: 0;
  }

  .nudge {
    width: 6ch;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    height: var(--control-h);
    padding: 0 4px;
    font: inherit;
    font-size: 11px;
    font-family: var(--mono);
    text-align: right;
  }

  .del {
    color: var(--muted);
    background: none;
    border: none;
    cursor: pointer;
    font-size: 12px;
    padding: 0 2px;
    line-height: 1;
  }

  .del:hover {
    color: var(--fg);
  }
</style>
