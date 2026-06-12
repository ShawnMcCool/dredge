# earworm v1 — Plan 3: `server` crate (dispatcher + control socket)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One dispatch layer joining `practice` (store, plan runner, scheduler) to `engine` (audio), exposed over a JSON-lines Unix socket — producing `earwormd`, a fully scriptable headless looper that runs practice plans.

**Architecture:** `App` owns the `Store`, an `AudioControl` (trait over the real `Engine` or a test mock), the open song, and the active `PlanRunner`. Every operation is a `dispatch(Request) -> Response` call; the socket and (later) Tauri share it. A pump loop calls `App::tick()` ~every 50 ms: it drains engine events, drives the plan runner (loop-wrap = rep done), and returns events for broadcast to subscribed socket clients. std-lib threading + `Arc<Mutex<App>>` — no async runtime.

**Tech Stack:** serde/serde_json, practice + engine crates, std `UnixListener`. Dev: hound (WAV fixture), tempfile.

**Known engine API facts (from Plan 2 as built):** `engine::Engine::start() -> Result<Engine>`, `.load(SongBuffer)`, `.send(EngineCmd)` (`&mut`), `.poll_events() -> Vec<EngineEvent>`; `EngineCmd::{Play, Pause, SeekSecs(f64), SetLoopSecs{start,end}, ClearLoop, SetRate(f64), SetPitchScale(f64), BassFocus(bool), Mute(bool)}`; `EngineEvent::{Position{secs,rate,playing}, LoopWrapped, Finished}`; `engine::decode::{decode_file, file_hash}`; `engine::peaks::load_or_compute`. Verify signatures against the source before coding.

**Spec:** `docs/superpowers/specs/2026-06-12-earworm-design.md`

---

### Task 1: Protocol types

**Files:**
- Modify: `crates/server/Cargo.toml` (deps: serde, serde_json, thiserror, time workspace; `practice = { path = "../practice" }`, `engine = { path = "../engine" }`; dev: tempfile, hound)
- Create: `crates/server/src/protocol.rs`
- Modify: `crates/server/src/lib.rs`

- [x] **Step 1: Write types + failing parse tests**

`crates/server/src/protocol.rs`:
```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Request {
    pub id: u64,
    pub cmd: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64, data: Value) -> Self {
        Self { id, ok: true, data, error: None }
    }
    pub fn err(id: u64, msg: impl Into<String>) -> Self {
        Self { id, ok: false, data: Value::Null, error: Some(msg.into()) }
    }
}

/// Broadcast event — one JSON line: {"event": "...", "data": {...}}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Event {
    pub event: String,
    pub data: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_parses_with_and_without_params() {
        let r: Request = serde_json::from_str(r#"{"id":1,"cmd":"play"}"#).unwrap();
        assert_eq!(r.cmd, "play");
        assert!(r.params.is_null());
        let r: Request =
            serde_json::from_str(r#"{"id":2,"cmd":"rate","params":{"value":0.8}}"#).unwrap();
        assert_eq!(r.params["value"], 0.8);
    }

    #[test]
    fn responses_serialize_compactly() {
        let ok = serde_json::to_string(&Response::ok(1, serde_json::Value::Null)).unwrap();
        assert_eq!(ok, r#"{"id":1,"ok":true}"#);
        let err = serde_json::to_string(&Response::err(2, "nope")).unwrap();
        assert_eq!(err, r#"{"id":2,"ok":false,"error":"nope"}"#);
    }
}
```

- [x] **Step 2: Run, pass, commit**

Run: `cargo test -p server` — 2 PASS.

```bash
git add -A && git commit -m "feat(server): json-lines protocol types"
```

---

### Task 2: `AudioControl` trait + mock

**Files:**
- Create: `crates/server/src/control.rs`
- Modify: `crates/server/src/lib.rs`

- [ ] **Step 1: Write trait, real impl, mock**

