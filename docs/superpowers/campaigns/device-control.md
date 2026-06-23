# Campaign: device control (input/output selection)

Choose which audio **output** and **input** devices dredge uses, from a new
right-pane **`devices`** tab, with selections that take effect live
(mid-playback) and persist across restarts. Built by *finishing the
backend-agnostic audio abstraction the codebase already has* (the cfg-gated
PipeWire/cpal split), so supporting alternatives to PipeWire on other OSes/distros
stays a matter of adding a cfg arm. Bundled with a doc-only vocabulary change:
every box on the stage is now a **"control box"**.

Designed 2026-06-23 (brainstorm + spec at
`docs/superpowers/specs/2026-06-23-device-control-design.md`). Work directly on
`main`.

> **For agentic workers:** use superpowers:subagent-driven-development (or
> executing-plans) to run this. Phases are dependency-ordered; each ends in a
> **verification gate** + a **commit**. Steps use `- [ ]` checkboxes. Phase 1 is
> doc-only. Phases 2–4 are backend; 5 is the first UI; 6–7 wire input + the
> tuner; 8 is polish. Audio device enumeration and live switching are **not**
> unit-testable here (need a live PipeWire + real devices) — those gates are
> empirical via `just cmd` and a human checklist, matching house practice. Pure
> logic (the neutral type, persistence/startup logic behind the mock
> `AudioControl`, tuner default-resolution, frontend stores) **is** unit-tested.

## The mental model (decisions, final)

- **Extend, don't replace.** The engine already commits to a backend-agnostic
  layer via a cfg-gated split: `output.rs`/`capture.rs` (PipeWire, Linux) vs
  `output_cpal.rs`/`capture_cpal.rs` (cpal, others), with `capture.rs`
  re-exporting the cpal impl. We finish two gaps: **output has no device
  enumeration/selection** (it grabs the default), and the one descriptor type
  (`CaptureNode`) **leaks PipeWire** (`serial`/`object.serial`). No
  `AudioBackend` trait object — that would discard the working pattern for
  runtime backend-family swapping we don't need.
- **Neutral boundary type with an opaque id.** `AudioDevice { id, name,
  is_default }`. `id` is opaque and backend-stable: the PipeWire backend encodes
  `object.serial` into it (as a decimal string — capture already targets by
  serial), the cpal backend uses the device name. The protocol/settings/UI store
  and echo `id` but never parse it.
- **Live output switch = `Engine`-side snapshot + replay.** App tracks almost no
  engine state (only a `last_position` cache), and the `RenderCore` can't be
  cheaply moved back out of the PipeWire stream's user-data. So `Engine` — which
  already sees every `EngineCmd` via `send()` — keeps a small pure `EngineState`
  snapshot (loop/rate/pitch/focus/mute/stem-gains/volume/play) + the latest
  position from `Position` events. On switch it tears down the output thread
  (stop flag + 100 ms poll timer → `mainloop.quit()` + `join`, mirroring
  capture), spawns a fresh thread with fresh rings on the new device (the
  `song_slot` Arc is unchanged → no re-decode), and **replays the snapshot**
  (state cmds → `SeekSecs(pos)` → `Play` if playing). No cross-thread move of the
  FFI `Stretcher`; a brief gap on resume is acceptable.
- **"System default" is the not-overriding state, per direction.** The settings
  value is an opaque id, or empty = follow system default. PipeWire follows the
  default sink live when no `TARGET_OBJECT` is set. In the tab, "System default"
  is the first list entry (selected when not overridden); the device currently
  resolving as default is marked via `is_default`.
- **Input has one global selection (the devices tab); the tuner follows it.**
  The only consumer of input is the tuner. The devices tab sets a global
  `input_device`. The tuner keeps its own picker but **defaults to "follow
  global"**, resolving: tuner override → global input → the `is_default` input.
