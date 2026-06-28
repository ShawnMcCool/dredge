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
  <!-- few controls, laid out vertically so the box reads as a tall settings
       panel rather than a short stub stretched to its neighbour's height. -->
  <Box id="click" grow={false}>
    <div class="settings">
      <div class="setting">
        <div class="row">
          <span class="lbl">count in</span>
          <button
            class="toggle"
            class:on={$countIn.enabled}
            onclick={() => actions.setCountIn({ enabled: !$countIn.enabled })}
            title="count in before playback">{$countIn.enabled ? "on" : "off"}</button
          >
        </div>
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

      <div class="setting">
        <div class="row">
          <span class="lbl">section</span>
          <button
            class="toggle"
            class:on={$sectionClick.enabled}
            onclick={() => actions.setSectionClick(!$sectionClick.enabled)}
            title="click every beat during marked sections">{$sectionClick.enabled ? "on" : "off"}</button
          >
        </div>
      </div>
    </div>
  </Box>
{/if}

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  /* each setting: a label+toggle row, with any sub-controls stacked beneath */
  .setting {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
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

  /* count-in sub-controls, indented under the label; the stepper dims (but stays
     adjustable) while count-in is off */
  .stepper,
  .modeline {
    padding-left: 2px;
  }
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
