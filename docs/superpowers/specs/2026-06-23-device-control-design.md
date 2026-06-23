# Device control + "control box" rename — design

**Date:** 2026-06-23
**Status:** approved design, pre-plan

## Intent

Let the user choose which audio **output** and **input** devices dredge uses,
from a new **`devices`** tab in the right pane, with selections that take effect
live (mid-playback) and persist across restarts. Do this by *extending the
audio-backend abstraction the codebase already has* rather than introducing new
machinery, so that supporting alternatives to PipeWire on other OSes/distros
stays a matter of adding a cfg arm.

Bundled with this is a **vocabulary change** (doc-only): every box on the stage
is now called a **"control box"**.

## Background — the abstraction already exists, asymmetrically

dredge already commits to a backend-agnostic audio layer via a **cfg-gated
split**:

- `engine/output.rs` (PipeWire, Linux) ↔ `engine/output_cpal.rs` (cpal, others)
- `engine/capture.rs` (PipeWire) ↔ `engine/capture_cpal.rs` (cpal); `capture.rs`
  re-exports the cpal impl so callers never know which backend they got.

Callers (`output::spawn`, `engine::capture::list_input_sources` /
`start_capture`) are already backend-neutral. The gaps:

1. **Output has no device enumeration or selection** — it always grabs the
   system default (`output_cpal.rs` uses `default_output_device`; the PipeWire
   stream sets no `target.object`).
2. **The one device-descriptor type leaks PipeWire.** `CaptureNode` carries
   `serial` / `object.serial`, a Linux-only concept, into the protocol.

This design **finishes and symmetrizes** that existing split. It does **not**
introduce an `AudioBackend` trait object — runtime dispatch buys runtime
*backend-family* swapping we don't need, and would replace the working cfg
pattern. We only need device swapping *within* one backend.

## Engine: neutral device type + symmetric enumeration

Introduce a backend-neutral descriptor used by both directions:

```rust
pub struct AudioDevice {
    pub id: String,        // opaque, backend-stable; never interpreted by callers
    pub name: String,      // human label for the list
    pub is_default: bool,  // true for the current system-default device
}
```

- **`id` is opaque.** The PipeWire backend encodes `object.serial` into it; the
  cpal backend uses the device name. The server, protocol, settings, and UI
  store and echo `id` back but never parse it. This is what stops PipeWire-isms
  from leaking across the boundary.
- `CaptureNode` is generalized into `AudioDevice` (input enumeration returns the
  neutral type). The tuner, which currently targets by `CaptureNode`/`node_id`,
  moves to targeting by opaque `id`.
- Add **output** enumeration symmetric to input:
  - `engine::device::list_output_devices() -> Result<Vec<AudioDevice>>`
  - `engine::device::list_input_devices()  -> Result<Vec<AudioDevice>>`
  - each cfg-gated PipeWire/cpal, in the same shape as the existing split.

The output thread spawn gains a target:

```rust
output::spawn(cmd_rx, evt_tx, song_slot, target: Option<String>)
// None  = system default. On Linux PipeWire then follows the default sink live
//         (no target.object set); on cpal "default" is resolved once at spawn.
// Some  = bind to the device whose opaque id matches.
```

## Engine: live output-device switching (snapshot + replay)

`Engine` gains `set_output_device(target: Option<String>)`. The output thread is
torn down and a fresh one spawned targeting the new device. Because the
`RenderCore` lives on the audio thread and can't be cheaply moved back out of the
PipeWire stream's user-data (the listener owns it), the **`Engine` rebuilds the
audio path and replays the live state it already tracks** onto a fresh
`RenderCore`. This keeps everything inside `Engine` — `App` tracks no engine
state today and gains none — and avoids any cross-thread move of the FFI-backed
`Stretcher`.

Mechanism (teardown mirrors the proven `capture::run_capture`):

- The output thread gains a `stop: Arc<AtomicBool>` polled by a repeating ~100 ms
  PipeWire timer that calls `mainloop.quit()` — identical to the capture thread.
  `output::spawn` keeps returning `JoinHandle<()>`; `RenderCore` is still built
  inside the thread.
- `Engine` keeps a small pure `EngineState` snapshot, updated on every
  `EngineCmd` it forwards through `send()` (loop region, rate, pitch, bass focus,
  mute, per-stem gains, volume, play/pause), and tracks the latest playback
  position from `Position` events drained in `poll_events`.
- `set_output_device`: set `stop = true` and `join()` the old thread; create
  **fresh** command/event ring buffers; build a fresh `RenderCore` from the
  unchanged `song_slot` `Arc` (it reloads the song via swap-detection — no
  re-decode); `output::spawn(new rings, song_slot, new_target, stop)`; replace
  `Engine`'s `cmd_tx`/`evt_rx` with the new ends; then replay the snapshot as a
  command sequence — volume, rate, pitch, focus, mute, observed stem gains,
  loop region — followed by `SeekSecs(position)` and `Play` if it was playing.
  A brief gain ramp/gap on resume is acceptable.

