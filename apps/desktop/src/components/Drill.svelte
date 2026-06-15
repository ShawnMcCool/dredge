<script lang="ts">
  // The drill box — a live practice workbench for the active loop. It edits an
  // ephemeral scratch span (drillSpan), never the saved loop. Shown only while a
  // loop is active (App gates on $currentLoop).
  //
  // Spine: the step-up tempo trainer — a ramp recipe that autopilots the global
  // playback rate across loop cycles (no second tempo). Region toys + recall
  // land in later phases.
  import { get } from "svelte/store";
  import {
    actions,
    currentLoop,
    drillRecall,
    drillSpan,
    drillTrainer,
    openSong,
    position,
  } from "../lib/stores";
  import type { TempoCurve } from "../lib/stores";
  import { fmtClock } from "../lib/format";
  import Box from "../lib/ui/Box.svelte";
  import Button from "../lib/ui/Button.svelte";
  import NumberField from "../lib/ui/NumberField.svelte";

  let saved = $derived($currentLoop);
  let span = $derived($drillSpan);
  let diverged = $derived(
    !!(saved && span && (span.start !== saved.start || span.end !== saved.end)),
  );
  let length = $derived(span ? span.end - span.start : 0);

  // Recipe editor — local primitives per curve so each field binds cleanly;
  // the derived recipe is pushed to the store (which re-applies the rate when
  // armed). Seed from the trainer's current recipe once.
  const init = get(drillTrainer).recipe;
  let kind = $state<TempoCurve["curve"]>(init.curve);
  let dwellRate = $state(init.curve === "dwell" ? init.rate : 0.9);
  let ladderStart = $state(init.curve === "ladder" ? init.start : 0.7);
  let ladderStep = $state(init.curve === "ladder" ? init.step : 0.05);
  let ladderTarget = $state(init.curve === "ladder" ? init.target : 1.0);
  let oscLow = $state(init.curve === "oscillate" ? init.low : 0.7);
  let oscHigh = $state(init.curve === "oscillate" ? init.high : 1.0);
  let oscPeriod = $state(init.curve === "oscillate" ? init.period : 3);

  let recipe = $derived<TempoCurve>(
    kind === "dwell"
      ? { curve: "dwell", rate: dwellRate }
      : kind === "ladder"
        ? { curve: "ladder", start: ladderStart, step: ladderStep, target: ladderTarget }
        : { curve: "oscillate", low: oscLow, high: oscHigh, period: oscPeriod },
  );
  $effect(() => {
    void actions.setTrainerRecipe(recipe);
  });

  let armed = $derived($drillTrainer.armed);
  let cycle = $derived($drillTrainer.cycle);
  let ratePct = $derived(Math.round($position.rate * 100));
  let hasGrid = $derived(!!$openSong?.analysis?.downbeats?.length);

  let everyN = $derived($drillRecall.everyN);
  let armNext = $derived($drillRecall.armNext);
  // cycle the every-Nth recall: off → 2 → 3 → 4 → off
  function cycleEveryN() {
    const cur = everyN;
    void actions.setRecallEveryN(cur === null ? 2 : cur >= 4 ? null : cur + 1);
  }
</script>

