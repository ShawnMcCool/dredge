# Operation Profiling — Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the `dredge` binary first-class, internal timing/device/engine records for the heavy operations (analysis + stems), persisted and emitted, plus an analysis-device setting with Rust-orchestrated GPU→CPU recovery.

**Architecture:** A `ProfileRun`/`ProfileStage` domain type timed in Rust with `std::time::Instant`; runs persist to a new SQLite `profiles` table (schema V4) and emit a `profile_run` event over the existing `tick()` broadcast. The `Analyzer`/`StemSeparator` traits gain a `force_cpu` flag that sets `CUDA_VISIBLE_DEVICES=""` on the spawned subprocess; `analysis.run` reads the `analysis_device` setting and, in `auto` mode, retries on CPU when a CUDA-OOM SongFormer run fell back to novelty.

**Tech Stack:** Rust (workspace crates `practice`, `server`), `rusqlite` (bundled), `serde`/`serde_json`, `std::time`, `std::process::Command`. No new dependencies.

**Scope note:** This plan is backend only and is fully testable via `cargo test -p practice` / `cargo test -p server` and the control socket. The frontend (settings toggle, engine caption, profiling panel, prepare-modal line) and `song.open`/`import`/`grab` decode timing are a separate follow-on plan.

---

## File Structure

- `crates/practice/src/model.rs` — add `ProfileStage`, `ProfileRun` wire types (Task 1).
- `crates/practice/src/store.rs` — `SCHEMA_V4`, migration bump, `save_profile`, `list_profiles` (Task 2).
- `crates/server/src/analysis.rs` — `force_cpu` on the trait + `ScriptAnalyzer`, `FakeAnalyzer`; `songformer_venv_present()` (Task 3).
- `crates/server/src/stems.rs` — `force_cpu` on `StemSeparator` + `DemucsSeparator` + fake (Task 4).
- `crates/server/src/profile.rs` — new `Timer` helper (Task 5).
- `crates/server/src/app.rs` — profile channel + `tick()` emit (Task 6); `analysis_run` device+recovery+profile (Task 7); `stems_separate` profile (Task 8); `profiles.list` command (Task 9).

---

## Task 1: ProfileRun / ProfileStage wire types

**Files:**
- Modify: `crates/practice/src/model.rs`

- [ ] **Step 1: Add the types**

Append to `crates/practice/src/model.rs` (after the `Analysis`/`AnalysisSection` block):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileStage {
    pub name: String,
    pub ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One timed run of a heavy operation. `started_at` is assigned by the store
/// on save (SQLite `datetime('now')`); the in-flight value is ignored on insert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileRun {
    pub op: String, // "analysis" | "stems" | "open" | "import" | "grab"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song_id: Option<SongId>,
    #[serde(default)]
    pub started_at: String,
    pub total_ms: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>, // "gpu" | "cpu" | "auto" | null
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>, // analysis only
    pub stages: Vec<ProfileStage>,
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p practice`
Expected: builds clean (no warnings about the new types).

- [ ] **Step 3: Commit**

```bash
git add crates/practice/src/model.rs
git commit -m "feat(practice): ProfileRun/ProfileStage wire types"
```

---

## Task 2: profiles table (schema V4) + store methods

**Files:**
- Modify: `crates/practice/src/store.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/practice/src/store.rs` (the other store tests build a fresh DB with `Store::open_in_memory()` — match that):

```rust
#[test]
fn profiles_roundtrip_and_trim() {
    let store = Store::open_in_memory().unwrap();
    let run = crate::model::ProfileRun {
        op: "analysis".into(),
        song_id: Some(crate::model::SongId(7)),
        started_at: String::new(),
        total_ms: 1234,
        ok: true,
        error: None,
        device: Some("cpu".into()),
        engine: Some("songformer".into()),
        stages: vec![crate::model::ProfileStage { name: "analyze".into(), ms: 1234, note: None }],
    };
    let started = store.save_profile(&run).unwrap();
    assert!(!started.is_empty(), "store assigns a timestamp");

    let listed = store.list_profiles(10).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].op, "analysis");
    assert_eq!(listed[0].total_ms, 1234);
    assert_eq!(listed[0].engine.as_deref(), Some("songformer"));
    assert_eq!(listed[0].stages.len(), 1);
    assert!(!listed[0].started_at.is_empty());

    // trim keeps only the most recent 200
    for i in 0..205 {
        let mut r = run.clone();
        r.total_ms = i;
        store.save_profile(&r).unwrap();
    }
    assert_eq!(store.list_profiles(1000).unwrap().len(), 200);
}
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p practice profiles_roundtrip_and_trim`
Expected: FAIL — `no method named save_profile`.

- [ ] **Step 3: Add SCHEMA_V4 and bump migration**

After the `SCHEMA_V3` const in `crates/practice/src/store.rs`:

```rust
/// v4: per-operation profiling runs (heavy ops). `stages` is JSON.
const SCHEMA_V4: &str = "
CREATE TABLE profiles (
    id INTEGER PRIMARY KEY,
    op TEXT NOT NULL,
    song_id INTEGER,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    total_ms INTEGER NOT NULL,
    ok INTEGER NOT NULL,
    error TEXT,
    device TEXT,
    engine TEXT,
    stages_json TEXT NOT NULL
);
";
```

In `migrate()`, after the `version < 3` block:

```rust
        if version < 4 {
            self.conn.execute_batch(SCHEMA_V4)?;
            self.conn.pragma_update(None, "user_version", 4)?;
        }
