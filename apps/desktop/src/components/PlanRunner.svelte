<script lang="ts">
  import {
    actions,
    loopName,
    openSong,
    pendingRatings,
    planStatus,
    quickActive,
    quickPromptVisible,
    quickSavedName,
    sessionSummary,
    type PlanStep,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";

  const MODE_WORD = { listen: "LISTEN", play: "PLAY", recall_silent: "FROM MEMORY" } as const;
  /** The fixed quick-session shape: listen ×2 → 6 oscillating play reps. */
  const QUICK_STEP_REPS = [2, 6];

  function stepReps(step: PlanStep): number {
    switch (step.step) {
      case "listen_first":
        return step.reps;
      case "play_reps":
        return step.reps;
      case "rotation":
        return step.rounds * step.reps_per_visit * step.loop_ids.length;
      case "recall_test":
        return step.alternations * 2;
    }
  }

  let plan = $derived($openSong?.plans.find((p) => p.id === $planStatus?.plan_id) ?? null);
  let totalSteps = $derived($quickActive ? QUICK_STEP_REPS.length : (plan?.steps.length ?? 0));
  let repsInStep = $derived(
    $quickActive && $planStatus
      ? (QUICK_STEP_REPS[$planStatus.step_idx] ?? 0)
      : plan && $planStatus
        ? stepReps(plan.steps[$planStatus.step_idx])
        : 0,
  );
  let prompt = $derived($pendingRatings[0] ?? null);
</script>

<div class="runner">
  {#if $planStatus}
    <div class="mode" class:recall={$planStatus.mode === "recall_silent"}>
      {MODE_WORD[$planStatus.mode]}
    </div>
    {#if $quickActive}
      <div class="loop quick">QUICK</div>
    {:else}
      <div class="loop">{loopName($planStatus.loop_id)}</div>
    {/if}
    <div class="mono detail">
      {Math.round($planStatus.rate * 100)}% · rep {$planStatus.rep_idx + 1}/{repsInStep} · step {$planStatus.step_idx + 1}/{totalSteps}
    </div>
    <div class="controls">
      <Button onclick={() => actions.skipStep()}>skip step</Button>
      <Button onclick={() => actions.stopPlan()}>stop</Button>
    </div>
  {/if}

  {#if prompt}
    <div class="rating fade-in">
      <p>How was {loopName(prompt.loop_id)}?{prompt.is_retest ? " (retest)" : ""}</p>
      <div class="choices">
        <Button onclick={() => actions.resolveRating("miss")}>1 Miss</Button>
        <Button onclick={() => actions.resolveRating("shaky")}>2 Shaky</Button>
        <Button onclick={() => actions.resolveRating("solid")}>3 Solid</Button>
      </div>
    </div>
  {/if}

  {#if $quickPromptVisible}
    <div class="rating fade-in">
      <p>Keep this riff?</p>
      <div class="choices">
        <Button onclick={() => actions.quickRate("miss")}>1 Miss</Button>
        <Button onclick={() => actions.quickRate("shaky")}>2 Shaky</Button>
        <Button onclick={() => actions.quickRate("solid")}>3 Solid</Button>
        <Button onclick={() => actions.quickDiscard()}>Esc discard</Button>
      </div>
    </div>
  {/if}

  {#if $quickSavedName}
    <div class="summary fade-in">
      <p class="mono">saved {$quickSavedName}</p>
    </div>
  {/if}

  {#if !$planStatus && $sessionSummary && !prompt}
    <div class="summary fade-in">
      <h2>session</h2>
      <p class="mono">{$sessionSummary.reps} reps · {$sessionSummary.steps} steps</p>
      <Button onclick={() => sessionSummary.set(null)}>done</Button>
    </div>
  {/if}
</div>

<style>
  .runner {
    display: flex;
    flex-direction: column;
    gap: var(--space);
  }

  .mode {
    font-size: 32px;
    font-weight: 700;
    letter-spacing: 0.04em;
    margin-top: calc(var(--space) * 3);
  }

  .mode.recall {
    color: var(--accent);
  }

  .loop {
    font-size: 16px;
  }

  .loop.quick {
    color: var(--muted);
    letter-spacing: 0.06em;
  }

  .detail {
    font-size: 12px;
    color: var(--muted);
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: calc(var(--space) / 2);
    margin-top: var(--space);
    min-width: 0;
  }

  .rating {
    border-top: 1px solid var(--line);
    padding-top: var(--space);
    margin-top: var(--space);
  }

  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: calc(var(--space) / 2);
    min-width: 0;
  }

  .summary p {
    color: var(--muted);
  }
</style>
