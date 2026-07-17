# Pedal control — markers, isolation snapshots, MIDI mapping

Design approved 2026-07-17. Supersedes the HID/bridge paths in
`docs/superpowers/campaigns/foot-pedal-control.md` (kept as history).

## Context

The M-Vave Chocolate Plus arrived and is confirmed working over USB in **U
(USB device) mode**: it enumerates as an ALSA MIDI client named `SINCO`
(Jieli Technology, USB `4353:4b4d`). In its default configuration each of the
four footswitches sends a single **Program Change** on channel 0 (PC 0–3),
press-only — no release event, so hold semantics need the pedal reprogrammed
to CC via the M-Vave app (deferred until an action needs it). The pedal also
supports two-switch combos as extra virtual buttons (to be captured during
verification) and Bluetooth (BLE-MIDI, which BlueZ also exposes as an ALSA
port — no dredge-side difference).

The original campaign assumed four fixed HID keystrokes and zero dredge code.
The requirement changed: assignments must be customizable **inside dredge**,
including actions that have no keyboard shortcut. So this is a native feature
with three independently shippable pieces, in build order:

1. **Song markers** — useful without any pedal.
2. **Isolation snapshots** — useful without any pedal.
3. **MIDI input + mapping** — turns pedal events into commands.

## Decisions

- **Native MIDI in the server crate** (not a bridge daemon, not frontend
  WebMIDI). The listener dispatches through the same `App` surface as every
  other client; desktop and `dredged` both get pedal support.
- **Global mapping.** One `{trigger → action}` list applies to every song.
  Per-song variation lives in the data the actions point at (where marker 2
  is, what snapshot 3 holds), never in the mapping.
- **Slot idiom.** Markers and snapshots are numbered slots (1..N, not capped
  at the pedal's button count) so a mapping row can reference "marker 2"
  stably across songs.
- **No special "full band" state.** The snapshot cycle walks saved slots
  only; a full-mix snapshot is just one you save.

## 1 — Song markers

**Model.** `Marker { slot: u32, pos: f64 }` (seconds); `markers: Vec<Marker>`
on `BundleManifest` with `#[serde(default)]`. Per song, travels with the
bundle, rides the existing atomic-rewrite persistence and `song.open`
payload.

**Commands.**
- `marker.set {slot, pos?}` — `pos` omitted = current playhead ("set marker
  from playhead").
- `marker.clear {slot}`
- `marker.play {slot}` — seek to the marker, then play.

**UI.**
- Numbered pips on the waveform; click a pip to seek. (Drag-to-adjust is a
  possible later nicety, not in scope.)
- A markers row in the pedal tab: per slot, current time, set-from-playhead,
  clear.

## 2 — Isolation snapshots

**Model.** `IsolationSnapshot { slot: u32, name: Option<String>, state:
Isolation }` reusing the existing `Isolation` struct (bass focus, levels,
mutes, solos); `snapshots: Vec<IsolationSnapshot>` on `BundleManifest`,
`#[serde(default)]`, per song.

**Commands.**
- `isolation.snapshot.save {slot, name?}` — captures current isolation state.
- `isolation.snapshot.activate {slot}` — applies via the existing
  `isolation.set` path so faders/persistence/events behave as manual moves.
- `isolation.snapshot.cycle` — next occupied slot, wrapping. The active-slot
  cursor is transient runtime state per open song, not persisted.
- `isolation.snapshot.clear {slot}`

**UI.** A snapshot slot row inside the isolation control box: click a chip to
activate, save/clear affordances, active chip in the accent color. Exact
gestures settle during UI batch work.

## 3 — MIDI input + mapping

**Listener.** New `crates/server/src/midi.rs`, own thread, `midir` crate
(ALSA backend; libasound already required). Rescans input ports on a slow
timer, auto-connects to everything except `Midi Through` — hotplug, USB or
BLE, zero configuration. No connected device ⇒ the module idles and keeps
rescanning.

**Triggers.** MIDI events normalize to compact keys: `pc:<ch>:<num>`,
`cc:<ch>:<num>:press|release`, `note:<ch>:<num>`. Device-agnostic: today's
switch 1 is `pc:0:0`; a reprogrammed CC switch or a second bank is just a new
key.

**Mapping.** Global list of `{trigger, action}` stored as JSON in the
settings table under `pedal_mapping` (settings DB, not the bundle). Malformed
or missing ⇒ empty mapping. Actions are a curated registry, each expanding to
an existing dispatcher command:

| Action | Params | Expands to |
|--------|--------|-----------|
| play/pause toggle | — | `play` / `pause` by current transport state |
| play from marker | slot | `marker.play` |
| set marker | slot | `marker.set` (playhead form) |
| activate snapshot | slot | `isolation.snapshot.activate` |
| cycle snapshots | — | `isolation.snapshot.cycle` |
| restart loop | — | seek to loop/span start (the `r` behavior) |

Future actions are one registry entry each.

**Learn flow.** The server pushes `midi.event {trigger}` to clients on every
incoming pedal event (footswitch rates are trivial). The UI's learn button
arms itself and takes the next event. No server-side learn state.

**UI.** New **pedal tab** in the dock (`TAB_VIEWS` in `App.svelte`):
connected-device status, mapping rows (trigger chip · action picker · param
picker · learn · remove), plus the markers row.

## Testing

Pure and unit-tested: trigger parsing/formatting, mapping lookup, action →
command expansion, snapshot cycle order/wrapping, marker set/clear/play
command handling, manifest round-trips with the new fields. Untested I/O
edge: the midir connection glue only.

## Out of scope / deferred

- Hold/momentary semantics (needs pedal reprogrammed to CC press/release).
- Bar-step scan actions from the original campaign (add to the registry if
  wanted later).
- Marker drag-to-adjust on the waveform.
- Capturing the two-switch combo triggers (verification-time detail).