```rust
use engine::buffer::SongBuffer;
use engine::pipeline::{EngineCmd, EngineEvent};

/// Everything App needs from the audio side — real Engine or test mock.
pub trait AudioControl: Send {
    fn load(&mut self, buf: SongBuffer);
    fn send(&mut self, cmd: EngineCmd);
    fn poll_events(&mut self) -> Vec<EngineEvent>;
}

impl AudioControl for engine::Engine {
    fn load(&mut self, buf: SongBuffer) {
        engine::Engine::load(self, buf);
    }
    fn send(&mut self, cmd: EngineCmd) {
        engine::Engine::send(self, cmd);
    }
    fn poll_events(&mut self) -> Vec<EngineEvent> {
        engine::Engine::poll_events(self)
    }
}

/// Test double: records commands, plays back queued events.
#[derive(Default)]
pub struct MockEngine {
    pub sent: Vec<EngineCmd>,
    pub queued_events: std::collections::VecDeque<EngineEvent>,
    pub loaded_frames: Option<usize>,
}

impl AudioControl for MockEngine {
    fn load(&mut self, buf: SongBuffer) {
        self.loaded_frames = Some(buf.frames());
    }
    fn send(&mut self, cmd: EngineCmd) {
        self.sent.push(cmd);
    }
    fn poll_events(&mut self) -> Vec<EngineEvent> {
        self.queued_events.drain(..).collect()
    }
}
```

(Adjust the `engine::Engine` method receivers to match the as-built API — if `load` takes `&self`, the impl still works through `&mut`.)

- [ ] **Step 2: Compile-check, commit**

Run: `cargo build -p server`

```bash
git add -A && git commit -m "feat(server): AudioControl trait with engine impl and test mock"
```

---

### Task 3: `App` — construction + library commands

**Files:**
- Create: `crates/server/src/app.rs`
- Modify: `crates/server/src/lib.rs`
- Test: `crates/server/tests/app_library.rs`

- [ ] **Step 1: App skeleton and library dispatch**

`crates/server/src/app.rs` — structure:

```rust
use crate::control::AudioControl;
use crate::protocol::{Event, Request, Response};
use practice::model::*;
use practice::runner::{PlanRunner, RepMode, RepSpec};
use practice::store::Store;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct App {
    store: Store,
    audio: Box<dyn AudioControl>,
    open_song: Option<OpenSong>,
    active_plan: Option<ActivePlan>,
    last_position: Option<(f64, f64, bool)>, // secs, rate, playing
}

struct OpenSong {
    song: Song,
    duration_secs: f64,
}

struct ActivePlan {
    plan_id: PlanId,
    runner: PlanRunner,
    loops: HashMap<LoopId, LoopRegion>,
}

impl App {
    pub fn new(store: Store, audio: Box<dyn AudioControl>) -> Self { /* fields */ }

    pub fn dispatch(&mut self, req: Request) -> Response {
        let id = req.id;
        match self.dispatch_inner(&req.cmd, req.params) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::err(id, e),
        }
    }

    fn dispatch_inner(&mut self, cmd: &str, p: Value) -> Result<Value, String> {
        match cmd {
            "song.import" => self.song_import(p),
            "song.list" => Ok(serde_json::to_value(self.store.list_songs().err_str()?).err_str()?),
            "song.open" => self.song_open(p),
            "section.replace" => self.section_replace(p),
            "loop.create" => self.loop_create(p),
            "loop.delete" => self.loop_delete(p),
            "loop.list" => self.loop_list(p),
            "junctions.derive" => self.junctions_derive(p),
            "plan.save" => self.plan_save(p),
            "plan.list" => self.plan_list(p),
            // transport + plan-run commands arrive in Tasks 4–5
            _ => Err(format!("unknown command: {cmd}")),
        }
    }
}
```

Use a tiny private extension trait (`err_str`) mapping any `Display` error to `String` to keep arms one-liners.

Command behaviors (params shapes shown as JSON):
- `song.import {path}`: `engine::decode::file_hash`; if `store.song_by_hash` hits, return the existing Song. Else `decode_file` (need duration), derive title from file stem, `insert_song`, **then** if a sidecar exists (`practice::sidecar::read_sidecar`) restore its sections/loops/plans into the store (sidecar wins over empty DB; ids re-assigned by insertion). Return Song.
- `song.open {song_id}`: load song row (`NotFound` if absent), `decode_file(path)`, compute peaks via `engine::peaks::load_or_compute(&buf, &hash)`, `audio.load(buf)`, set `open_song`. Return `{song, sections, loops, plans, peaks}`.
- `section.replace {song_id, sections: [{name,start,end,position}]}` → store.replace_sections, **then auto-refresh junction loops**: delete existing `LoopKind::Junction` loops for the song, re-derive via `practice::junction::derive_junctions(&sections, 2.0, 2.0)`, insert them. Return new sections + junction loops.
- `loop.create {song_id, name, start, end}` (kind Manual), `loop.delete {loop_id}`, `loop.list {song_id}`.
- `junctions.derive {song_id, tail?, head?}` (defaults 2.0): same as the refresh above, explicit trigger.
- `plan.save {song_id, name, steps}` (steps = the serde representation pinned in practice::model tests), `plan.list {song_id}`.