<Box label="drill" wide>
  {#snippet tools()}
    <button
      onclick={() => actions.drillResetSpan()}
      disabled={!diverged}
      title="reset the scratch span to the saved loop"
      aria-label="reset span"
    >⟲</button>
  {/snippet}

  <div class="head-row">
    <span class="loop-name">{saved?.name ?? "loop"}</span>
    {#if span}
      <span class="span" class:diverged>
        {fmtClock(span.start)} – {fmtClock(span.end)}
        <span class="len">({fmtClock(length)})</span>
      </span>
    {/if}
  </div>

  <section class="toys">
    <div class="row">
      <span class="cap">region</span>
      <span class="grp" title="move the loop start">
        start
        <Button variant="chip" onclick={() => actions.drillNudge("start", -1)} aria-label="start earlier">◂</Button>
        <Button variant="chip" onclick={() => actions.drillNudge("start", 1)} aria-label="start later">▸</Button>
      </span>
      <span class="grp" title="move the loop end">
        end
        <Button variant="chip" onclick={() => actions.drillNudge("end", -1)} aria-label="end earlier">◂</Button>
        <Button variant="chip" onclick={() => actions.drillNudge("end", 1)} aria-label="end later">▸</Button>
      </span>
      <span class="grp" title="shrink to one half">
        isolate
        <Button variant="chip" onclick={() => actions.drillIsolate("first")}>1st</Button>
        <Button variant="chip" onclick={() => actions.drillIsolate("second")}>2nd</Button>
      </span>
      <span class="grp" title="extend / retract the start to rehearse the entrance">
        run-up
        <Button variant="chip" onclick={() => actions.drillRunUp(1)}>+bar</Button>
        <Button variant="chip" onclick={() => actions.drillRunUp(-1)}>−bar</Button>
      </span>
      {#if !hasGrid}<span class="hint">no grid — stepping by 0.25 s / ~2 s bars</span>{/if}
    </div>
  </section>

  <section class="trainer">
    <div class="row">
      <span class="cap">tempo trainer</span>
      <div class="picker">
        <Button variant="chip" active={kind === "dwell"} onclick={() => (kind = "dwell")}>dwell</Button>
        <Button variant="chip" active={kind === "ladder"} onclick={() => (kind = "ladder")}>ladder</Button>
        <Button variant="chip" active={kind === "oscillate"} onclick={() => (kind = "oscillate")}>oscillate</Button>
      </div>
    </div>

    <div class="row params">
      {#if kind === "dwell"}
        <NumberField label="rate" bind:value={dwellRate} step={0.05} min={0.25} max={2} />
      {:else if kind === "ladder"}
        <NumberField label="start" bind:value={ladderStart} step={0.05} min={0.25} max={2} />
        <NumberField label="step" bind:value={ladderStep} step={0.01} min={0} max={1} />
        <NumberField label="target" bind:value={ladderTarget} step={0.05} min={0.25} max={2} />
      {:else}
        <NumberField label="low" bind:value={oscLow} step={0.05} min={0.25} max={2} />
        <NumberField label="high" bind:value={oscHigh} step={0.05} min={0.25} max={2} />
        <NumberField label="every" bind:value={oscPeriod} step={1} min={1} max={16} />
      {/if}
    </div>

    <div class="row controls">
      <Button accent={armed} onclick={() => (armed ? actions.disarmTrainer() : actions.armTrainer())}>
        {armed ? "disarm" : "arm"}
      </Button>
      <span class="readout">
        <span class="rate">{ratePct}%</span>
        {#if armed}<span class="cyc">cycle {cycle}</span>{/if}
      </span>
      <Button variant="chip" onclick={() => actions.resetRate()} title="return the global rate to 100%">reset rate</Button>
    </div>
  </section>

  <section class="recall">
    <div class="row">
      <span class="cap">recall</span>
      <Button
        active={armNext}
        onclick={() => actions.armRecallNext()}
        title="mute the recording for the next pass — play it from memory"
      >next pass silent</Button>
      <Button
        variant="chip"
        active={everyN !== null}
        onclick={cycleEveryN}
        title="silence every Nth pass"
      >every {everyN ?? "off"}</Button>
      {#if armNext}<span class="flag">next pass: from memory</span>{/if}
    </div>
  </section>
</Box>

<style>
  .head-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space);
  }
  .loop-name {
    font-size: 13px;
    color: var(--fg);
  }
  .span {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--muted);
  }
  .span.diverged {
    color: var(--accent);
  }
  .len {
    opacity: 0.8;
  }

  .toys,
  .trainer,
  .recall {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .flag {
    font-size: 11px;
    color: var(--accent);
    font-family: var(--mono);
  }
  .grp {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--muted);
  }
  .hint {
    font-size: 10px;
    color: var(--muted);
    opacity: 0.7;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space);
    flex-wrap: wrap;
  }
  .cap {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .picker {
    display: flex;
    gap: 4px;
  }
  .params {
    font-size: 11px;
    color: var(--muted);
  }
  .controls {
    justify-content: flex-start;
  }
  .readout {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-family: var(--mono);
  }
  .rate {
    font-size: 15px;
    color: var(--fg);
  }
  .cyc {
    font-size: 11px;
    color: var(--accent);
  }
</style>