- **No volume in the tab.** dredge's volume is already app-owned (the master
  playback fader + stem gains are sample multipliers in `RenderCore`),
  independent of OS device volume; a follow/override model is a routing concept
  that doesn't transfer.

## Existing surfaces this rides (verified 2026-06-23)

| Need | Mechanism | Location |
|------|-----------|----------|
| PipeWire input scan (mirror for outputs) | registry scan, `media.class`, `object.serial`, `node.description` | `capture.rs:57-116` |
| Target a device | `TARGET_OBJECT` property = serial string | `capture.rs:205-208` |
| Thread teardown | `stop: Arc<AtomicBool>` + 100 ms timer → `mainloop.quit()` + `join` | `capture.rs:122-151, 285-306` |
| Output thread | `output::spawn(cmd_rx, evt_tx, song_slot) -> JoinHandle<()>` | `output.rs:29-42` |
| Output stream connect (add target) | `stream.connect(Output, None, flags, params)` | `output.rs:129-136` |
| RenderCore (owns Pipeline+volume; built *in-thread* today) | `RenderCore::new`/`fill` | `render_core.rs:23-86` |
| Engine handle | `Engine{cmd_tx,evt_rx,song_slot,_audio_thread}`; `start()` | `engine.rs:6-44` |
| Mockable audio seam | `trait AudioControl {load,send,poll_events}` + impl for `Engine` | `control.rs:4-21` |
| Dispatch arms | `match cmd {...}` | `app.rs:429-479` |
| `send_ok` helper | `self.audio.send(cmd); Ok(Null)` | `app.rs:517-520` |
| Settings read/write | `store.get_setting/set_setting`; `settings.set`/`settings.get_all` | `store.rs:71-89`, `app.rs:474-475, 489-500` |
| String-setting read pattern | `get_setting(k).ok().flatten().and_then(as_str)` | `app.rs:770-777` |
| Engine construction | `Engine::start()` → `Box::new` into `App::new` | `dredged.rs:60-70`, host.rs (desktop) |
| Tuner control trait + Real/Mock | `TunerControl{list_inputs,start,stop,is_running}` | `tuner.rs:39-44, 58-106, 204-234` |
| Tuner dispatch | `tuner.inputs`/`tuner.start`/`tuner.stop`; `tuner_start` | `app.rs:454-459, 1091-1099` |
| cpal capture (mirror for output) | `list_input_sources`/`run_capture` | `capture_cpal.rs:1-99` |
| Engine module list | `pub mod ...` | `lib.rs:1-29` |
| EngineCmd / Pipeline state surface | Play/Pause/Seek/Loop/Rate/Pitch/Bass/Mute/StemGain/Volume | `pipeline.rs:8-33, 42-73` |
| TAB registry | `ALL_TABS`, `TAB_VIEWS`, render | `App.svelte:33-45, 138-147` |
| Tab view style | `<h2>` header, no `Box` | `SettingsPanel.svelte:93` |
| IPC | `cmd<T>(cmd, params)`, `onEvent(handler)` | `ipc.ts:14-56` |
| Settings store + load + setSetting | `settings`, `loadSettings()`, `actions.setSetting` | `stores.ts:321, 400-447, 577-587` |
| Tuner stores + actions | `tunerInputs/tunerInputName/tunerOn`; `refreshTunerInputs/tunerPowerOn/setTunerInput` | `stores.ts:258-265, 868-897` |
| Device-picker markup + CSS | `.picker`/`.dev`/`.dev.sel{border-color:var(--accent)}` | `Tuner.svelte:69-78, 109-133` |
| Accent active state | `var(--accent)` | `Tuner.svelte:132`, `App.svelte:346` |
| FE wire type (input) | `CaptureNode {id,serial,app,media}` | `stores.ts:154-162` |
| vitest style + single run | `describe/it/expect`; `pnpm vitest run lib/<f>.test.ts` | `tuner-math.test.ts` |

**Not unit-testable here:** device enumeration (needs live PipeWire) and the live
output handoff (needs real audio). The cpal arms compile only on non-Linux, so
they're verified by build, not run, on this machine.

