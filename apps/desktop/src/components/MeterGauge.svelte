<script lang="ts">
  interface Props {
    listening: boolean; // on but no steady pitch
    note: string;
    octave: number;
    cents: number;
    inTune: boolean;
    locked: boolean;
  }
  let { listening, note, octave, cents, inTune, locked }: Props = $props();

  // marker offset: -50..+50 cents -> 0..100%
  const pct = $derived(Math.max(0, Math.min(100, 50 + cents)));
  const word = $derived(cents > 0 ? "sharp" : cents < 0 ? "flat" : "");
</script>

<div class="gauge" class:intune={inTune} class:locked>
  {#if listening}
    <div class="hint">listening… play a note</div>
    <div class="bar"><span class="mid"></span><span class="mk idle"></span></div>
  {:else}
    <div class="head">
      <span class="note">{note}<span class="oct">{octave}</span></span>
      <span class="cents">{cents > 0 ? "+" : ""}{cents}¢ {inTune ? "✓" : word}{locked ? " · locked" : ""}</span>
    </div>
    <div class="bar">
      <span class="mid"></span>
      <span class="mk" style="left: {pct}%"></span>
    </div>
    <div class="scale"><span>♭ −50</span><span>0</span><span>+50 ♯</span></div>
  {/if}
</div>

<style>
  .gauge { display: flex; flex-direction: column; gap: 8px; }
  .hint { color: var(--muted); font-style: italic; font-size: 0.85rem; }
  .head { display: flex; align-items: baseline; gap: 12px; }
  .note { font-size: 1.9rem; font-weight: 600; line-height: 1; color: var(--fg); }
  .oct { font-size: 0.9rem; color: var(--muted); }
  .cents { font-size: 0.85rem; color: var(--cyan, #4fc3d4); }
  .bar { position: relative; height: 16px; background: var(--bg-raised); border-radius: 8px; }
  .mid { position: absolute; left: 50%; top: 0; bottom: 0; width: 2px; background: var(--muted); }
  .mk { position: absolute; top: -3px; width: 8px; height: 22px; border-radius: 3px; background: var(--cyan, #4fc3d4); transform: translateX(-50%); transition: left 80ms linear; }
  .mk.idle { left: 50%; background: var(--muted); }
  .intune .cents, .intune .note { color: var(--solid, #5fd38a); }
  .intune .mk { background: var(--solid, #5fd38a); box-shadow: 0 0 8px var(--solid, #5fd38a); }
  .scale { display: flex; justify-content: space-between; font-size: 0.65rem; color: var(--muted); }
  .locked .mk { animation: pulse 0.4s ease-out; }
  @keyframes pulse { 0% { transform: translateX(-50%) scale(1.6); } 100% { transform: translateX(-50%) scale(1); } }
</style>
