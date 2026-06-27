<script lang="ts">
  import { actions, countIn, sectionClick, sectionClickAvailable } from "../lib/stores";
  import { stepCountInBeats } from "../lib/count-in";
  import Box from "../lib/ui/Box.svelte";
  import Group from "../lib/ui/Group.svelte";

  function stepCount(d: number) {
    void actions.setCountIn({ beats: stepCountInBeats($countIn.beats, d) });
  }
  let countLabel = $derived(`${$countIn.beats} beat${$countIn.beats === 1 ? "" : "s"}`);
</script>

{#if $sectionClickAvailable}
  <Box id="click" label="click track">
    <div class="rows">
      <Group label="count in">
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
        <span class="modeline">
          <button
            class="modeword"
            class:on={$countIn.enabled}
            onclick={() =>
              actions.setCountIn({ loopMode: $countIn.loopMode === "first" ? "every" : "first" })}
            title="count in on the first pass, or before every loop"
          >{$countIn.loopMode}</button>
          loop
        </span>
      </Group>

      <Group label="section click">
        <button
          class="toggle"
          class:on={$sectionClick.enabled}
          onclick={() => actions.setSectionClick(!$sectionClick.enabled)}
          title="click every beat during marked sections"
        >{$sectionClick.enabled ? "on" : "off"}</button>
      </Group>
    </div>
  </Box>
{/if}

<style>
  .rows {
    display: flex;
    flex-direction: column;
    gap: 12px;
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

  /* bare − value + stepper */
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
  /* the beats stepper dims while count-in is off, but stays adjustable */
  .stepper.off {
    opacity: 0.4;
  }
  .pval {
    min-width: 5ch;
    text-align: center;
    font-size: 13px;
    color: var(--fg);
  }

  /* count-in loop mode: inline label word ("count in EVERY loop"). Reads as a
     label, accent-colored when on, muted grey when off. */
  .modeline {
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
  }
  .modeword {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--muted);
    cursor: pointer;
  }
  .modeword.on {
    color: var(--accent);
  }
  .modeword:hover {
    color: var(--fg);
  }
</style>