PipeWire targets a device by setting the `TARGET_OBJECT` property to the opaque
device id (the `object.serial` string) before `connect` — exactly as
`capture::run_capture` already does for input (`None` = no property = follow the
system default sink, which PipeWire then tracks live). cpal selects the matching
output device by name when (re)building the stream.

Input switching is unchanged — it reuses the tuner's existing stop/restart
capture path (`RealTuner::start` stops then starts a fresh capture session); it
just targets by opaque `id` now.

## Server / protocol

New commands on the single dispatcher (`server::app::App`), following existing
naming:

- `device.outputs` → `Vec<AudioDevice>` (enumerate outputs)
- `device.inputs`  → `Vec<AudioDevice>` (enumerate inputs)
- `device.setOutput` `{ id: String | null }` — null = follow system default;
  triggers the live switch.
- `device.setInput`  `{ id: String | null }` — null = follow system default.

Enumeration of outputs is potentially heavy (registry scan); follow the
lock-phasing convention in `app.rs` if it blocks the pump, as input enumeration
already does on its own scan thread.

## Persistence

Two settings keys in the existing `settings` table:

- output device override: opaque `id`, or empty = follow system default.
- input device override: opaque `id`, or empty = follow system default.

Startup / missing-device behavior:

- On startup, enumerate. If a saved override `id` is present in the list, apply
  it. If it is **absent** (device unplugged), fall back to **system default
  silently** but **keep the saved id**, so the device re-binds automatically the
  next time enumeration sees it (e.g. on a manual refresh / tab open).

## UI — the `devices` tab

A new tab registered in the `TAB_VIEWS` registry in `App.svelte`, joining
structure/loops/capture/export/profile/settings/guide, backed by a new
`Devices.svelte`. (Placement is the **right pane**, per the panel/tabs
vocabulary — not a stage control box.)

Layout: two labeled sections, **output** and **input**. Each is a vertical
**list of device buttons** in the tuner idiom (`class="dev"`, selected gets
`.sel`) — **no `<select>` element, no Fader**:

```
output
  ▸ System default   (Speakers)      ← follow; highlighted when not overriding
    Scarlett 2i2
    HDMI Audio
input
  ▸ System default   (Built-in Mic)
    Scarlett 2i2

            [ reset to system ]
```

- The first row in each list is **"System default"**; it is the active
  (highlighted) row whenever that direction is not overridden. The device that
  is *currently* the system default is marked (`is_default`), so the user can see
  what "follow" resolves to.
- Picking any real device below overrides that direction **independently**
  (output and input are separate). This per-direction "System default" entry is
  what replaces a master "override system" checkbox.
- **"reset to system"** sets both directions back to follow-system.
- All UI state derives from dispatch responses/events per the project's
  single-source-of-truth rule; the devices tab mirrors `device.*` wire shapes in
  `lib/stores.ts`.

### No volume in this tab

Volume is deliberately out of scope here. dredge's volume is already
**app-owned**: the master playback fader in `Transport.svelte`
(`playback_volume`, 0–1.5 → `EngineCmd::SetVolume`) and per-stem gains in the
isolation control box are sample multipliers in the `RenderCore`, independent of
any OS device volume. A "follow system vs override" model is a *routing* concept
and does not transfer to a continuous, inherently-app-owned level; a device-level
volume would just duplicate the existing relativistic player volume. So the tab
is purely device selection.

## Tuner interaction

The tuner keeps its own input picker, but its default selection becomes
**"default"** = follow the global input device chosen in the `devices` tab. It
can still override to a specific device. Resolution chain:

```
system default  →  devices-tab input override  →  tuner override
```

## Vocabulary change (doc-only)

Every box on the stage is now a **"control box"**. The terms "controls box" (for
Transport) and the bare collective "boxes" retire. Update:

- `CLAUDE.md` (the UI vocabulary section)
- the `ui-vocabulary` auto-memory
- any other doc that names these stage elements

No code/CSS class renames are required (existing classes like `main.stage` and
the `Box` widget stay); this is a naming/terminology change only.

## Testing

- `AudioDevice` enumeration for input/output exercised behind the existing mock
  seam (the `TunerControl` Real/Mock pattern generalizes).
- `EngineState` snapshot/replay is pure: unit-test that observing a command
  stream yields the right snapshot and that `replay_cmds(pos)` emits the
  expected ordered `EngineCmd`s (volume→…→loop→seek→play-if-playing).
- `set_output_device` on a mock `AudioControl` records the requested target
  (None vs Some(id)); the persistence + startup-application logic in `App` is
  tested against that mock (saved override applied on construction, silent
  fallback when absent, `device.setOutput` persists + forwards). The live switch
  itself is verified manually (needs real audio).
- Tuner "follow default" resolution: with no tuner override, the tuner resolves
  to the devices-tab input; with an override, it does not.
- Persistence: saved override applied when present; silent fallback to default
  when absent while retaining the saved id.

## Out of scope

- An `AudioBackend` trait object / runtime backend-family switching.
- Owning or mirroring OS hardware device volume.
- Adding a non-PipeWire/non-cpal backend (ALSA/JACK) now — the design only keeps
  that a cfg-arm away.
