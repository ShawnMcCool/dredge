<script lang="ts">
  import {
    actions,
    loopName,
    openSong,
    pendingRatings,
    planStatus,
    sessionSummary,
    type PlanStep,
  } from "../lib/stores";

  const MODE_WORD = { listen: "LISTEN", play: "PLAY", recall_silent: "FROM MEMORY" } as const;

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
  let totalSteps = $derived(plan?.steps.length ?? 0);
  let repsInStep = $derived(
    plan && $planStatus ? stepReps(plan.steps[$planStatus.step_idx]) : 0,
  );
  let prompt = $derived($pendingRatings[0] ?? null);
</script>

<div class="runner">
  {#if $planStatus}
    <div class="mode" class:recall={$planStatus.mode === "recall_silent"}>
      {MODE_WORD[$planStatus.mode]}
    </div>
    <div class="loop">{loopName($planStatus.loop_id)}</div>
    <div class="mono detail">
      {Math.round($planStatus.rate * 100)}% · rep {$planStatus.rep_idx + 1}/{repsInStep} · step {$planStatus.step_idx + 1}/{totalSteps}
    </div>
    <div class="controls">
      <button onclick={() => actions.skipStep()}>skip step</button>
      <button onclick={() => actions.stopPlan()}>stop</button>
    </div>
  {/if}

  {#if prompt}
    <div class="rating fade-in">
      <p>How was {loopName(prompt.loop_id)}?{prompt.is_retest ? " (retest)" : ""}</p>
      <div class="choices">
        <button onclick={() => actions.resolveRating("miss")}>1 Miss</button>
        <button onclick={() => actions.resolveRating("shaky")}>2 Shaky</button>
        <button onclick={() => actions.resolveRating("solid")}>3 Solid</button>
      </div>
    </div>
  {/if}

  {#if !$planStatus && $sessionSummary && !prompt}
    <div class="summary fade-in">
      <h2>session</h2>
      <p class="mono">{$sessionSummary.reps} reps · {$sessionSummary.steps} steps</p>
      <button onclick={() => sessionSummary.set(null)}>done</button>
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

  .detail {
    font-size: 12px;
    color: var(--muted);
  }

  .controls {
    display: flex;
    gap: calc(var(--space) / 2);
    margin-top: var(--space);
  }

  .rating {
    border-top: 1px solid var(--line);
    padding-top: var(--space);
    margin-top: var(--space);
  }

  .choices {
    display: flex;
    gap: calc(var(--space) / 2);
  }

  .summary p {
    color: var(--muted);
  }

  .summary button {
    align-self: flex-start;
  }
</style>