---

## Phase 1 — Vocabulary: "control box" (docs only)

**Goal:** the term **control box** replaces "controls box"/"boxes" for every box
on the stage, consistently in the docs/memory that define UI naming. No code or
CSS class renames.

**Files:** `CLAUDE.md`; the auto-memory `ui-vocabulary` file under
`/home/shawn/.claude/projects/-home-shawn-src-dredge/memory/`.

- [ ] **1.1** In `CLAUDE.md`, the "UI vocabulary" section: change the **Stage**
  bullet so the stacked boxes are collectively **control boxes** — i.e. "the
  controls box (`Transport.svelte`)" becomes "the **transport control box**", and
  the row of boxes (isolation/notes/tuner/drill) are **control boxes**. Keep
  every concrete component name; only the collective noun changes. Add one line:
  "Call them *control boxes*, never 'containers'/'panels'/bare 'boxes'."
- [ ] **1.2** Update the `ui-vocabulary` memory file body to say stage holds
  **control boxes** (controls/isolation/notes/tuner/drill), and note the new
  `devices` **tab** in the panel. Keep the `[[...]]` links intact.
- [ ] **1.3** Grep for stray "controls box"/"the boxes" in `docs/` and the other
  campaign/spec files; leave historical campaign records as-is (don't rewrite
  history) but fix any *current* reference doc. `rg -ni "controls box|the boxes"
  docs CLAUDE.md`.
- [ ] **Gate:** `rg -ni "controls box" CLAUDE.md` returns nothing in the vocab
  section. **Commit:** `docs: rename stage 'boxes' to 'control boxes'`.

## Phase 2 — Neutral `AudioDevice` type + enumeration (engine)

**Goal:** `engine::device::{AudioDevice, list_output_devices, list_input_devices}`
reachable over dispatch as `device.outputs` / `device.inputs`.

**Files:** new `crates/engine/src/device.rs`; `crates/engine/src/lib.rs`;
`crates/server/src/app.rs`.

- [ ] **2.1** Create `crates/engine/src/device.rs` with the neutral type:
  ```rust
  #[derive(Debug, Clone, PartialEq, serde::Serialize)]
  pub struct AudioDevice {
      pub id: String,       // opaque, backend-stable; PipeWire: object.serial; cpal: name
      pub name: String,
      pub is_default: bool,
  }
  ```
  Add `pub mod device;` to `lib.rs` (alphabetical, after `decode`).
- [ ] **2.2** Linux enumeration: in `device.rs` add
  `#[cfg(target_os = "linux")] pub fn list_output_devices() -> crate::error::Result<Vec<AudioDevice>>`
  and `list_input_devices()`. Mirror `capture.rs::scan_input_sources`
  (`capture.rs:57-116`): a 300 ms registry scan on a named thread, filtering
  `media.class == "Audio/Sink"` for outputs and `"Audio/Source"` for inputs. Map
  each to `AudioDevice { id: object.serial-as-string (fallback global.id),
  name: node.description → node.nick → node.name, is_default: false }`. Factor
  the shared scan into one private `scan(class: &str) -> Result<Vec<AudioDevice>>`
  to keep the two public fns one-liners (DRY against the duplication temptation).
- [ ] **2.3** is_default (Linux, best-effort): during the same scan, also watch
  the `Metadata` global named `default` and read `default.audio.sink` /
  `default.audio.source` (their value JSON carries `"name": "<node.name>"`). After
  the scan, set `is_default` on the device whose `node.name` matches. If the
  metadata isn't observed, leave all `false` (the UI still works — "System
  default" just won't show a resolved name). Keep this isolated so it can't break
  enumeration.
