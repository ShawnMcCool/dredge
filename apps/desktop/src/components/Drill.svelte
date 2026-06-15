<script lang="ts">
  // The drill box — a live practice workbench for the active loop. It edits an
  // ephemeral scratch span (drillSpan), never the saved loop. Shown only while a
  // loop is active (App gates on $currentLoop). Phase 2: shell + span readout;
  // the trainer / region toys / recall land in later phases.
  import { actions, currentLoop, drillSpan } from "../lib/stores";
  import { fmtClock } from "../lib/format";
  import Box from "../lib/ui/Box.svelte";

  let saved = $derived($currentLoop);
  let span = $derived($drillSpan);
  let diverged = $derived(
    !!(saved && span && (span.start !== saved.start || span.end !== saved.end)),
  );
  let length = $derived(span ? span.end - span.start : 0);
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
</style>
