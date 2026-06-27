<script lang="ts">
  import Box from "../lib/ui/Box.svelte";
  import Group from "../lib/ui/Group.svelte";
  import { actions, metronome, metronomeBeat, openSong, type Cadence, type Kit } from "../lib/stores";
  import { clampBpm, strongMask } from "../lib/metronome";

  const KITS: { id: Kit; label: string }[] = [
    { id: "click", label: "click" },
    { id: "kick_snare", label: "kick/snare" },
    { id: "cowbell", label: "cowbell" },
  ];
  const SIGS = [2, 3, 4, 5, 6, 7];
  const CADENCES: { id: Cadence; label: string }[] = [
    { id: "beat", label: "beat" },
    { id: "half", label: "½ bar" },
    { id: "bar", label: "bar" },
  ];

  let canSync = $derived($openSong?.analysis?.bpm != null);
  let beat = $derived($metronomeBeat);
  let dots = $derived(Array.from({ length: $metronome.beatsPerBar }, (_, i) => i + 1));
  let mask = $derived(strongMask($metronome.beatsPerBar));
  const isStrong = (d: number) => (mask & (1 << (d - 1))) !== 0;

  function setBpm(raw: number) {
    if (!Number.isFinite(raw)) return;
    void actions.setMetronome({ bpm: clampBpm(raw) }); // clamp so the field self-corrects
  }
</script>

<Box id="metronome">
  <div class="rows">
    <div class="bar" aria-hidden="true">
      {#each dots as d (d)}
        <span
          class="dot"
          class:accent={d === 1}
          class:strong={isStrong(d) && d !== 1}
          class:lit={beat?.beat === d && $metronome.running}
        ></span>
      {/each}
    </div>

    <Group label="tempo">
      <button
        class="toggle primary"
        class:on={$metronome.running}
        onclick={() => actions.toggleMetronome()}
        title={$metronome.running ? "stop the metronome" : "start the metronome"}
      >{$metronome.running ? "stop" : "start"}</button>
      <span class="bpm">
        <input
          type="number"
          min="30"
          max="300"
          value={$metronome.bpm}
          onchange={(e) => setBpm(e.currentTarget.valueAsNumber)}
          aria-label="tempo in bpm"
        />
        <span class="unit">bpm</span>
      </span>
      <button class="toggle" onclick={() => actions.tapTempo(performance.now())} title="tap to set the tempo">tap</button>
      {#if canSync}
        <button class="toggle" onclick={() => actions.syncMetronomeToSong()} title="use the song's analyzed tempo">sync</button>
      {/if}
    </Group>

    <Group label="feel">
      <select
        class="sig"
        value={$metronome.beatsPerBar}
        onchange={(e) => actions.setMetronome({ beatsPerBar: Number(e.currentTarget.value) })}
        aria-label="time signature"
      >
        {#each SIGS as n (n)}<option value={n}>{n}/4</option>{/each}
      </select>
      {#each CADENCES as c (c.id)}
        <button
          class="toggle"
          class:on={$metronome.cadence === c.id}
          onclick={() => actions.setMetronome({ cadence: c.id })}
          title={`accent the ${c.label}`}
        >{c.label}</button>
      {/each}
    </Group>

    <Group label="sound">
      {#each KITS as k (k.id)}
        <button
          class="toggle"
          class:on={$metronome.kit === k.id}
          onclick={() => actions.setMetronome({ kit: k.id })}
        >{k.label}</button>
      {/each}
    </Group>
  </div>
</Box>

<style>
  .rows {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* beat indicator: one dot per beat in the bar, beat 1 larger, the live beat lit */
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 14px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--line);
    transition: background 60ms;
  }
  .dot.strong {
    width: 11px;
    height: 11px;
    background: var(--accent-dim);
  }
  .dot.accent {
    width: 12px;
    height: 12px;
    background: var(--accent-dim); /* primary downbeat: largest + accented at rest */
  }
  .dot.lit {
    background: var(--accent);
  }
  .dot.accent.lit {
    box-shadow: 0 0 6px var(--accent-dim);
  }

  /* pill controls — same shape as the click-track toggles: muted by default,
     accent border + text when on */
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
  /* start/stop reads as the box's primary verb */
  .toggle.primary.on {
    background: var(--accent);
    color: var(--bg);
  }

  .bpm {
    display: inline-flex;
    align-items: baseline;
    gap: 3px;
  }
  .bpm input {
    width: 4em;
    font-size: 12px;
    padding: 1px 3px;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .unit {
    color: var(--muted);
    font-size: 11px;
  }

  .sig {
    font-size: 11px;
    padding: 1px 3px;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--line);
    border-radius: 4px;
    cursor: pointer;
  }
</style>