After any mutation of sections/loops/plans: write the sidecar (`practice::sidecar::write_sidecar`) for the affected song; ignore (log to stderr) sidecar IO errors — DB is the primary store.

- [ ] **Step 2: Write failing tests**

`crates/server/tests/app_library.rs` — helpers: temp dir; generate a 2 s 44.1 kHz WAV with hound (reuse the pattern from `crates/engine/tests/decode.rs`); `App::new(Store::open_in_memory()... , Box::new(MockEngine::default()))`. Note `App` needs a way to inspect the mock in tests — give `App` `#[cfg(test)]`-free access via `pub fn audio_mut(&mut self) -> &mut dyn AudioControl` and downcast… **simpler:** make tests construct `App` with the mock, and assert effects through dispatch responses + store-visible state only; engine-command assertions happen in Task 4's tests where they matter, via a small `Arc<Mutex<MockEngine>>` wrapper implementing `AudioControl` (commands forwarded; test keeps a clone).

Tests (write with real JSON requests through `dispatch`):
1. `import_then_list_then_open` — import the WAV; response has a song id and the title equals the file stem; `song.list` shows 1; `song.open` returns peaks with `buckets.len() > 0`; re-import same path returns the same id (hash dedupe).
2. `sections_autoderive_junctions` — open song; `section.replace` with two sections [0–1 s], [1–2 s]; response/`loop.list` contains one junction loop named `"A→B"` spanning ~0.9–1.1? (tail/head 2.0 clamped to section bounds: start = max(1.0-2.0, 0.0) = 0.0, end = min(1.0+2.0, 2.0) = 2.0) — assert kind junction and bounds 0.0/2.0.
3. `loops_and_plans_roundtrip` — create a manual loop, save a plan referencing it (one `play_reps` step), `plan.list` returns it; sidecar file exists next to the WAV and parses (`read_sidecar`).
4. `unknown_command_errors` — `{"cmd":"bogus"}` → ok:false, error contains "unknown".

- [ ] **Step 3: Run (fail), implement, run (pass)**