```

- [ ] **Step 4: Add the store methods**

Add to `impl Store` (near `save_analysis`/`get_analysis`), importing `ProfileRun`/`ProfileStage` at the top `use crate::model::...` line:

```rust
    /// Insert one profiling run; trims history to the most recent 200.
    /// Returns the `started_at` SQLite assigned.
    pub fn save_profile(&self, run: &crate::model::ProfileRun) -> Result<String> {
        let started: String = self.conn.query_row(
            "INSERT INTO profiles (op, song_id, total_ms, ok, error, device, engine, stages_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             RETURNING started_at",
            params![
                run.op,
                run.song_id.map(|s| s.0),
                run.total_ms,
                run.ok as i64,
                run.error,
                run.device,
                run.engine,
                serde_json::to_string(&run.stages)?,
            ],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "DELETE FROM profiles WHERE id NOT IN
                (SELECT id FROM profiles ORDER BY id DESC LIMIT 200)",
            [],
        )?;
        Ok(started)
    }

    pub fn list_profiles(&self, limit: i64) -> Result<Vec<crate::model::ProfileRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT op, song_id, started_at, total_ms, ok, error, device, engine, stages_json
             FROM profiles ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |row| {
                let stages: String = row.get(8)?;
                Ok(crate::model::ProfileRun {
                    op: row.get(0)?,
                    song_id: row.get::<_, Option<i64>>(1)?.map(crate::model::SongId),
                    started_at: row.get(2)?,
                    total_ms: row.get::<_, i64>(3)? as u64,
                    ok: row.get::<_, i64>(4)? != 0,
                    error: row.get(5)?,
                    device: row.get(6)?,
                    engine: row.get(7)?,
                    stages: serde_json::from_str(&stages).map_err(json_err)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
```

- [ ] **Step 5: Run the test to confirm it passes**

Run: `cargo test -p practice profiles_roundtrip_and_trim`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/practice/src/store.rs
git commit -m "feat(practice): profiles table (V4) + save_profile/list_profiles"
```

---

## Task 3: `force_cpu` on the Analyzer + venv probe

**Files:**
- Modify: `crates/server/src/analysis.rs`

- [ ] **Step 1: Update the failing test**

In `crates/server/src/analysis.rs` tests, change the existing
`analyze_parses_the_stdout_contract_and_ignores_stderr` to pass `false`, and add
a new test asserting `force_cpu` sets the env. Replace the stub body so it echoes
`CUDA_VISIBLE_DEVICES`:

```rust
    #[test]
    fn force_cpu_sets_cuda_visible_devices_empty() {
        let dir = tempfile::tempdir().unwrap();
        // stub prints engine reflecting whether CUDA was hidden
        let script = stub_script(
            dir.path(),
            r#"if [ -z "${CUDA_VISIBLE_DEVICES+x}" ]; then ENG=unset; \
elif [ -z "$CUDA_VISIBLE_DEVICES" ]; then ENG=cpu; else ENG=gpu; fi
echo "{\"bpm\":1.0,\"beats\":[],\"downbeats\":[],\"sections\":[],\"engine\":\"$ENG\"}""#,
        );
        let a = ScriptAnalyzer::with_script(script);
        assert_eq!(a.analyze(Path::new("/tmp/x.mp3"), true).unwrap().engine, "cpu");
        assert_eq!(a.analyze(Path::new("/tmp/x.mp3"), false).unwrap().engine, "unset");
    }
```

Also update the existing `analyze_parses...` and `analyze_surfaces_stderr_tail_on_failure` calls from `.analyze(Path::new("/tmp/x.mp3"))` to `.analyze(Path::new("/tmp/x.mp3"), false)`.

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server force_cpu_sets_cuda_visible_devices_empty`
Expected: FAIL — `analyze` takes 1 argument.

- [ ] **Step 3: Change the trait + impls**

In `crates/server/src/analysis.rs`:

Trait:
```rust
    fn analyze(&self, audio: &Path, force_cpu: bool) -> Result<Analysis, String>;
```

`ScriptAnalyzer::analyze` — set the env when `force_cpu`:
```rust
    fn analyze(&self, audio: &Path, force_cpu: bool) -> Result<Analysis, String> {
        let script = self.script.as_ref().ok_or(
            "analysis script not found — expected <repo>/scripts/analyze (or set $DREDGE_ANALYZE)",
        )?;
        let mut cmd = std::process::Command::new(script);
        cmd.arg(audio);
        if force_cpu {
            cmd.env("CUDA_VISIBLE_DEVICES", "");
        }
        let output = cmd
            .output()
            .map_err(|e| format!("failed to run {}: {e}", script.display()))?;
        // ...rest unchanged (stderr handling + JSON parse)...
```

`FakeAnalyzer::analyze`:
```rust
    fn analyze(&self, _audio: &Path, _force_cpu: bool) -> Result<Analysis, String> {
        Ok(fake_analysis())
    }
```

Add the venv probe (top-level fn in `analysis.rs`):
```rust
/// True when the optional SongFormer venv python is present and executable —
/// mirrors `scripts/analyze_impl.py::songformer_python`.
pub fn songformer_venv_present() -> bool {
    use std::os::unix::fs::PermissionsExt;
    let venv = std::env::var_os("DREDGE_SONGFORMER_VENV")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".local/share/dredge/songformer-venv")
        });
    let py = venv.join("bin/python");
    std::fs::metadata(&py)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
```

- [ ] **Step 4: Run tests to confirm pass**

Run: `cargo test -p server --lib analysis`
Expected: PASS (the force_cpu test and the updated existing ones).

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/analysis.rs
git commit -m "feat(server): Analyzer.analyze gains force_cpu + songformer_venv_present probe"
```

---

## Task 4: `force_cpu` on the StemSeparator

**Files:**
- Modify: `crates/server/src/stems.rs`

- [ ] **Step 1: Write/adjust the test**

In `crates/server/src/stems.rs` tests, add a test that the binary runs with the env set. Since `separate` shells out, assert via a stub binary on a tempdir PATH; simplest is to test the env wiring with a fake. Add:

```rust
    #[test]
    fn separate_forwards_force_cpu_env() {
        let dir = tempfile::tempdir().unwrap();
        // stub `demucs` that fails unless CUDA_VISIBLE_DEVICES is empty,
        // proving force_cpu reached the Command env.
        let bin = dir.path().join("demucs");
        std::fs::write(
            &bin,
            "#!/bin/sh\nif [ -z \"${CUDA_VISIBLE_DEVICES+x}\" ]; then exit 7; fi\nexit 9\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let sep = DemucsSeparator { binary: bin.to_string_lossy().into_owned() };
        let out = dir.path().join("out");
        // force_cpu=true → env present → stub exits 9 (not 7); separate returns Err
        // containing the demucs failure (it never produces stems), which is fine —
        // we only assert the env was set by checking the exit code path.
        let err = sep.separate(Path::new("/tmp/a.mp3"), &out, true).unwrap_err();
        assert!(err.contains("9"), "force_cpu must set the env: {err}");
    }
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server separate_forwards_force_cpu_env`
Expected: FAIL — `separate` takes 2 args.

- [ ] **Step 3: Change the trait + impl**

In `crates/server/src/stems.rs`:

Trait:
```rust
    fn separate(&self, audio: &Path, out_dir: &Path, force_cpu: bool) -> Result<Vec<PathBuf>, String>;
```

`DemucsSeparator::separate` — set env on the `Command`:
```rust
        let mut cmd = std::process::Command::new(&self.binary);
        cmd.args(Self::command_args(audio, &tmp));
        if force_cpu {
            cmd.env("CUDA_VISIBLE_DEVICES", "");
        }
        let output = cmd
            .output()
            .map_err(|e| format!("failed to run {}: {e}", self.binary))?;
```

Update `FakeSeparator::separate` (defined in this file, a unit struct) to the new
signature, ignoring `_force_cpu`. Also update the **existing** call in the
stems.rs tests — `FakeSeparator.separate(&src, &out_dir)` becomes
`FakeSeparator.separate(&src, &out_dir, false)`.

- [ ] **Step 4: Run to confirm pass**

Run: `cargo test -p server --lib stems`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/stems.rs
git commit -m "feat(server): StemSeparator.separate gains force_cpu"
```

---

## Task 5: the `Timer` helper

**Files:**
- Create: `crates/server/src/profile.rs`
- Modify: `crates/server/src/lib.rs` (add `mod profile;`)

- [ ] **Step 1: Write the failing test**

Create `crates/server/src/profile.rs`:

```rust
//! Minimal stopwatch for heavy-op profiling. No framework — `Instant` + stages.

use practice::model::{ProfileRun, ProfileStage};
use practice::model::SongId;
use std::time::Instant;

pub struct Timer {
    op: String,
    song_id: Option<SongId>,
    start: Instant,
    stages: Vec<ProfileStage>,
}

impl Timer {
    pub fn new(op: &str, song_id: Option<SongId>) -> Self {
        Self { op: op.into(), song_id, start: Instant::now(), stages: Vec::new() }
    }

    /// Time `f`, record a stage with `name`, return f's value.
    pub fn stage<T>(&mut self, name: &str, f: impl FnOnce() -> T) -> T {
        let t0 = Instant::now();
        let out = f();
        self.stages.push(ProfileStage {
            name: name.into(),
            ms: t0.elapsed().as_millis() as u64,
            note: None,
        });
        out
    }

    /// Attach a note to the most recently recorded stage.
    pub fn note_last(&mut self, note: &str) {
        if let Some(s) = self.stages.last_mut() {
            s.note = Some(note.into());
        }
    }

    pub fn finish(
        self,
        ok: bool,
        error: Option<String>,
        device: Option<String>,
        engine: Option<String>,
    ) -> ProfileRun {
        ProfileRun {
            op: self.op,
            song_id: self.song_id,
            started_at: String::new(),
            total_ms: self.start.elapsed().as_millis() as u64,
            ok,
            error,
            device,
            engine,
            stages: self.stages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_records_stages_and_total() {
        let mut t = Timer::new("analysis", Some(SongId(3)));
        let v = t.stage("a", || 21 + 21);
        assert_eq!(v, 42);
        t.note_last("ok");
        let run = t.finish(true, None, Some("cpu".into()), Some("songformer".into()));
        assert_eq!(run.op, "analysis");
        assert_eq!(run.song_id, Some(SongId(3)));
        assert_eq!(run.stages.len(), 1);
        assert_eq!(run.stages[0].name, "a");
        assert_eq!(run.stages[0].note.as_deref(), Some("ok"));
        assert_eq!(run.device.as_deref(), Some("cpu"));
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/server/src/lib.rs`, add alongside the other `mod` lines:

```rust
mod profile;
```

(If `app.rs` needs it, also `pub(crate) use profile::Timer;` is fine — but `mod profile;` + `crate::profile::Timer` works.)

- [ ] **Step 3: Run the test**

Run: `cargo test -p server timer_records_stages_and_total`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/profile.rs crates/server/src/lib.rs
git commit -m "feat(server): Timer helper for op profiling"
```

---

## Task 6: profile channel on App + tick() emit

**Files:**
- Modify: `crates/server/src/app.rs`

- [ ] **Step 1: Add the channel fields**

In the `App` struct (after `analysis_rx` / `analyzing`):

```rust
    /// Finished profiling runs; drained by `tick()`, persisted, emitted as
    /// `profile_run`.
    profile_tx: mpsc::Sender<ProfileRun>,
    profile_rx: mpsc::Receiver<ProfileRun>,
```

Add the import near the other `practice::model` imports at the top of `app.rs`:
```rust
use practice::model::ProfileRun;
```

In `App::new`, create and store the channel:
```rust
        let (profile_tx, profile_rx) = mpsc::channel();
```
and add `profile_tx, profile_rx,` to the struct literal.

- [ ] **Step 2: Drain + persist + emit in `tick()`**

In `tick()`, right after the analysis-draining `while let Ok((song_id, result)) = self.analysis_rx.try_recv()` loop, add:

```rust
        // finished profiling runs: persist (store on this thread) then emit
        while let Ok(mut run) = self.profile_rx.try_recv() {
            match self.store.save_profile(&run) {
                Ok(started) => run.started_at = started,
                Err(e) => eprintln!("dredge: profile save failed: {e}"),
            }
            if let Ok(data) = serde_json::to_value(&run) {
                events.push(Event { event: "profile_run".into(), data });
            }
        }
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p server`
Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/app.rs
git commit -m "feat(server): App profile channel; tick persists + emits profile_run"
```

---

## Task 7: analysis_run — read device setting, time + recover, send profile

**Files:**
- Modify: `crates/server/src/app.rs`

Server App tests are **integration tests** in `crates/server/tests/*.rs` that
build `App::new(...)`, call `app.set_analyzer(...)`, dispatch via
`app.dispatch(Request{..})`, and drain events from `app.tick()` (see
`app_analysis.rs`). Create a new file `crates/server/tests/app_profiling.rs`
modeled on `app_analysis.rs`:

```rust
//! Profiling + analysis device control through the dispatcher. Hermetic.
use practice::store::Store;
use serde_json::{json, Value};
use server::analysis::{fake_analysis, songformer_venv_present, Analyzer, FakeAnalyzer};
use server::app::App;
use server::capture_control::MockCapture;
use server::control::MockEngine;
use server::protocol::Request;
use server::stems::FakeSeparator;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn req(app: &mut App, cmd: &str, params: Value) -> Value {
    let resp = app.dispatch(Request { id: 1, cmd: cmd.into(), params });
    assert!(resp.ok, "{cmd} failed: {:?}", resp.error);
    resp.data
}

fn write_test_wav(path: &std::path::Path) {
    let samples: Vec<f32> = (0..48_000)
        .flat_map(|i| {
            let v = (i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            [v, v]
        })
        .collect();
    engine::capture::write_wav(path, &samples).unwrap();
}

struct Ctx { app: App, song_id: i64, _dir: tempfile::TempDir }

fn setup(analyzer: Arc<dyn Analyzer>) -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("song.wav");
    write_test_wav(&wav);
    let mut app = App::new(
        Store::open_in_memory().unwrap(),
        Box::new(Arc::new(Mutex::new(MockEngine::default()))),
        Box::new(MockCapture::default()),
        Arc::new(FakeSeparator),
    );
    app.set_analyzer(analyzer);
    let song = req(&mut app, "song.import", json!({"path": wav.to_string_lossy()}));
    let song_id = song["id"].as_i64().unwrap();
    Ctx { app, song_id, _dir: dir }
}

/// Poll tick() until a named event lands (≤15 s); returns its data.
fn wait_for_event(app: &mut App, name: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        for ev in app.tick() {
            if ev.event == name { return ev.data; }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no {name} event within 15 s");
}

// gpu call → novelty, cpu call → songformer (exercises the recovery branch)
struct DeviceAwareAnalyzer;
impl Analyzer for DeviceAwareAnalyzer {
    fn analyze(&self, _a: &std::path::Path, force_cpu: bool) -> Result<practice::model::Analysis, String> {
        let mut x = fake_analysis();
        x.engine = if force_cpu { "songformer".into() } else { "beat_this+novelty".into() };
        Ok(x)
    }
    fn is_available(&self) -> bool { true }
}

#[test]
fn analysis_cpu_setting_forces_cpu_and_profiles() {
    let mut ctx = setup(Arc::new(DeviceAwareAnalyzer));
    req(&mut ctx.app, "settings.set", json!({"key":"analysis_device","value":"cpu"}));
    req(&mut ctx.app, "analysis.run", json!({"song_id": ctx.song_id, "force": true}));
    let data = wait_for_event(&mut ctx.app, "profile_run");
    assert_eq!(data["op"], "analysis");
    assert_eq!(data["device"], "cpu");
    assert_eq!(data["engine"], "songformer");
    assert!(data["total_ms"].as_u64().is_some());
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server --test app_profiling`
Expected: FAIL — `analyze` arity / `profile_run` never arrives.

- [ ] **Step 3: Add the recovery helper to analysis.rs**

In `crates/server/src/analysis.rs`:

```rust
/// Run analysis honoring the device setting, recovering a CUDA-OOM SongFormer
/// fallback onto CPU when `auto`. Records stages into `timer`. Returns the
/// chosen result and the resolved device label.
pub fn analyze_with_recovery(
    analyzer: &dyn Analyzer,
    audio: &Path,
    device_setting: &str,
    timer: &mut crate::profile::Timer,
) -> (Result<Analysis, String>, Option<String>) {
    if device_setting == "cpu" {
        let r = timer.stage("analyze", || analyzer.analyze(audio, true));
        return (r, Some("cpu".into()));
    }
    // auto: GPU first
    let r = timer.stage("analyze (gpu)", || analyzer.analyze(audio, false));
    match &r {
        Ok(a) if a.engine == "songformer" => (r, Some("gpu".into())),
        Ok(_) if songformer_venv_present() => {
            timer.note_last("songformer fell back; retrying on cpu");
            let r2 = timer.stage("analyze (cpu)", || analyzer.analyze(audio, true));
            match &r2 {
                Ok(a2) if a2.engine == "songformer" => (r2, Some("cpu".into())),
                _ => (r2, None),
            }
        }
        _ => (r, None),
    }
}
```

Make sure `crate::profile::Timer` is reachable — `profile` is a sibling module
(`mod profile;` in lib.rs); reference it as `crate::profile::Timer`.

- [ ] **Step 4: Rewrite the `analysis_run` spawn block**

In `analysis_run`, read the setting before spawning and build/send the profile in
the worker. Replace the existing spawn block (the `let analyzer = ...; std::thread::spawn(...)`)
with:

```rust
        self.analyzing.insert(p.song_id.0);
        let analyzer = self.analyzer.clone();
        let tx = self.analysis_tx.clone();
        let profile_tx = self.profile_tx.clone();
        let audio_path = PathBuf::from(&song.path);
        let song_id = p.song_id;
        let device_setting = self
            .store
            .get_setting("analysis_device")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "auto".into());
        std::thread::spawn(move || {
            let mut timer = crate::profile::Timer::new("analysis", Some(song_id));
            let (result, device) = crate::analysis::analyze_with_recovery(
                analyzer.as_ref(),
                &audio_path,
                &device_setting,
                &mut timer,
            );
            let engine = result.as_ref().ok().map(|a| a.engine.clone());
            let err = result.as_ref().err().cloned();
            let run = timer.finish(result.is_ok(), err, device, engine);
            let _ = tx.send((song_id, result));
            let _ = profile_tx.send(run);
        });
        Ok(json!({"state": "running"}))
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p server --test app_profiling analysis_cpu_setting_forces_cpu_and_profiles`
Expected: PASS.

- [ ] **Step 6: Add the auto-recovery test**

Append to `crates/server/tests/app_profiling.rs`:

```rust
#[test]
fn analysis_auto_recovers_to_cpu_when_songformer_present() {
    // Only meaningful when a songformer venv exists; gate on the probe so a
    // machine without it still passes deterministically.
    if !songformer_venv_present() { return; }
    let mut ctx = setup(Arc::new(DeviceAwareAnalyzer)); // default setting = auto
    req(&mut ctx.app, "analysis.run", json!({"song_id": ctx.song_id, "force": true}));
    let data = wait_for_event(&mut ctx.app, "profile_run");
    assert_eq!(data["engine"], "songformer");
    assert_eq!(data["device"], "cpu");
    let stages = data["stages"].as_array().unwrap();
    assert!(stages.iter().any(|s| s["name"] == "analyze (cpu)"));
}
```

Run: `cargo test -p server --test app_profiling analysis_auto_recovers`
Expected: PASS (or a clean no-op return when no venv).

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/app.rs crates/server/src/analysis.rs crates/server/tests/app_profiling.rs
git commit -m "feat(server): analysis.run honors analysis_device + GPU->CPU recovery + profiling"
```

---

## Task 8: stems_separate — profile + device

**Files:**
- Modify: `crates/server/src/app.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/server/tests/app_profiling.rs` (the `setup`'s App already holds
a `FakeSeparator`, whose `separate` writes the four stem WAVs and succeeds):

```rust
#[test]
fn stems_separate_records_a_profile() {
    let mut ctx = setup(Arc::new(FakeAnalyzer));
    req(&mut ctx.app, "stems.separate", json!({"song_id": ctx.song_id}));
    let data = wait_for_event(&mut ctx.app, "profile_run");
    assert_eq!(data["op"], "stems");
    assert!(data["total_ms"].as_u64().is_some());
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server --test app_profiling stems_separate_records_a_profile`
Expected: FAIL — no `profile_run` for stems yet / `separate` arity.

- [ ] **Step 3: Update the stems spawn block**

In `stems_separate`, read the device setting before spawning, pass `force_cpu`,
and send a profile. Replace the spawn block:

```rust
        let separator = self.separator.clone();
        let tx = self.job_tx.clone();
        let profile_tx = self.profile_tx.clone();
        let separating = self.separating.clone();
        let audio_path = PathBuf::from(&song.path);
        let song_id = p.song_id;
        let force_cpu = self
            .store
            .get_setting("analysis_device")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(str::to_owned))
            .map(|s| s == "cpu")
            .unwrap_or(false);
        let device = if force_cpu { "cpu" } else { "auto" }.to_string();
        std::thread::spawn(move || {
            let mut timer = crate::profile::Timer::new("stems", Some(song_id));
            let result = timer.stage("demucs", || separator.separate(&audio_path, &cache, force_cpu));
            separating.lock().unwrap().remove(&song_id.0);
            let err = result.as_ref().err().cloned();
            let run = timer.finish(result.is_ok(), err.clone(), Some(device), None);
            let data = match result {
                Ok(_) => json!({"song_id": song_id, "state": "done"}),
                Err(e) => json!({"song_id": song_id, "state": "failed", "error": e}),
            };
            let _ = tx.send(Event { event: "stems_progress".into(), data });
            let _ = profile_tx.send(run);
        });
        Ok(json!({"state": "running"}))
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p server --test app_profiling stems_separate_records_a_profile`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/app.rs crates/server/tests/app_profiling.rs
git commit -m "feat(server): stems.separate honors analysis_device + profiling"
```

---

## Task 9: `profiles.list` command

**Files:**
- Modify: `crates/server/src/app.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/server/tests/app_profiling.rs` — run an analysis (which now
persists a profile), then read it back through the new command:

```rust
#[test]
fn profiles_list_returns_recorded_runs() {
    let mut ctx = setup(Arc::new(FakeAnalyzer));
    req(&mut ctx.app, "analysis.run", json!({"song_id": ctx.song_id, "force": true}));
    wait_for_event(&mut ctx.app, "profile_run");
    let v = req(&mut ctx.app, "profiles.list", json!({"limit": 10}));
    let arr = v.as_array().unwrap();
    assert!(arr.iter().any(|r| r["op"] == "analysis"), "lists the analysis run");
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server --test app_profiling profiles_list_returns_recorded_runs`
Expected: FAIL — `unknown command: profiles.list`.

- [ ] **Step 3: Add the command**

In `dispatch_inner`'s match (next to `settings.get_all`):
```rust
            "profiles.list" => self.profiles_list(p),
```

Add the handler (near `settings_*`):
```rust
    fn profiles_list(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default = "default_limit")]
            limit: i64,
        }
        fn default_limit() -> i64 { 50 }
        let p: P = from_params(p).unwrap_or(P { limit: 50 });
        serde_json::to_value(self.store.list_profiles(p.limit).err_str()?).err_str()
    }
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p server --test app_profiling profiles_list_returns_recorded_runs`
Expected: PASS.

- [ ] **Step 5: Full gate**

Run: `cargo test -p server && cargo test -p practice && cargo clippy -p server -p practice --all-targets -- -D warnings`
Expected: all PASS, no clippy warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/app.rs crates/server/tests/app_profiling.rs
git commit -m "feat(server): profiles.list command"
```

---

## Manual verification (after Task 9)

- [ ] Build and run the daemon: `just build && just daemon` (or `just run`).
- [ ] Force CPU: `just cmd '{"id":1,"cmd":"settings.set","params":{"key":"analysis_device","value":"cpu"}}'`
- [ ] Re-analyze a song (force) and confirm via `just cmd '{"id":2,"cmd":"profiles.list","params":{"limit":5}}'` that a run appears with `"device":"cpu"`, `"engine":"songformer"`, and a non-zero `total_ms`.
- [ ] Subscribe and watch for a `profile_run` event during analysis.

---

## Self-review checklist (done while writing)

- **Spec coverage:** ProfileRun/stages (T1), profiles table V4 + bounded history (T2), device setting honored + env injection (T3/T4), Rust GPU→CPU recovery (T7), whole-op timing for analysis (T7) and stems (T8), `profile_run` event (T6), `profiles.list` (T9). Engine field reused, no contract change. **Frontend + open/import/grab timing intentionally deferred to Plan B (noted in scope).**
- **Test location:** server App tests are integration tests under `crates/server/tests/` (verified: `app_analysis.rs`, `app_settings.rs` use `App::new` + `set_analyzer` + `app.dispatch` + `tick()` event draining). Tasks 7–9 add `crates/server/tests/app_profiling.rs` in that style; Tasks 3/4 edit the inline `#[cfg(test)]` modules in `analysis.rs`/`stems.rs`; Task 2 edits `store.rs` tests (`Store::open_in_memory()`).
- **Placeholder scan:** none — every code step has full code. The only verify-on-execute item: the exact constructor args to `App::new` (store/audio/capture/separator) — confirmed against `app_analysis.rs::setup`.
- **Type consistency:** `analyze(audio, force_cpu)`, `separate(audio, out, force_cpu)`, `Timer::{new,stage,note_last,finish}`, `ProfileRun` fields, `save_profile -> String`, `list_profiles(i64)` consistent across tasks.
