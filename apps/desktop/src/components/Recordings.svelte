<script lang="ts">
  // Recordings box: capture your own input over the track as additive layers.
  // One row per take — name, level, mute, nudge, delete. Recording always
  // covers one pass over the chosen span, after the count-in.
  import { actions, openSong, recordingActive, recordings, selection, currentLoop } from "../lib/stores";
  import { framesToMs, msToFrames } from "../lib/recording-math";
  import { cmd } from "../lib/ipc";
  import Box from "../lib/ui/Box.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";

  type Span = "song" | "selection" | "loop";
  let span = $state<Span>("song");
  let devices = $state<{ id: string; name: string }[]>([]);
  let deviceId = $state<string>("");

  $effect(() => {
    void cmd<{ id: string; name: string }[]>("device.inputs").then((d) => {
      devices = d;
      if (!deviceId && d.length) deviceId = d[0].id;
    });
  });

  async function record() {
    if ($recordingActive) {
      await actions.stopRecording();
      return;
    }
    const sel = $selection;
    const lp = $currentLoop;
    const range =
      span === "selection" && sel ? { start: sel.start, end: sel.end }
      : span === "loop" && lp ? { start: lp.start, end: lp.end }
      : undefined;
    await actions.startRecording(span, deviceId, range);
  }
</script>

{#if $openSong}
  <Box label="recordings" wide>
    <div class="bar">
      <select bind:value={span} disabled={$recordingActive}>
        <option value="song">full song</option>
        <option value="selection" disabled={!$selection}>selection</option>
        <option value="loop" disabled={!$currentLoop}>loop</option>
      </select>
      <select bind:value={deviceId} disabled={$recordingActive}>
        {#each devices as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
      </select>
      <Button variant="toggle" active={$recordingActive} onclick={() => void record()}>
        {$recordingActive ? "stop" : "record"}
      </Button>
    </div>

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
        <button class="del" onclick={() => void actions.deleteRecording(r.id)} title="delete">✕</button>
      </div>
    {/each}
  </Box>
{/if}

<style>
  .bar {
    display: flex;
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
  }

  select:disabled {
    color: var(--muted);
    cursor: default;
  }

  .row {
    display: flex;
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