Run: `cargo test -p server --test app_library` — 4 PASS.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(server): app dispatcher with library commands, junction auto-derive, sidecar write"
```

---

### Task 4: Transport + plan execution driving

**Files:**
- Modify: `crates/server/src/app.rs`
- Test: `crates/server/tests/app_plan_run.rs`

- [ ] **Step 1: Add transport + plan arms and `tick`**

New dispatch arms:
- `"play" | "pause"` → EngineCmd::Play / Pause
- `"seek" {secs}`, `"rate" {value}`, `"loop.set" {start,end}`, `"loop.clear"`, `"bass_focus" {on}`, `"mute" {on}`
- `"pitch" {semitones, cents, octave_up}` → scale = `2f64.powf((semitones + cents/100.0)/12.0) * if octave_up {2.0} else {1.0}` → SetPitchScale
- `"status"` → `{position_secs, rate, playing, song_id, plan: {plan_id, step_idx, rep_idx, mode, loop_id} | null}` from `last_position` + active plan
- `"plan.start" {plan_id}` → load plan + its song's loops; build `PlanRunner`; apply first rep (below); error if no current rep or song not open
- `"plan.stop"` → drop active plan, Pause
- `"plan.skip_step"` → runner.skip_step(); apply new current or finish

Applying a `RepSpec` (private `fn apply_rep(&mut self, spec: RepSpec)`):
```rust
let l = &self.active_plan.as_ref().unwrap().loops[&spec.loop_id];
self.audio.send(EngineCmd::SetLoopSecs { start: l.start, end: l.end });
self.audio.send(EngineCmd::SetRate(spec.rate));
self.audio.send(EngineCmd::Mute(spec.mode == RepMode::RecallSilent));
self.audio.send(EngineCmd::Play);
```

`pub fn tick(&mut self) -> Vec<Event>`:
1. `for ev in self.audio.poll_events()`:
   - `Position{..}` → update `last_position`; emit `position` event (the socket layer throttles by only broadcasting the last one per tick — just emit once per tick using the final Position seen).
   - `LoopWrapped` → emit `loop_wrapped`; if a plan is active: record an (unrated) rep row (`store.record_rep` mode = rep mode string, rate, is_retest false), advance the runner; if the step index changed emit `step_finished {step_idx: old}`; if `current()` is `Some(spec)` and spec differs (always apply anyway) → `apply_rep`, emit `rep_changed {loop_id, rate, mode, step_idx, rep_idx}`; if `None` → Pause, emit `plan_finished`, clear active plan.
   - `Finished` → emit `song_finished`.
2. Return collected events.

- [ ] **Step 2: Write failing tests**

`crates/server/tests/app_plan_run.rs` — use the shared-mock wrapper:
```rust
#[derive(Clone, Default)]
struct SharedMock(std::sync::Arc<std::sync::Mutex<server::control::MockEngine>>);
impl server::control::AudioControl for SharedMock {
    fn load(&mut self, b: engine::buffer::SongBuffer) { self.0.lock().unwrap().load(b) }
    fn send(&mut self, c: engine::pipeline::EngineCmd) { self.0.lock().unwrap().send(c) }
    fn poll_events(&mut self) -> Vec<engine::pipeline::EngineEvent> {
        self.0.lock().unwrap().poll_events()
    }
}
```

Setup helper: import WAV, open song, create two manual loops A [0,1] and B [1,2], save a plan:
steps = `[listen_first(A, 1), play_reps(A, 2, dwell 0.8), recall_test(B, 1, rate 1.0)]`.

Tests:
1. `plan_start_applies_first_rep` — after `plan.start`: mock `sent` ends with SetLoop(0,1), SetRate(1.0) (listen rep rate 1.0), Mute(false), Play.
2. `wraps_drive_progression_through_modes` — push `LoopWrapped` into the mock queue and call `tick()` repeatedly (one wrap per tick), collecting events. Expected rep sequence: listen(A) → play(A)@0.8 ×2 → recall play(B)@1.0 → recall silent(B) → finished. Assert: (a) `rep_changed` events carry modes `["play","play","play","recall_silent"]`-ish — precisely: after wrap1 mode=play rate 0.8; wrap2 → play rate 0.8; wrap3 → mode play loop B (recall audible half); wrap4 → mode recall_silent and mock received `Mute(true)`; wrap5 → `plan_finished` and a final `Pause`. (b) store now holds 5 unrated rep rows.
3. `skip_step_jumps` — start plan, dispatch `plan.skip_step` → current becomes the play_reps step; mock got a new SetRate(0.8).
4. `status_reports_plan_state` — after start, `status` shows step_idx 0, mode "listen".

- [ ] **Step 3: Run (fail), implement, run (pass)**

Run: `cargo test -p server --test app_plan_run` — 4 PASS.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(server): transport commands and event-driven plan execution"
```

---

### Task 5: Ratings, resurfacing, retention commands

**Files:**
- Modify: `crates/server/src/app.rs`
- Test: `crates/server/tests/app_rating.rs`

- [ ] **Step 1: Add arms**

