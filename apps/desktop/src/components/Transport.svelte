<script lang="ts">
  import { actions, bassFocusOn, muted, openSong, pitch, position } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";
  import Group from "../lib/ui/Group.svelte";
  import Toolbar from "../lib/ui/Toolbar.svelte";

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

  function onCentsInput(e: Event) {
    const cents = Number((e.currentTarget as HTMLInputElement).value);
    void actions.setPitch($pitch.semitones, cents);
  }
</script>

<div class="transport">
  <Toolbar>
    <Group>
      <Button
        variant="icon"
        style="width: 40px"
        onclick={() => ($position.playing ? actions.pause() : actions.play())}
      >
        {$position.playing ? "⏸" : "▶"}
      </Button>
      <span class="readout time">
        {fmt($position.secs)} / {fmtTotal($openSong?.song.duration_secs ?? 0)}
      </span>
    </Group>

    <Group grow>
      <span class="readout rate">{Math.round($position.rate * 100)}%</span>
      <Fader
        value={$position.rate}
        min={0.25}
        max={2}
        step={0.05}
        accent
        onchange={(v) => void actions.setRate(v)}
        format={(v) => `rate ${Math.round(v * 100)}%`}
      />
    </Group>

    <Group>
      {#each RATE_PRESETS as r (r)}
        <Button
          variant="chip"
          active={Math.abs($position.rate - r) < 0.001}
          onclick={() => actions.setRate(r)}
        >
          {Math.round(r * 100)}
        </Button>
      {/each}
    </Group>

    <Group label="pitch">
      {#each SEMITONE_CHIPS as st (st)}
        <Button
          variant="chip"
          active={$pitch.semitones === st}
          onclick={() => actions.setPitch(st, $pitch.cents)}
        >
          {st > 0 ? `+${st}` : st}
        </Button>
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
    </Group>

    <Group>
      <Button variant="toggle" active={$bassFocusOn} onclick={() => actions.bassFocus(!$bassFocusOn)}>
        BASS FOCUS
      </Button>
      <Button variant="toggle" active={$muted} onclick={() => actions.mute(!$muted)}>MUTE</Button>
    </Group>
  </Toolbar>
</div>

<style>
  .transport {
    padding: var(--space) 0;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  .time {
    color: var(--fg);
    min-width: 13ch;
  }

  .rate {
    color: var(--accent);
    min-width: 4ch;
    text-align: right;
  }

  .cents {
    width: 4.5em;
    font-size: 11px;
    padding: 1px 4px;
  }
</style>
