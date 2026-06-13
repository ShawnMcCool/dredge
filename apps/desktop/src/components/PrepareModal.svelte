<script lang="ts">
  // Progress modal for the one-button prepare flow: two step rows over a
  // coarse overall bar. The subprocesses emit no percentages, so the bar is
  // honest-coarse (0 → 50 → 100) and each row is a state glyph.
  import { actions, prepareState, profiles, type PrepareStepState } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Modal from "../lib/ui/Modal.svelte";

  const STEPS = [
    { key: "analysis", label: "analysis" },
    { key: "stems", label: "stems" },
  ] as const;

  const GLYPHS: Record<PrepareStepState, string> = {
    pending: "·",
    running: "◌",
    done: "✓",
    cached: "✓",
    failed: "✗",
  };

  const terminal = (s: PrepareStepState) => s !== "pending" && s !== "running";

  let state = $derived($prepareState);
  let progress = $derived(
    state ? Object.values(state.steps).filter(terminal).length * 50 : 0,
  );
  let failed = $derived(state !== null && Object.values(state.steps).includes("failed"));

  function lastRun(step: string): string | null {
    const s = $prepareState;
    if (!s) return null;
    const op = step === "analysis" ? "analysis" : "stems";
    const run = $profiles.find((p) => p.op === op && p.song_id === s.song_id);
    if (!run) return null;
    const ms = run.total_ms;
    const t = ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
    return [t, run.device, run.engine].filter(Boolean).join(" · ");
  }
</script>

{#if state}
  <Modal open title="prepare" closable={failed} onclose={() => actions.closePrepare()}>
    <ul>
      {#each STEPS as step (step.key)}
        {@const s = state.steps[step.key]}
        <li class="step">
          <span
            class="glyph mono"
            class:running={s === "running"}
            class:done={s === "done" || s === "cached"}
            class:failed={s === "failed"}
          >
            {GLYPHS[s]}
          </span>
          <span class="name">{step.label}</span>
          {#if s === "cached"}<span class="note mono">cached</span>{/if}
          {#if terminal(s)}
            {@const summary = lastRun(step.key)}
            {#if summary}<span class="note mono">{summary}</span>{/if}
          {/if}
          {#if state.errors[step.key]}
            <span class="error">{state.errors[step.key]}</span>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="bar">
      <div class="fill" style="width: {progress}%"></div>
    </div>
    {#if failed}
      <div class="actions">
        <Button onclick={() => actions.closePrepare()}>close</Button>
      </div>
    {/if}
  </Modal>
{/if}

<style>
  .step {
    display: flex;
    align-items: baseline;
    gap: var(--space);
    margin-bottom: var(--space);
    min-width: 0;
  }

  .glyph {
    flex: 0 0 auto;
    width: 1.2em;
    text-align: center;
    color: var(--muted);
  }

  .glyph.running {
    color: var(--accent);
    animation: spin 1s linear infinite;
  }

  .glyph.done {
    color: var(--solid);
  }

  .glyph.failed {
    color: var(--miss);
  }

  /* the dotted-circle glyph reads as a spinner once it turns */
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .name {
    flex: 0 0 auto;
  }

  .note {
    font-size: 11px;
    color: var(--muted);
  }

  .error {
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
    min-width: 0;
  }

  .bar {
    height: 2px;
    background: var(--line);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    transition: width var(--fade) ease-out;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: calc(var(--space) * 2);
  }
</style>