- `"rep.rate" {loop_id, rating: "miss"|"shaky"|"solid", is_retest?}` → `store.record_rep` (mode "rated", rate = last known rate or 1.0, given is_retest default false) + resurfacing: find previous state in `store.all_resurfacing()`, `practice::schedule::next_state(prev, loop_id, rating, today)`, `store.upsert_resurfacing`. Today = `time::OffsetDateTime::now_utc().date()` (UTC is fine for a practice ladder). Return new `{interval_idx, due_on}`.
- `"due.list"` → `practice::schedule::due(&store.all_resurfacing()?, today)` → loop ids, hydrated to `[{loop_id, name, song_id}]` (join via a new `Store` query or per-id lookups — simplest: `store.all_resurfacing()` + `loop.list` per song the App has; add `Store::loop_by_id(&self, id: LoopId) -> Result<Option<(LoopRegion, SongId)>>` to practice if needed — if added, include a store test in practice's tests/store.rs and keep its style).
- `"retention" {song_id}` → `store.retention(song_id)` as `[{loop_id, rating, at}]`.

- [ ] **Step 2: Tests**

`crates/server/tests/app_rating.rs`:
1. `rating_solid_schedules_resurfacing` — rate loop solid → response `due_on` is tomorrow (interval_idx 0); rate solid again → interval_idx 1, due +2 days.
2. `due_list_surfaces_overdue` — upsert (via two ratings then manual: just rate once, then assert `due.list` is empty today; can't time-travel — instead directly `store`-level: this test may construct App with a store pre-seeded with an overdue resurfacing row, then `due.list` returns it).
3. `retention_via_dispatch` — record retest reps through `rep.rate {is_retest: true}` twice (shaky then solid), `retention` returns solid.

- [ ] **Step 3: Run (fail), implement, run (pass); commit**

Run: `cargo test -p server --test app_rating` — 3 PASS.

```bash
git add -A && git commit -m "feat(server): rating, resurfacing and retention commands"
```

---

### Task 6: Unix socket server

**Files:**
- Create: `crates/server/src/socket.rs`
- Modify: `crates/server/src/lib.rs`
- Test: `crates/server/tests/socket.rs`

- [ ] **Step 1: Implement**

`crates/server/src/socket.rs`:
```rust
use crate::app::App;
use crate::protocol::{Event, Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn default_socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("earworm.sock")
}

pub struct SocketServer {
    pub app: Arc<Mutex<App>>,
    subscribers: Arc<Mutex<Vec<UnixStream>>>,
}
```

Behavior:
- `pub fn serve(app: Arc<Mutex<App>>, path: &Path) -> std::io::Result<ServerHandle>`: remove stale socket file, bind `UnixListener`, spawn accept thread; each client gets a thread: `BufReader::lines()`; per line: parse `Request` (parse error → `Response::err(0, ...)`), special-case `"subscribe"` (clone stream into `subscribers`, reply ok), else `app.lock().dispatch(req)`; write response + `\n`, flush.
- Pump thread: every 50 ms `app.lock().tick()`; serialize each `Event`, write to every subscriber (drop subscribers whose write fails).
- `ServerHandle { path, shutdown: Arc<AtomicBool>, ... }` with `Drop` removing the socket file; accept loop uses `set_nonblocking(true)` + 50 ms sleep so shutdown flag is honored (also lets one thread double as accept+pump — implementer's choice, keep it simple).

- [ ] **Step 2: Integration test**

`crates/server/tests/socket.rs` — start `serve` on a temp path with MockEngine app; connect with `UnixStream`:
1. `request_response_roundtrip` — send `{"id":7,"cmd":"song.list"}` line, read line, parse: id 7, ok true, data `[]`.
2. `subscribe_receives_events` — client A subscribes; push a `LoopWrapped` into the shared mock; within ~500 ms client A reads a `{"event":"loop_wrapped"...}` line (set read timeout 2 s).
3. `bad_json_gets_error_response` — send `not json\n`, get ok:false.

- [ ] **Step 3: Run (fail→pass); commit**

Run: `cargo test -p server --test socket` — 3 PASS.

```bash
git add -A && git commit -m "feat(server): json-lines unix socket with event broadcast"
```

---

### Task 7: `earwormd` binary

**Files:**
- Create: `crates/server/src/bin/earwormd.rs`

- [ ] **Step 1: Implement**

```rust
// earwormd — headless earworm: real engine + control socket.
// Usage: earwormd [--socket <path>] [--db <path>]
// Defaults: $XDG_RUNTIME_DIR/earworm.sock, ~/.local/share/earworm/earworm.db
```
Parse args by hand (no clap). Create data dir, `Store::open`, `engine::Engine::start()`, `App::new`, `serve`, then park the main thread (`loop { thread::sleep(1s) }`; Ctrl-C kills it, ServerHandle Drop cleanup is best-effort).

- [ ] **Step 2: Smoke test**

```bash
cargo build -p server --release
./target/release/earwormd --socket /tmp/ew-test.sock --db /tmp/ew-test.db &
sleep 1
printf '%s\n' '{"id":1,"cmd":"song.list"}' | timeout 3 nc -U /tmp/ew-test.sock -q1
kill %1
```
Expected: `{"id":1,"ok":true,"data":[]}`. (If `nc` lacks `-q`, use a tiny python one-liner.) Requires PipeWire — present on this machine.

- [ ] **Step 3: Full gate + commit**

Run: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`

```bash
git add -A && git commit -m "feat(server): earwormd headless binary"
```

---

## Self-review checklist

- Spec coverage: shared dispatch layer ✔ (socket + future Tauri both call `App::dispatch`), mpv-style socket at XDG path ✔, events stream ✔, plan execution driven by loop-wrap events ✔ (listen/play/recall modes incl. Mute), junction auto-derive on section save ✔, sidecar written on every mutation ✔, ratings → resurfacing ladder ✔, retention query ✔, headless usable product ✔.
- Type-consistency notes for the implementer: `RepMode::RecallSilent` serializes as `recall_silent`; `practice::store` ratings are text; engine `SetLoopSecs` field names `start`/`end`.
- Deferred: Tauri UI (Plan 4), capture (v2), stems (v3).
