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
    inputLevel,
    recordings,
    selection,
    currentLoop,
    type AudioDevice,
    type RecordSpan,
  } from "../lib/stores";
  import { framesToMs, msToFrames } from "../lib/recording-math";
  import { cmd } from "../lib/ipc";
  import { traceErr } from "../lib/trace";
  import Box from "../lib/ui/Box.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Dropdown from "../lib/ui/Dropdown.svelte";
  import Fader from "../lib/ui/Fader.svelte";

  let devices = $state<AudioDevice[]>([]);

  $effect(() => {
    void cmd<AudioDevice[]>("device.inputs")
      .then((d) => {
        devices = d;
      })
      .catch((e) => traceErr("recordings", `device.inputs failed: ${e}`));
  });

  // Run the input-level monitor while armed and not recording, on the selected
  // input. Cleanup stops it — so it also restarts when the input changes and
  // stops on disarm / record / unmount. The recorder owns the device during an
  // actual take (the backend stops the monitor at recording.start).
  $effect(() => {
    if ($recordArmed && !$recordingActive) {
      const device = $recordInput;
      void actions.startInputMonitor(device);
      return () => void actions.stopInputMonitor();
    }
  });

  const spanOptions = $derived([
    { value: "song", label: "full song" },
    { value: "playhead", label: "from playhead" },
    { value: "selection", label: "selection", disabled: !$selection },
    { value: "loop", label: "loop", disabled: !$currentLoop },
  ]);
  const inputOptions = $derived([
    { value: "default", label: "default (follow devices)" },
    ...devices.map((d) => ({ value: String(d.id), label: d.name })),
  ]);
</script>

{#if $openSong}
  <Box id="recordings" wide>
    <div class="arm">
      <!-- A plain <div>, not <label>: the Dropdown is a custom button widget,
           and a wrapping <label> forwards an option click to its first labelable
           descendant (the trigger), re-toggling the menu open in WebKitGTK. The
           .flabel is a caption, not a form label. -->
      <div class="field">
        <span class="flabel">span</span>
        <Dropdown
          value={$recordSpan}
          options={spanOptions}
          disabled={$recordingActive}
          label="recording span"
          onchange={(v) => recordSpan.set(v as RecordSpan)}
        />
      </div>
      <div class="field">
        <span class="flabel">input</span>
        <Dropdown
          value={$recordInput}
          options={inputOptions}
          disabled={$recordingActive}
          label="input device"
          onchange={(v) => recordInput.set(v)}
        />
      </div>
      <div class="arm-row">
        <Button
          variant="toggle"
          active={$recordArmed}
          disabled={$recordingActive}
          onclick={() => recordArmed.set(!$recordArmed)}
        >
          {$recordArmed ? "armed ✓" : "arm"}
        </Button>
      </div>
    </div>
    {#if $recordArmed && !$recordingActive}
      <div class="meter" title="input level" aria-label="input level">
        <div class="meter-bar" style="width: {Math.min(100, ($inputLevel?.peak ?? 0) * 100)}%"></div>
      </div>
      <p class="hint">
        {($inputLevel?.peak ?? 0) < 0.001
          ? "no input signal — check the input device"
          : "record from the transport"}
      </p>
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
  /* arm controls stacked vertically — each a small label over a popover select,
     so the box isn't dominated by full-width native select bars */
  .arm {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 240px;
    max-width: 100%;
  }
  .flabel {
    font-size: 11px;
    color: var(--muted);
  }
  .arm-row {
    margin-top: 2px;
  }

  .meter {
    width: 240px;
    max-width: 100%;
    height: 8px;
    margin-top: 6px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 3px;
    overflow: hidden;
  }
  .meter-bar {
    height: 100%;
    background: var(--accent);
    transition: width 60ms linear;
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
