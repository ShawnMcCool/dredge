<script lang="ts">
  import { onMount } from "svelte";
  import { cmd } from "../lib/ipc";
  import { actions, lastMidiTrigger, openSong, settings, PEDAL_MAPPING, type PedalBinding } from "../lib/stores";
  import { asyncAction } from "../lib/async-action.svelte";
  import { fmtDur } from "../lib/format";
  import Button from "../lib/ui/Button.svelte";
  import SectionHead from "../lib/ui/SectionHead.svelte";

  // Must match the Rust `run_trigger` match in crates/server/src/app.rs exactly.
  const ACTIONS: { id: string; label: string; slot: boolean }[] = [
    { id: "play_pause", label: "play / pause", slot: false },
    { id: "restart_loop", label: "restart loop", slot: false },
    { id: "play_marker", label: "play from marker", slot: true },
    { id: "set_marker", label: "set marker", slot: true },
    { id: "activate_snapshot", label: "activate snapshot", slot: true },
    { id: "cycle_snapshots", label: "cycle snapshots", slot: false },
  ];
  const MARKER_SLOTS = [1, 2, 3, 4, 5, 6];

  const act = asyncAction();

  let devices = $state<string[]>([]);
  let learning = $state<number | null>(null);
  let learnSeq = 0;

  const rows = $derived(($settings[PEDAL_MAPPING] as PedalBinding[] | undefined) ?? []);
  const markers = $derived($openSong?.markers ?? []);

  function needsSlot(action: string): boolean {
    return ACTIONS.find((a) => a.id === action)?.slot ?? false;
  }

  async function refreshDevices(): Promise<void> {
    const r = await cmd<{ devices: string[] }>("midi.status");
    devices = r.devices;
  }

  onMount(() => {
    void refreshDevices();
    const t = setInterval(() => void refreshDevices(), 5000);
    return () => clearInterval(t);
  });

  // Learn flow: while a row is armed, the next pedal press fills its trigger.
  // Re-learning steals the trigger from whatever row already held it (last
  // learn wins, no dialog/confirmation). Setting `learning = null` before the
  // write is what stops this effect from reprocessing the same midi event
  // once `rows` re-derives from the settings store round-trip; `learnSeq` is
  // only ever bumped from `arm`, never inside the effect.
  $effect(() => {
    const ev = $lastMidiTrigger;
    if (learning === null || !ev || ev.seq <= learnSeq) return;
    const i = learning;
    const next = rows.map((r, idx) =>
      idx === i ? { ...r, trigger: ev.trigger } : r.trigger === ev.trigger ? { ...r, trigger: "" } : r,
    );
    learning = null;
    void act.run(() => actions.setPedalMapping(next));
  });

  function arm(i: number): void {
    learnSeq = $lastMidiTrigger?.seq ?? 0;
    learning = learning === i ? null : i;
  }

  function setRow(i: number, patch: Partial<PedalBinding>): Promise<void> {
    return act.run(() => actions.setPedalMapping(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r))));
  }

  function setAction(i: number, action: string): void {
    // Slot actions always persist a concrete slot (default 1); others drop it.
    void setRow(i, { action, slot: needsSlot(action) ? (rows[i].slot ?? 1) : undefined });
  }

  function setSlot(i: number, raw: number): void {
    if (!Number.isFinite(raw) || raw < 1) return;
    void setRow(i, { slot: Math.floor(raw) });
  }

  function addRow(): void {
    void act.run(() => actions.setPedalMapping([...rows, { trigger: "", action: "play_pause" }]));
  }

  function removeRow(i: number): void {
    if (learning === i) learning = null;
    void act.run(() => actions.setPedalMapping(rows.filter((_, idx) => idx !== i)));
  }

  function markerAt(slot: number): number | null {
    return markers.find((m) => m.slot === slot)?.pos ?? null;
  }

  function setMarker(slot: number) {
    return act.run(() => actions.setMarker(slot));
  }

  function playMarker(slot: number) {
    return act.run(() => actions.playMarker(slot));
  }

  function clearMarker(slot: number) {
    return act.run(() => actions.clearMarker(slot));
  }
</script>

{#if act.error}
  <div class="error">{act.error}</div>
{/if}

<section class="group">
  <SectionHead>device</SectionHead>
  {#if devices.length === 0}
    <p class="dim">no MIDI device</p>
  {:else}
    <ul class="devlist">
      {#each devices as d (d)}
        <li>{d}</li>
      {/each}
    </ul>
  {/if}
</section>

<section class="group">
  <SectionHead>mapping</SectionHead>
  <div class="rows">
    {#each rows as row, i (i)}
      <div class="row">
        <Button variant="chip" active={learning === i} onclick={() => arm(i)} title="learn from next pedal press">
          {learning === i ? "…" : row.trigger || "learn"}
        </Button>
        <select class="action-sel" value={row.action} onchange={(e) => setAction(i, e.currentTarget.value)} aria-label="action">
          {#each ACTIONS as a (a.id)}<option value={a.id}>{a.label}</option>{/each}
        </select>
        {#if needsSlot(row.action)}
          <input
            class="slot-inp"
            type="number"
            min="1"
            value={row.slot ?? 1}
            onchange={(e) => setSlot(i, e.currentTarget.valueAsNumber)}
            aria-label="slot"
          />
        {/if}
        <Button variant="chip" onclick={() => removeRow(i)} title="remove binding">×</Button>
      </div>
    {/each}
  </div>
  <Button onclick={addRow}>add binding</Button>
</section>

{#if $openSong}
  <section class="group">
    <SectionHead>markers</SectionHead>
    <div class="rows">
      {#each MARKER_SLOTS as slot (slot)}
        {@const pos = markerAt(slot)}
        <div class="row">
          <span class="slot-num mono">{slot}</span>
          <span class="readout">{pos == null ? "—" : fmtDur(pos)}</span>
          <Button variant="chip" onclick={() => setMarker(slot)} title="set from playhead">set</Button>
          {#if pos != null}
            <Button variant="chip" onclick={() => playMarker(slot)} title="play from marker">play</Button>
            <Button variant="chip" onclick={() => clearMarker(slot)} title="clear marker">×</Button>
          {/if}
        </div>
      {/each}
    </div>
  </section>
{/if}

<style>
  .group {
    margin-bottom: calc(var(--space) * 2.5);
  }
  .group:last-child {
    margin-bottom: 0;
  }

  .dim {
    color: var(--muted);
    font-size: 12px;
    margin: 0;
  }

  .devlist {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
  }

  .devlist li {
    padding: 5px 8px;
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 6px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .action-sel {
    flex: 1;
    height: var(--control-h);
    padding: 0 6px;
    font-size: 12px;
    cursor: pointer;
  }

  .slot-inp {
    width: 3.4em;
    font-size: 11px;
    padding: 1px 3px;
  }

  .slot-num {
    width: 1.2em;
    text-align: right;
    color: var(--muted);
    font-size: 12px;
  }

  .row .readout {
    flex: 1;
    font-size: 12px;
  }

  .error {
    color: var(--fg);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 6px 10px;
    font-size: 12px;
    margin-bottom: var(--space);
  }
</style>
