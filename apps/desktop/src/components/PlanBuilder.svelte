<script lang="ts">
  import {
    actions,
    openSong,
    type LoopRegion,
    type PlanStep,
    type TempoCurve,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";

  type StepType = PlanStep["step"];
  type CurveType = TempoCurve["curve"];

  let name = $state("practice");
  let steps = $state<PlanStep[]>([]);
  let addType = $state<StepType>("listen_first");
  let picked = $state<number[]>([]); // loop ids for "suggested plan" / rotation
  let error = $state("");
  let lastSongId: number | null = null;

  $effect(() => {
    const open = $openSong;
    if (open?.song.id !== lastSongId) {
      lastSongId = open?.song.id ?? null;
      steps = [];
      picked = [];
      error = "";
    }
  });

  function loops(): LoopRegion[] {
    return $openSong?.loops ?? [];
  }

  function firstLoopId(): number {
    return loops()[0]?.id ?? 0;
  }

  function defaultCurve(): TempoCurve {
    return { curve: "dwell", rate: 0.85 };
  }

  function addStep() {
    const loop_id = firstLoopId();
    const byType: Record<StepType, PlanStep> = {
      listen_first: { step: "listen_first", loop_id, reps: 2 },
      play_reps: { step: "play_reps", loop_id, reps: 4, curve: defaultCurve() },
      rotation: {
        step: "rotation",
        loop_ids: picked.length >= 2 ? [...picked] : loops().map((l) => l.id),
        rounds: 2,
        reps_per_visit: 2,
        curve: defaultCurve(),
      },
      recall_test: { step: "recall_test", loop_id, alternations: 2, rate: 1.0 },
    };
    steps.push(byType[addType]);
  }

  function removeStep(i: number) {
    steps.splice(i, 1);
  }

  function moveStep(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= steps.length) return;
    [steps[i], steps[j]] = [steps[j], steps[i]];
  }

  function setCurveType(step: PlanStep & { curve: TempoCurve }, t: CurveType) {
    if (t === "dwell") step.curve = { curve: "dwell", rate: 0.85 };
    else if (t === "ladder") step.curve = { curve: "ladder", start: 0.7, step: 0.05, target: 1.0 };
    else step.curve = { curve: "oscillate", low: 0.7, high: 1.0, period: 3 };
  }

  function togglePick(id: number) {
    picked = picked.includes(id) ? picked.filter((p) => p !== id) : [...picked, id];
  }

  /** The evidence-based default: listen → oscillating play per loop,
   *  then interleaved rotation, then a recall test. */
  function suggested() {
    const ids = picked.length > 0 ? picked : loops().map((l) => l.id);
    if (ids.length === 0) return;
    const out: PlanStep[] = [];
    for (const id of ids) {
      out.push({ step: "listen_first", loop_id: id, reps: 2 });
      out.push({
        step: "play_reps",
        loop_id: id,
        reps: 4,
        curve: { curve: "oscillate", low: 0.7, high: 1.0, period: 3 },
      });
    }
    if (ids.length >= 2) {
      out.push({
        step: "rotation",
        loop_ids: [...ids],
        rounds: 2,
        reps_per_visit: 2,
        curve: { curve: "dwell", rate: 0.85 },
      });
    }
    out.push({ step: "recall_test", loop_id: ids[0], alternations: 2, rate: 1.0 });
    steps = out;
    if (name === "practice") name = "suggested";
  }

  async function save() {
    error = "";
    try {
      await actions.savePlan(name, $state.snapshot(steps) as PlanStep[]);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function start(planId: number) {
    error = "";
    try {
      await actions.startPlan(planId);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

{#snippet curveEditor(step: PlanStep & { curve: TempoCurve })}
  <select
    value={step.curve.curve}
    onchange={(e) => setCurveType(step, e.currentTarget.value as CurveType)}
  >
    <option value="dwell">dwell</option>
    <option value="ladder">ladder</option>
    <option value="oscillate">oscillate</option>
  </select>
  {#if step.curve.curve === "dwell"}
    <label>rate <input type="number" step="0.05" min="0.25" max="2" bind:value={step.curve.rate} /></label>
  {:else if step.curve.curve === "ladder"}
    <label>start <input type="number" step="0.05" bind:value={step.curve.start} /></label>
    <label>step <input type="number" step="0.01" bind:value={step.curve.step} /></label>
    <label>target <input type="number" step="0.05" bind:value={step.curve.target} /></label>
  {:else}
    <label>low <input type="number" step="0.05" bind:value={step.curve.low} /></label>
    <label>high <input type="number" step="0.05" bind:value={step.curve.high} /></label>
    <label>period <input type="number" step="1" min="2" bind:value={step.curve.period} /></label>
  {/if}
{/snippet}

{#snippet loopSelect(step: PlanStep & { loop_id: number })}
  <select bind:value={step.loop_id}>
    {#each loops() as l (l.id)}
      <option value={l.id}>{l.name}</option>
    {/each}
  </select>
{/snippet}

<h2>plan builder</h2>
{#if !$openSong}
  <p class="empty">open a song first</p>
{:else if loops().length === 0}
  <p class="empty">create loops first</p>
{:else}
  <div class="pick-lane">
    {#each loops() as l (l.id)}
      <label class="pick">
        <input type="checkbox" checked={picked.includes(l.id)} onchange={() => togglePick(l.id)} />
        {l.name}
      </label>
    {/each}
  </div>
  <Button style="width: 100%; margin-bottom: var(--space)" onclick={suggested}>
    suggested plan
  </Button>

  <ul class="steps">
    {#each steps as step, i (i)}
      <li class="step">
        <div class="head">
          <span class="kind">{step.step.replace("_", " ")}</span>
          <span class="ops">
            <Button variant="chip" onclick={() => moveStep(i, -1)}>↑</Button>
            <Button variant="chip" onclick={() => moveStep(i, 1)}>↓</Button>
            <Button variant="chip" onclick={() => removeStep(i)}>×</Button>
          </span>
        </div>
        <div class="fields">
          {#if step.step === "listen_first"}
            {@render loopSelect(step)}
            <label>reps <input type="number" min="1" bind:value={step.reps} /></label>
          {:else if step.step === "play_reps"}
            {@render loopSelect(step)}
            <label>reps <input type="number" min="1" bind:value={step.reps} /></label>
            {@render curveEditor(step)}
          {:else if step.step === "rotation"}
            <select multiple bind:value={step.loop_ids} size={Math.min(loops().length, 4)}>
              {#each loops() as l (l.id)}
                <option value={l.id}>{l.name}</option>
              {/each}
            </select>
            <label>rounds <input type="number" min="1" bind:value={step.rounds} /></label>
            <label>reps/visit <input type="number" min="1" bind:value={step.reps_per_visit} /></label>
            {@render curveEditor(step)}
          {:else}
            {@render loopSelect(step)}
            <label>alternations <input type="number" min="1" bind:value={step.alternations} /></label>
            <label>rate <input type="number" step="0.05" min="0.25" max="2" bind:value={step.rate} /></label>
          {/if}
        </div>
      </li>
    {/each}
  </ul>

  <div class="bar">
    <select bind:value={addType}>
      <option value="listen_first">Listen first</option>
      <option value="play_reps">Play reps</option>
      <option value="rotation">Rotation</option>
      <option value="recall_test">Recall test</option>
    </select>
    <Button onclick={addStep}>+ step</Button>
  </div>

  <div class="bar">
    <input class="plan-name" bind:value={name} />
    <Button accent disabled={steps.length === 0} onclick={save}>save</Button>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  {#if $openSong.plans.length > 0}
    <h2 class="existing">plans</h2>
    <ul>
      {#each $openSong.plans as plan (plan.id)}
        <li class="plan-row">
          <span>{plan.name} <span class="muted mono">{plan.steps.length} steps</span></span>
          <Button onclick={() => start(plan.id)}>▶ start</Button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}

<style>
  .empty {
    font-size: 11px;
    color: var(--muted);
  }

  .pick-lane {
    display: flex;
    flex-wrap: wrap;
    gap: calc(var(--space) / 2);
    margin-bottom: var(--space);
  }

  .pick {
    font-size: 11px;
    color: var(--muted);
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .steps {
    margin-bottom: var(--space);
  }

  .step {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: calc(var(--space) / 2);
    margin-bottom: calc(var(--space) / 2);
  }

  .head {
    display: flex;
    justify-content: space-between;
    margin-bottom: calc(var(--space) / 2);
  }

  .kind {
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--accent);
  }

  .ops {
    display: inline-flex;
    gap: 2px;
  }

  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: calc(var(--space) / 2);
    font-size: 11px;
    color: var(--muted);
  }

  .fields label {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .fields input[type="number"] {
    width: 3.8em;
    font-size: 11px;
    padding: 1px 3px;
  }

  .fields select {
    font-size: 11px;
    padding: 1px 3px;
    max-width: 110px;
    min-width: 0;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: calc(var(--space) / 2);
    margin-bottom: var(--space);
    min-width: 0;
  }

  .bar select {
    min-width: 0;
  }

  .plan-name {
    flex: 1;
    min-width: 0;
  }

  .existing {
    margin-top: var(--space);
  }

  .plan-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: calc(var(--space) / 2);
  }

  .muted {
    color: var(--muted);
    font-size: 11px;
  }

  .error {
    font-size: 11px;
    color: var(--miss);
  }
</style>
