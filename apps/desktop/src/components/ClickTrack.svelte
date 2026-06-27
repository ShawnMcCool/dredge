<script lang="ts">
  import { actions, countIn, sectionClick, sectionClickAvailable } from "../lib/stores";
  import { stepCountInBeats } from "../lib/count-in";
  import Box from "../lib/ui/Box.svelte";

  function stepCount(d: number) {
    void actions.setCountIn({ beats: stepCountInBeats($countIn.beats, d) });
  }
  let countLabel = $derived(`${$countIn.beats} beat${$countIn.beats === 1 ? "" : "s"}`);
</script>

{#if $sectionClickAvailable}
  <!-- few controls — lock to content width and lay the two settings out as one
       aligned label→controls grid so the toggles line up. -->
  <Box id="click" grow={false}>
    <div class="settings">
      <span class="lbl">count in</span>
      <div class="ctl">
        <button
          class="toggle"
          class:on={$countIn.enabled}
          onclick={() => actions.setCountIn({ enabled: !$countIn.enabled })}
          title="count in before playback">{$countIn.enabled ? "on" : "off"}</button
        >
        <span class="stepper" class:off={!$countIn.enabled} title="beats before playback">
          <button onclick={() => stepCount(-1)} aria-label="fewer count-in beats">−</button>
          <span class="pval mono">{countLabel}</span>
          <button onclick={() => stepCount(1)} aria-label="more count-in beats">+</button>
        </span>
        <span class="modeline">
          <button
            class="modeword"
            class:on={$countIn.enabled}
            onclick={() => actions.setCountIn({ loopMode: $countIn.loopMode === "first" ? "every" : "first" })}
            title="count in on the first pass, or before every loop">{$countIn.loopMode}</button
          > loop
        </span>
      </div>

      <span class="lbl">section</span>
      <div class="ctl">
        <button
          class="toggle"
          class:on={$sectionClick.enabled}
          onclick={() => actions.setSectionClick(!$sectionClick.enabled)}
          title="click every beat during marked sections">{$sectionClick.enabled ? "on" : "off"}</button
        >
      </div>
    </div>
  </Box>
{/if}

<style>
  /* label → controls, two rows that share a column so the toggles align */
  .settings {
    display: grid;
    grid-template-columns: auto auto;
    align-items: center;
    column-gap: 14px;
    row-gap: 10px;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
  }
  .ctl {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  /* on/off pill — outline when off, accent when on (keeps the value visible
     while disabled). Both toggles share it so they read identically. */
  .toggle {
    background: none;
    border: 1px solid var(--line);
    color: var(--muted);
    border-radius: 4px;
    padding: 1px 7px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
  }
  .toggle:hover {
    color: var(--fg);
    border-color: var(--accent-dim);
  }
  .toggle.on {
    border-color: var(--accent);
    color: var(--accent);
  }

  /* − value + stepper; dims (but stays adjustable) while count-in is off */
  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 2px;
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
  .stepper.off {
    opacity: 0.4;
  }
  .pval {
    min-width: 5ch;
    text-align: center;
    font-size: 12px;
    color: var(--fg);
  }

  /* count-in loop mode, reads as a label word ("every loop"), accent when on */
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