- [ ] **2.4** cpal enumeration: `#[cfg(not(target_os = "linux"))]` arms in
  `device.rs` mirroring `capture_cpal.rs:15-37` — `host.output_devices()` /
  `host.input_devices()`, `id = name`, `is_default` by comparing each name to
  `host.default_output_device()`/`default_input_device()` name. (Compiles on
  non-Linux only; verified by `cargo check` cross-target is out of scope — just
  keep signatures identical to the Linux arms.)
- [ ] **2.5** Server dispatch: in `app.rs` `dispatch_inner` add, next to the
  `tuner.*` arms:
  ```rust
  "device.outputs" => serde_json::to_value(engine::device::list_output_devices().map_err(|e| e.to_string())?).err_str(),
  "device.inputs"  => serde_json::to_value(engine::device::list_input_devices().map_err(|e| e.to_string())?).err_str(),
  ```
- [ ] **Gate:** `cargo build -p engine -p server`; with a daemon running,
  `just cmd '{"id":1,"cmd":"device.outputs"}'` lists your sinks and
  `just cmd '{"id":1,"cmd":"device.inputs"}'` lists your sources, each with a
  non-empty `name` and a stable `id`, and exactly one `is_default:true` per list
  (or all false if metadata wasn't read). **Commit:**
  `feat(engine): neutral AudioDevice type + output/input enumeration`.

## Phase 3 — Output thread: target + stop + Engine snapshot/replay (engine)

**Goal:** the output thread can be torn down and respawned on a chosen device,
and `Engine` restores playback by replaying a snapshot it maintains.

**Files:** new `crates/engine/src/engine_state.rs` (+ tests);
`crates/engine/src/output.rs`, `output_cpal.rs`, `engine.rs`, `lib.rs`.

- [ ] **3.1** Pure snapshot type. New `engine_state.rs` with:
  ```rust
  #[derive(Default, Clone)]
  pub struct EngineState {
      pub loop_region: Option<(f64, f64)>,
      pub rate: Option<f64>,
      pub pitch_scale: Option<f64>,
      pub bass_focus: bool,
      pub muted: bool,
      pub stem_gains: std::collections::BTreeMap<usize, f32>, // only observed idxs
      pub volume: Option<f32>,
      pub playing: bool,
      pub pos_secs: f64,
  }
  impl EngineState {
      pub fn observe(&mut self, cmd: &EngineCmd) { /* match: set fields; Play→playing=true; Pause→playing=false; SeekSecs→pos_secs; ClearLoop→None; SetLoopSecs→Some */ }
      pub fn set_position(&mut self, secs: f64, playing: bool) { self.pos_secs = secs; self.playing = playing; }
      pub fn replay_cmds(&self) -> Vec<EngineCmd> { /* volume, rate, pitch, BassFocus(bass), Mute(muted) if set, stem gains, loop (Some→SetLoopSecs), SeekSecs(pos), then Play if playing — in THIS order */ }
  }
  ```
  Add `pub mod engine_state;` to `lib.rs`. Unit-test `observe`+`replay_cmds`:
  feed a command sequence, assert the snapshot, assert the replayed `Vec` order
  and contents (and that a paused state omits the trailing `Play`).
- [ ] **3.2** Stop timer in the output thread. In `output.rs::run`, take a
  `stop: Arc<AtomicBool>` and add the exact 100 ms stop-poll timer from
  `capture.rs:285-306` (when set, `ml.quit()`); `drop(timer)` after
  `mainloop.run()`. `output::spawn` keeps returning `JoinHandle<()>` but its
  signature gains `target: Option<String>` and `stop: Arc<AtomicBool>`.
  `RenderCore::new(...)` stays built **inside** the thread as today.
- [ ] **3.3** PipeWire target: build a mutable `Properties` (like
  `capture.rs:198-208`) and, when `target` is `Some(serial_str)`,
  `props.insert(*pw::keys::TARGET_OBJECT, serial_str)` before
  `StreamBox::new`. `None` ⇒ no property ⇒ follow the default sink.
- [ ] **3.4** cpal output (`output_cpal.rs`): match the new signature
  (`target`/`stop`); select the output device whose `name == target` (else
  `default_output_device()`); make the park loop poll `stop` and exit.
- [ ] **3.5** `Engine` struct + `start()` + the switch. Add `stop:
  Arc<AtomicBool>`, `target: Option<String>`, `state: EngineState`, and make
  `cmd_tx`/`evt_rx`/`_audio_thread` reassignable. `send()` calls
  `self.state.observe(&cmd)` before pushing. `poll_events()` updates
  `self.state.set_position(secs, playing)` for each `Position` event. Add:
  ```rust
  pub fn set_output_device(&mut self, target: Option<String>) -> crate::error::Result<()> {
      self.stop.store(true, Ordering::Relaxed);
      if let Some(h) = self._audio_thread.take() { let _ = h.join(); }
      let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(256);
      let (evt_tx, evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(1024);
      self.stop = Arc::new(AtomicBool::new(false));
      self.target = target.clone();
      let h = crate::output::spawn(cmd_rx, evt_tx, self.song_slot.clone(), target, self.stop.clone())?;
      self.cmd_tx = cmd_tx; self.evt_rx = evt_rx; self._audio_thread = h;
      for cmd in self.state.replay_cmds() { let _ = self.cmd_tx.push(cmd); }
      Ok(())
  }
  ```
  `start()` builds the first `stop`/rings and calls `output::spawn(cmd_rx,
  evt_tx, song_slot.clone(), None, stop.clone())`.
- [ ] **Gate:** `cargo test -p engine` green (existing pipeline tests
  unaffected + new `engine_state` tests); `cargo build --workspace` clean;
  `cargo clippy -p engine` no warnings. `set_output_device` isn't reachable over
  dispatch yet (Phase 4). **Commit:**
  `feat(engine): retargetable output thread + EngineState snapshot/replay`.

## Phase 4 — Output selection over dispatch + persistence (server)

**Goal:** `device.setOutput` switches the output live, persists the choice, and
the saved choice is applied at startup.

**Files:** `crates/server/src/control.rs`, `crates/server/src/app.rs` (+ its
tests).

- [ ] **4.1** Extend the audio seam. In `control.rs` add to `AudioControl`:
  `fn set_output_device(&mut self, target: Option<String>);` Impl on
  `engine::Engine` forwards to `Engine::set_output_device` (ignoring the result
  or logging on error). Find the test mock for `AudioControl` (in
  `app.rs`/`control.rs` tests) and add the method, recording the last target in a
  field like `pub last_output: Option<Option<String>>`.
- [ ] **4.2** Dispatch + handler. Add the arm
  `"device.setOutput" => self.device_set_output(p),` and:
  ```rust
  fn device_set_output(&mut self, p: Value) -> Result<Value, String> {
      #[derive(Deserialize)] struct P { id: Option<String> }
      let p: P = from_params(p)?;
      let val = p.id.clone().map(Value::String).unwrap_or(Value::Null);
      self.store.set_setting("output_device", &val).err_str()?;
      self.audio.set_output_device(p.id);
      Ok(Value::Null)
  }
  ```
  (`output_device` setting: a string id, or JSON `null`/absent = follow system.)
- [ ] **4.3** Apply at startup. At the end of `App::new` (after `audio` is in
  place), read the saved id with the `analysis_device` pattern
  (`app.rs:770-777`) and, if `Some(non-empty)`, call
  `audio.set_output_device(Some(id))`. Keep the saved id even if the device is
  currently absent (PipeWire just falls back to default; it re-binds on a later
  switch). Note: `App::new` takes `audio` by value — call this before moving it
  into the struct, or add a small `self`-method invoked once.
- [ ] **4.4** Tests (unit, with the mock). In `app.rs` tests: (a)
  `device.setOutput {id:"123"}` writes the `output_device` setting **and** the
  mock's `last_output == Some(Some("123"))`; (b) `device.setOutput {id:null}`
  writes null and `last_output == Some(None)`; (c) constructing `App` with a
  pre-seeded `output_device` setting calls `set_output_device(Some(id))` on the
  mock during startup.
- [ ] **Gate:** `cargo test -p server` green. Empirical: play a song, then
  `just cmd '{"id":1,"cmd":"device.setOutput","params":{"id":"<serial from
  device.outputs>"}}'` moves audio to that sink within ~a beat, playback
  position intact; `{"id":null}` returns to the default. **Commit:**
  `feat(server): device.setOutput — live switch + persistence + startup apply`.

## Phase 5 — Devices tab + output list (frontend)

**Goal:** a `devices` tab listing outputs; "System default" + each device;
selecting one switches live; "reset to system".

**Files:** `apps/desktop/src/lib/stores.ts`; new
`apps/desktop/src/components/Devices.svelte`; `apps/desktop/src/App.svelte`;
new `apps/desktop/src/lib/devices.ts` + `devices.test.ts` (pure label logic).

- [ ] **5.1** stores.ts types + stores + key. Add:
  ```ts
  export interface AudioDevice { id: string; name: string; is_default: boolean }
  export const OUTPUT_DEVICE = "output_device";
  export const outputDevices = writable<AudioDevice[]>([]);
  export const outputDevice = writable<string | null>(null); // null = system default
  ```
- [ ] **5.2** stores.ts actions:
  ```ts
  async refreshOutputs(): Promise<void> { outputDevices.set(await cmd<AudioDevice[]>("device.outputs")); },
  async setOutputDevice(id: string | null): Promise<void> {
    outputDevice.set(id);
    await cmd("device.setOutput", { id });
    await this.setSetting(OUTPUT_DEVICE, id ?? "");
  },
  ```
  In `loadSettings()` (after the volume block, `stores.ts:414`): if
  `typeof all[OUTPUT_DEVICE] === "string" && all[OUTPUT_DEVICE]`
  `outputDevice.set(all[OUTPUT_DEVICE])` else `outputDevice.set(null)`.
- [ ] **5.3** Pure label helper `lib/devices.ts`:
  `defaultName(devices: AudioDevice[]): string | null` returns the `name` of the
  `is_default` device (for the "System default (Speakers)" annotation). Unit-test
  in `devices.test.ts`: returns the default's name; null when none flagged.
- [ ] **5.4** `Devices.svelte` (mirror SettingsPanel's `<h2>` style + Tuner's
  `.dev`/`.sel` list). `onMount(() => act.run(() => actions.refreshOutputs()))`.
  Markup:
  ```svelte
  <h2>devices</h2>
  <section class="group">
    <h3 class="group-head">output</h3>
    <div class="picker">
      <button class="dev" class:sel={$outputDevice === null}
              onclick={() => actions.setOutputDevice(null)}>
        System default{defaultName($outputDevices) ? ` (${defaultName($outputDevices)})` : ""}
      </button>
      {#each $outputDevices as d (d.id)}
        <button class="dev" class:sel={$outputDevice === d.id}
                onclick={() => actions.setOutputDevice(d.id)}>{d.name}</button>
      {/each}
    </div>
  </section>
  <Button onclick={() => actions.setOutputDevice(null)}>reset to system</Button>
  ```
  Copy the `.picker`/`.dev`/`.dev.sel` CSS from `Tuner.svelte:109-133`
  (`.dev.sel { border-color: var(--accent) }`).
- [ ] **5.5** Register the tab in `App.svelte`: add `"devices"` to `ALL_TABS`
  (`:33`), `import Devices from "./components/Devices.svelte"`, and
  `devices: Devices` in `TAB_VIEWS` (`:36`).
- [ ] **Gate:** `cd apps/desktop && pnpm vitest run lib/devices.test.ts` green;
  `just lint` (svelte-check) clean. Empirical in the real app (`just dev` — the
  Tauri webview can't be driven headlessly): the `devices` tab lists outputs,
  "System default" is highlighted initially, clicking a device switches audio and
  persists across a restart. **Commit:**
  `feat(desktop): devices tab with live output selection`.

## Phase 6 — Input enumeration + global input setting (frontend + server)

**Goal:** the tab's **input** section lists inputs and persists a global
`input_device`; the backend stores it (consumed by the tuner in Phase 7).

**Files:** `crates/server/src/app.rs`; `apps/desktop/src/lib/stores.ts`;
`apps/desktop/src/components/Devices.svelte`.

- [ ] **6.1** Server: `device.setInput` arm + handler, identical shape to
  `device_set_output` but persisting the `input_device` setting and **not**
  touching `self.audio` (input has no live engine stream; the tuner consumes the
  setting on its next start). Add a unit test that it writes `input_device`.
- [ ] **6.2** stores.ts: `INPUT_DEVICE = "input_device"`,
  `inputDevices = writable<AudioDevice[]>([])`,
  `inputDevice = writable<string | null>(null)`; `refreshInputs()` →
  `cmd("device.inputs")`; `setInputDevice(id)` → set store, `cmd("device.setInput",
  {id})`, `setSetting(INPUT_DEVICE, id ?? "")`. Load in `loadSettings()` like
  output. Have `refreshOutputs` callers also refresh inputs (or add a combined
  `refreshDevices()` that does both; call it in `Devices.svelte` onMount).
- [ ] **6.3** `Devices.svelte`: add an `input` `<section>` mirroring output —
  "System default (… )" + each input device, bound to `$inputDevice` /
  `actions.setInputDevice`. "reset to system" resets **both** directions.
- [ ] **Gate:** `cargo test -p server` green; `just lint` clean. Empirical: the
  tab's input section lists sources; selecting one persists `input_device`
  (verify via `just cmd '{"id":1,"cmd":"settings.get_all"}'`). **Commit:**
  `feat(desktop,server): devices tab input list + global input setting`.

## Phase 7 — Tuner follows the global input ("default") (server + frontend)

**Goal:** the tuner targets by **opaque id** and defaults to **follow the global
input**, with its own override still available.

**Files:** `crates/server/src/tuner.rs`; `crates/server/src/app.rs`;
`apps/desktop/src/lib/stores.ts`; `apps/desktop/src/components/Tuner.svelte`.

- [ ] **7.1** Tuner targets by id. Change `TunerControl::start(node_id: u32, …)`
  to `start(device_id: &str, …)`, and `RealTuner::start` to resolve via
  `engine::device::list_input_devices()` (find by `id`), then start capture on
  that device. Since capture targets by `serial` and `AudioDevice.id` *is* the
  serial string for PipeWire, pass it straight through (parse to the type
  `start_capture` needs, or add a thin `start_capture_by_id(&str, secs)` to
  `capture.rs`/`capture_cpal.rs`). Update `MockTuner` + `tuner.rs` tests to the
  string id. `list_inputs` now returns `Vec<AudioDevice>` — drop `CaptureNode`
  from the tuner surface.
- [ ] **7.2** Server: `tuner.inputs` returns `device.inputs` (or just have the
  frontend call `device.inputs`); `tuner_start` takes `{ device_id: String }`.
  Resolution of "default" lives in the frontend (it knows the global input), so
  the server just starts whatever id it's given.
- [ ] **7.3** Frontend resolution. Replace the name-based `tunerInputName` with an
  id-based selection plus a `"default"` sentinel: `tunerInput =
  writable<string | "default">("default")` (persist under the existing
  `tuner_input_name` key or a new `tuner_input`). Add a pure
  `resolveTunerInput(sel, globalInput, inputs)` in `lib/devices.ts`:
  `sel !== "default"` → `sel`; else `globalInput` if set; else the `is_default`
  input's id; else `inputs[0]?.id`. Unit-test all four branches in
  `devices.test.ts`. `tunerPowerOn`/`setTunerInput` use it and call
  `tuner.start { device_id }`.
- [ ] **7.4** `Tuner.svelte` gear picker: first entry **"default (follow
  devices)"** (`class:sel={$tunerInput === "default"}`), then each input device
  by id. Picking "default" stores the sentinel; picking a device stores its id.
  Keep the `.dev`/`.sel` styling.
- [ ] **Gate:** `cargo test -p server` green;
  `pnpm vitest run lib/devices.test.ts` green (resolution branches); `just lint`
  clean. Empirical: with the tuner on "default", changing the devices-tab input
  changes what the tuner listens to; a tuner-specific override wins over the
  global. **Commit:**
  `feat(tuner): target inputs by id; default follows the global input device`.

## Phase 8 — Polish + integration pass

**Goal:** discoverability, teardown correctness, full green.

**Files:** `apps/desktop/src/components/Guide.svelte`; `Devices.svelte`;
`stores.ts`.

- [ ] **8.1** Guide tab: a short blurb on the `devices` tab (output/input
  selection, "System default" = follow). House style: no hand-holding —
  one or two plain sentences.
- [ ] **8.2** Refresh on open + on focus: the tab re-runs `refreshDevices()`
  when shown (devices come and go); confirm a freshly-plugged interface appears
  without an app restart.
- [ ] **8.3** Absent-device audit: a persisted `output_device` whose device is
  unplugged at launch must not break startup — playback runs on the default, the
  saved id is retained, and re-plugging + reselecting re-binds. Verify by saving
  a bogus id and launching.
- [ ] **8.4** Stale-binary note: rebuild release (`just build`) so the running
  app reflects the feature (per the project's known "stale release binary"
  gotcha).
- [ ] **Gate:** `just check` (full `cargo test --workspace` + `pnpm vitest run` +
  lint) green; human checklist in `just dev`: output switches live, input drives
  the tuner, selections survive restart, unplugged device degrades gracefully.
  **Commit:** `feat(desktop): devices tab guide entry + refresh/teardown audit`.

---

## Execution order & status

Dependency-ordered: 1 (docs) is independent and first; 2→3→4 build the engine +
server output path; 5 is the first UI; 6→7 add input + the tuner chain; 8 is
polish. Commit per phase on `main`; `just check` is the final gate, with
per-phase gates as listed (empirical via `just cmd` / `just dev` where audio or
the Tauri webview can't be exercised by automated tests).

**STATUS (2026-06-23): NOT STARTED** — campaign authored from the approved spec.

## Self-review notes

- **Spec coverage:** neutral type + enumeration (Ph2) ✓; live output handoff
  (Ph3) ✓; dispatch + persistence + startup (Ph4) ✓; devices tab output (Ph5) +
  input (Ph6) ✓; tuner follow-default (Ph7) ✓; vocabulary rename (Ph1) ✓; no
  volume in tab (honored — never added). 
- **Opaque id consistency:** `AudioDevice.id` is the PipeWire `object.serial`
  string end to end — enumeration (2.2), output target (3.3), input target
  (7.1) — so the same id round-trips through settings and the tuner without
  parsing.
- **Risk register:** (a) `is_default` via PipeWire metadata (2.3) is best-effort
  and isolated — failure degrades to unmarked "System default", not broken
  enumeration (confirmed false in practice: registry globals don't expose the
  metadata key/values, so all `is_default=false` for now — revisit with a
  metadata listener if the marker is wanted); (b) the live switch rebuilds the
  audio path and replays `EngineState` (Ph3) rather than moving the FFI
  `Stretcher` across threads — no `unsafe Send` needed; the replay order is the
  thing to get right, and it's unit-tested; (c) output `TARGET_OBJECT` is assumed
  symmetric to capture's proven input targeting — verify in the Ph4 empirical
  gate before building UI on it.
- **Backend-uniform:** every PipeWire step names its `capture.rs` mirror; the
  cpal arms keep identical signatures so the cfg split stays the only difference,
  preserving the "add a backend = add a cfg arm" property.
