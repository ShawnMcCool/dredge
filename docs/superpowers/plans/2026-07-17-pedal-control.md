# Pedal Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> Work directly on `main` (repo convention — no feature branches). Spec:
> `docs/superpowers/specs/2026-07-17-pedal-control-design.md`.

**Goal:** Drive dredge from the M-Vave Chocolate Plus foot pedal via in-app customizable MIDI mappings, plus the two per-song features the pedal actions point at: numbered position markers and isolation snapshots.

**Architecture:** Markers and snapshots are new slot-numbered collections on `BundleManifest` (practice crate) with dispatcher commands in `server::app`. A MIDI listener thread (`midir`/ALSA) normalizes pedal events to trigger strings; `App::tick` drains them, looks each up in a global `pedal_mapping` setting, and runs the mapped action through existing command paths. The UI adds a pedal dock tab (mapping editor with learn flow + markers row), snapshot chips in the isolation box, and marker pips on the waveform.

**Tech Stack:** Rust (serde, midir 0.10, mpsc), Svelte 5, vitest, cargo test.

**Verification commands:** `cargo test -p practice`, `cargo test -p server`, `cd apps/desktop && pnpm vitest run`, `just check` at the end.

---

## Wire vocabulary (used by every task — read first)

- **Trigger keys** (strings): `pc:<ch>:<num>` (Program Change), `note:<ch>:<num>` (note-on, vel > 0), `cc:<ch>:<num>:press` / `cc:<ch>:<num>:release` (CC, value ≥ 64 = press). The pedal's four switches today: `pc:0:0` … `pc:0:3`.
- **Actions** (strings, flat binding rows): `play_pause`, `restart_loop`, `play_marker`, `set_marker`, `activate_snapshot`, `cycle_snapshots`. Slot-taking actions carry `slot` in the binding.
- **`pedal_mapping` setting** (settings DB, global): JSON array of `{ "trigger": "pc:0:0", "action": "play_marker", "slot": 2 }` (slot omitted when unused).
- **New commands:** `marker.set {song_id?, slot, pos?}` (pos defaults to playhead), `marker.clear {song_id?, slot}`, `marker.play {song_id?, slot}`, `isolation.snapshot.save {song_id?, slot, name?, state?}`, `isolation.snapshot.activate {song_id?, slot}`, `isolation.snapshot.cycle`, `isolation.snapshot.clear {song_id?, slot}`, `pedal.trigger {trigger}` (debug/test seam), `midi.status` → `{devices: [..]}`. `song_id` always defaults to the open song.
- **New events:** `markers {song_id, markers}`, `snapshots {song_id, snapshots}`, `isolation {song_id, isolation, slot}` (a snapshot was applied server-side; `slot` = which), `midi {trigger}` (every normalized pedal event; feeds the UI learn flow).

---

### Task 1: Marker model + library API (practice crate)

**Files:**
- Modify: `crates/practice/src/model.rs` (add `Marker` near `Isolation`, ~line 155)
- Modify: `crates/practice/src/bundle.rs:28-45` (`BundleManifest`)
- Modify: `crates/practice/src/library.rs` (new API next to `set_isolation`, ~line 458)
- Tests: colocated `#[cfg(test)]` modules in the same files

- [ ] **Step 1: Write failing model/manifest tests**

In `crates/practice/src/model.rs`, after the existing `isolation_tests` module:

```rust
#[cfg(test)]
mod marker_tests {
    use super::*;

    #[test]
    fn marker_round_trips() {
        let m = Marker { slot: 2, pos: 92.5 };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<Marker>(&s).unwrap(), m);
    }
}
```

In `crates/practice/src/bundle.rs`, in (or alongside) the existing manifest test module, add a test that an old manifest JSON without a `markers` key deserializes to an empty vec — copy the shape of the existing "defaults when absent" manifest test in that file (e.g. how `isolation`/`routines` defaults are asserted):

```rust
#[test]
fn manifest_without_markers_defaults_empty() {
    // Build a minimal manifest via the existing test constructor in this
    // module, serialize it, strip nothing — older files simply lack the key.
    // Deserializing any pre-markers manifest must yield markers == [].
    let json = serde_json::to_value(sample_manifest()).unwrap(); // reuse this module's existing sample/helper
    let mut obj = json.as_object().unwrap().clone();
    obj.remove("markers");
    let m: BundleManifest = serde_json::from_value(Value::Object(obj)).unwrap();
    assert!(m.markers.is_empty());
}
```

(If the module has no `sample_manifest()` helper, reuse whatever existing test constructs a `BundleManifest`; match its idiom.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p practice marker`
Expected: compile error — `Marker` and `markers` don't exist.

- [ ] **Step 3: Implement model + manifest field**

`crates/practice/src/model.rs`, after the `Isolation` impl:

```rust
/// A numbered per-song position marker (seconds). Slots are stable handles the
/// global pedal mapping points at ("play from marker 2"); the number is the
/// identity, not an ordering of creation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub slot: u32,
    pub pos: f64,
}
```

`crates/practice/src/bundle.rs`, on `BundleManifest` after `isolation`:

```rust
    #[serde(default)]
    pub markers: Vec<Marker>,
```

(Add `Marker` to the existing `use crate::model::{...}` import in bundle.rs.)

- [ ] **Step 4: Write failing library tests**

In `crates/practice/src/library.rs` tests (follow the existing test module's tempdir + `create_song` idiom in that file):

```rust
#[test]
fn markers_set_overwrite_clear_and_sort() {
    let (mut lib, song_id) = lib_with_song(); // reuse/extend this module's existing helper
    lib.set_marker(song_id, 3, 30.0).unwrap();
    lib.set_marker(song_id, 1, 10.0).unwrap();
    assert_eq!(
        lib.list_markers(song_id).iter().map(|m| m.slot).collect::<Vec<_>>(),
        vec![1, 3]
    );
    lib.set_marker(song_id, 3, 33.0).unwrap(); // overwrite, not duplicate
    assert_eq!(lib.marker(song_id, 3).unwrap().pos, 33.0);
    assert_eq!(lib.list_markers(song_id).len(), 2);
    lib.clear_marker(song_id, 1).unwrap();
    assert!(lib.marker(song_id, 1).is_none());
}
```

If the library test module has no `lib_with_song()` helper, write one modeled on `app_with_song()` in `crates/server/src/app.rs:3917-3936` (tempdir root + `create_song(&audio_path, "Title", Some("Band"), "hash", 1.0)`).

- [ ] **Step 5: Run to verify failure**

Run: `cargo test -p practice markers_set`
Expected: compile error — the three functions don't exist.

- [ ] **Step 6: Implement the library API**

`crates/practice/src/library.rs`, next to `set_isolation`/`get_isolation` (~line 458), using the same `Result` alias and `entry_mut`/`persist` pattern as `insert_loop` (line 288):

```rust
    // ── markers ────────────────────────────────────────────────────────────

    pub fn list_markers(&self, song_id: SongId) -> Vec<Marker> {
        self.entry(song_id)
            .map(|e| e.manifest.markers.clone())
            .unwrap_or_default()
    }

    pub fn marker(&self, song_id: SongId, slot: u32) -> Option<Marker> {
        self.entry(song_id)?
            .manifest
            .markers
            .iter()
            .copied()
            .find(|m| m.slot == slot)
    }

    /// Set (or overwrite) the marker in `slot`. Kept sorted by slot.
    pub fn set_marker(&mut self, song_id: SongId, slot: u32, pos: f64) -> Result<Marker> {
        let entry = self.entry_mut(song_id)?;
        let m = Marker { slot, pos };
        match entry.manifest.markers.iter_mut().find(|x| x.slot == slot) {
            Some(x) => *x = m,
            None => {
                entry.manifest.markers.push(m);
                entry.manifest.markers.sort_by_key(|x| x.slot);
            }
        }
        Self::persist(entry)?;
        Ok(m)
    }

    pub fn clear_marker(&mut self, song_id: SongId, slot: u32) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.markers.retain(|m| m.slot != slot);
        Self::persist(entry)
    }
```

(If there is no `entry(&self, ...)` read accessor, mirror however `list_loops` (line 273) reads an entry.) Add `Marker` to the library's model imports.

- [ ] **Step 7: Run to verify pass**

Run: `cargo test -p practice`
Expected: all pass, including the three new tests.

- [ ] **Step 8: Commit**

```bash
git add crates/practice
git commit -m "feat(practice): per-song numbered markers in the bundle manifest"
```

---

### Task 2: Marker commands (server crate)

**Files:**
- Modify: `crates/server/src/app.rs` — dispatch table (~line 675), new handlers near the transport section (~line 824), `finish_open` payload (~line 2304), tests near `routine_tests`

- [ ] **Step 1: Write failing dispatch tests**

In `crates/server/src/app.rs`, new test module (reuse `app_with_song()` from `routine_tests` — move it up into a shared `#[cfg(test)]` helper region or duplicate it locally; prefer moving it next to `make_shared_mock` (~line 3789) and making both modules use it):

```rust
#[cfg(test)]
mod marker_cmd_tests {
    use super::*;
    use practice::model::Marker;

    #[test]
    fn marker_set_explicit_pos_persists_and_emits() {
        let (mut app, song_id) = app_with_song();
        let resp = app.dispatch(req(
            "marker.set",
            json!({ "song_id": song_id, "slot": 2, "pos": 12.5 }),
        ));
        assert!(resp.ok, "got: {:?}", resp.error);
        assert_eq!(app.library.marker(song_id, 2), Some(Marker { slot: 2, pos: 12.5 }));
        // The mutation queues a "markers" event; tick() drains job_rx.
        let events = app.tick();
        assert!(events.iter().any(|e| e.event == "markers"));
    }

    #[test]
    fn marker_set_without_pos_uses_playhead() {
        let (mut app, song_id) = app_with_song();
        app.last_position = Some((42.0, 1.0, true, None));
        let resp = app.dispatch(req("marker.set", json!({ "song_id": song_id, "slot": 1 })));
        assert!(resp.ok, "got: {:?}", resp.error);
        assert_eq!(app.library.marker(song_id, 1).unwrap().pos, 42.0);
    }

    #[test]
    fn marker_play_seeks_and_plays() {
        let (mock, mut app) = make_shared_mock();
        let song_id = seed_song(&mut app); // see step 3 note on sharing app_with_song with a mock
        app.library.set_marker(song_id, 4, 90.0).unwrap();
        let resp = app.dispatch(req("marker.play", json!({ "song_id": song_id, "slot": 4 })));
        assert!(resp.ok, "got: {:?}", resp.error);
        let sent = &mock.lock().unwrap().sent;
        assert!(sent.iter().any(|c| matches!(c, EngineCmd::SeekSecs(s) if *s == 90.0)));
        assert!(sent.iter().any(|c| matches!(c, EngineCmd::Play)));
    }

    #[test]
    fn marker_play_missing_slot_errors() {
        let (mut app, song_id) = app_with_song();
        let resp = app.dispatch(req("marker.play", json!({ "song_id": song_id, "slot": 9 })));
        assert!(!resp.ok);
    }

    #[test]
    fn marker_cmds_without_song_id_need_an_open_song() {
        let (mut app, _) = app_with_song(); // song exists but is not open
        let resp = app.dispatch(req("marker.set", json!({ "slot": 1, "pos": 0.0 })));
        assert!(!resp.ok, "no open song and no explicit song_id must error");
    }
}
```

Note on helpers: `app_with_song()` in `routine_tests` builds its own `MockEngine` without keeping the shared handle. For `marker_play_seeks_and_plays` you need the mock handle, so add a variant next to `make_shared_mock`:

```rust
    /// make_shared_mock() plus a library song (tempdir root), returning its id.
    fn seed_song(app: &mut App) -> SongId {
        let lib_dir = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let audio = src.path().join("a.flac");
        std::fs::write(&audio, b"AUDIO").unwrap();
        std::mem::forget(src);
        app.set_library_root(lib_dir.path().to_path_buf());
        std::mem::forget(lib_dir);
        app.library
            .create_song(&audio, "Title", Some("Band"), "hash", 1.0)
            .unwrap()
            .id
    }
```

(and rewrite `app_with_song()` as `let (_, mut app) = make_shared_mock(); let id = seed_song(&mut app); (app, id)` so there's one seeding path.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p server marker`
Expected: FAIL — `unknown command: marker.set`.

- [ ] **Step 3: Implement handlers**

Dispatch table (`app.rs`, after `"isolation.set"`):

```rust
            "marker.set" => self.marker_set(p),
            "marker.clear" => self.marker_clear(p),
            "marker.play" => self.marker_play(p),
```

Handlers (new `// --- markers ---` section after the transport section):

```rust
    // --- markers ------------------------------------------------------------

    /// Resolve the target song: explicit id, else the open song.
    fn target_song(&self, explicit: Option<SongId>) -> Result<SongId, String> {
        explicit
            .or_else(|| self.open_song.as_ref().map(|o| o.song.id))
            .ok_or_else(|| "no song open".to_owned())
    }

    fn push_markers_event(&self, song_id: SongId) {
        let _ = self.job_tx.send(Event {
            event: "markers".into(),
            data: json!({ "song_id": song_id, "markers": self.library.list_markers(song_id) }),
        });
    }

    fn marker_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default)]
            song_id: Option<SongId>,
            slot: u32,
            #[serde(default)]
            pos: Option<f64>,
        }
        let p: P = from_params(p)?;
        self.marker_set_slot(p.song_id, p.slot, p.pos)
    }

    fn marker_set_slot(
        &mut self,
        song_id: Option<SongId>,
        slot: u32,
        pos: Option<f64>,
    ) -> Result<Value, String> {
        let song_id = self.target_song(song_id)?;
        let pos = pos.unwrap_or_else(|| self.last_position.map(|p| p.0).unwrap_or(0.0));
        self.library.set_marker(song_id, slot, pos).err_str()?;
        self.push_markers_event(song_id);
        Ok(Value::Null)
    }

    fn marker_clear(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default)]
            song_id: Option<SongId>,
            slot: u32,
        }
        let p: P = from_params(p)?;
        let song_id = self.target_song(p.song_id)?;
        self.library.clear_marker(song_id, p.slot).err_str()?;
        self.push_markers_event(song_id);
        Ok(Value::Null)
    }

    fn marker_play(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default)]
            song_id: Option<SongId>,
            slot: u32,
        }
        let p: P = from_params(p)?;
        self.marker_play_slot(p.song_id, p.slot)?;
        Ok(Value::Null)
    }

    fn marker_play_slot(&mut self, song_id: Option<SongId>, slot: u32) -> Result<(), String> {
        let song_id = self.target_song(song_id)?;
        let m = self
            .library
            .marker(song_id, slot)
            .ok_or_else(|| format!("no marker in slot {slot}"))?;
        self.audio.send(EngineCmd::SeekSecs(m.pos));
        self.audio.send(EngineCmd::Play);
        Ok(())
    }
```

In `finish_open` (~line 2304), add to the payload `json!` right after `"isolation"`:

```rust
    "markers": self.library.list_markers(song_id),
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p server`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server
git commit -m "feat(server): marker.set/clear/play commands + markers in song.open"
```

---

### Task 3: Isolation snapshots model + gain resolution (practice crate)

**Files:**
- Modify: `crates/practice/src/model.rs` (snapshot struct + `Isolation::resolve_gains`)
- Modify: `crates/practice/src/bundle.rs` (`snapshots` field)
- Modify: `crates/practice/src/library.rs` (snapshot API)
- Read first: `apps/desktop/src/lib/isolation.ts` and `lib/isolation.test.ts`

- [ ] **Step 1: Read the frontend gain-fold semantics**

Run: `cd apps/desktop && pnpm vitest run lib/isolation.test.ts` and read `apps/desktop/src/lib/isolation.ts`. Confirm exactly how levels/mutes/solos fold into resolved gains (specifically: when any solo is set, are non-soloed stems zeroed, and does a stem that is both soloed and muted stay audible?). The Rust `resolve_gains` below assumes **solo-set-wins**: with any solo active, gain = level/100 for soloed stems and 0 for the rest (mutes ignored); with no solo, muted = 0, else level/100. If the TS differs, mirror the TS exactly and adjust the tests in Step 2 to match.

- [ ] **Step 2: Write failing tests**

In `model.rs`'s `isolation_tests` module:

```rust
    #[test]
    fn resolve_gains_no_solo_folds_mutes_and_levels() {
        let mut i = Isolation::default();
        i.levels[0] = 50;
        i.mutes[1] = true;
        let g = i.resolve_gains();
        assert_eq!(g[0], 0.5);
        assert_eq!(g[1], 0.0);
        assert_eq!(g[2], 1.0);
    }

    #[test]
    fn resolve_gains_solo_silences_everything_else() {
        let mut i = Isolation::default();
        i.solos[1] = true; // drums only
        i.levels[1] = 80;
        let g = i.resolve_gains();
        assert_eq!(g[1], 0.8_f32);
        for (idx, gain) in g.iter().enumerate() {
            if idx != 1 {
                assert_eq!(*gain, 0.0, "stem {idx} must be silent under solo");
            }
        }
    }
```

New module for the snapshot type:

```rust
#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[test]
    fn snapshot_round_trips() {
        let s = IsolationSnapshot {
            slot: 1,
            name: Some("drums only".into()),
            state: Isolation::default(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<IsolationSnapshot>(&json).unwrap(), s);
    }
}
```

Library test (same module/idiom as Task 1's marker test):

```rust
    #[test]
    fn snapshots_save_overwrite_clear_and_sort() {
        let (mut lib, song_id) = lib_with_song();
        lib.save_snapshot(song_id, 2, None, Isolation::default()).unwrap();
        lib.save_snapshot(song_id, 1, Some("full".into()), Isolation::default()).unwrap();
        assert_eq!(
            lib.list_snapshots(song_id).iter().map(|s| s.slot).collect::<Vec<_>>(),
            vec![1, 2]
        );
        let mut alt = Isolation::default();
        alt.mutes[0] = true;
        lib.save_snapshot(song_id, 2, None, alt.clone()).unwrap(); // overwrite
        assert_eq!(lib.snapshot(song_id, 2).unwrap().state, alt);
        assert_eq!(lib.list_snapshots(song_id).len(), 2);
        lib.clear_snapshot(song_id, 1).unwrap();
        assert!(lib.snapshot(song_id, 1).is_none());
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p practice snapshot resolve_gains`
Expected: compile errors.

- [ ] **Step 4: Implement**

`model.rs`, inside `impl Isolation`:

```rust
    /// Resolved per-stem gains (0.0..=1.0) with mute/solo folded in — the Rust
    /// mirror of the frontend's gain fold in `lib/isolation.ts`. Solo set wins:
    /// with any solo active only soloed stems are audible.
    pub fn resolve_gains(&self) -> [f32; STEM_COUNT] {
        let n = self.normalized();
        let any_solo = n.solos.iter().any(|s| *s);
        let mut out = [0.0; STEM_COUNT];
        for i in 0..STEM_COUNT {
            let audible = if any_solo { n.solos[i] } else { !n.mutes[i] };
            if audible {
                out[i] = f32::from(n.levels[i]) / 100.0;
            }
        }
        out
    }
```

After the `Marker` struct:

```rust
/// A saved isolation-box state under a slot number — what a pedal button's
/// "activate snapshot N" points at. Per song, in the bundle manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IsolationSnapshot {
    pub slot: u32,
    #[serde(default)]
    pub name: Option<String>,
    pub state: Isolation,
}
```

`bundle.rs`, on `BundleManifest` after `markers`:

```rust
    #[serde(default)]
    pub snapshots: Vec<IsolationSnapshot>,
```

`library.rs`, after the marker API (same patterns):

```rust
    // ── isolation snapshots ────────────────────────────────────────────────

    pub fn list_snapshots(&self, song_id: SongId) -> Vec<IsolationSnapshot> {
        self.entry(song_id)
            .map(|e| e.manifest.snapshots.clone())
            .unwrap_or_default()
    }

    pub fn snapshot(&self, song_id: SongId, slot: u32) -> Option<IsolationSnapshot> {
        self.entry(song_id)?
            .manifest
            .snapshots
            .iter()
            .cloned()
            .find(|s| s.slot == slot)
    }

    /// Save (or overwrite) the snapshot in `slot`. Kept sorted by slot.
    pub fn save_snapshot(
        &mut self,
        song_id: SongId,
        slot: u32,
        name: Option<String>,
        state: Isolation,
    ) -> Result<IsolationSnapshot> {
        let entry = self.entry_mut(song_id)?;
        let s = IsolationSnapshot { slot, name, state };
        match entry.manifest.snapshots.iter_mut().find(|x| x.slot == slot) {
            Some(x) => *x = s.clone(),
            None => {
                entry.manifest.snapshots.push(s.clone());
                entry.manifest.snapshots.sort_by_key(|x| x.slot);
            }
        }
        Self::persist(entry)?;
        Ok(s)
    }

    pub fn clear_snapshot(&mut self, song_id: SongId, slot: u32) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.snapshots.retain(|s| s.slot != slot);
        Self::persist(entry)
    }
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p practice`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/practice
git commit -m "feat(practice): isolation snapshots in the manifest + resolve_gains"
```

---

### Task 4: Snapshot commands + cycle cursor (server crate)

**Files:**
- Modify: `crates/server/src/app.rs` — `App` struct (~line 425), `App::new` (~line 536), dispatch table, handlers after the marker section, `finish_open`, tests

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod snapshot_cmd_tests {
    use super::*;
    use practice::model::Isolation;

    fn saved(app: &mut App, song_id: SongId, slot: u32, muted_stem: usize) {
        let mut iso = Isolation::default();
        iso.mutes[muted_stem] = true;
        app.library.save_snapshot(song_id, slot, None, iso).unwrap();
    }

    #[test]
    fn save_uses_provided_state_and_emits() {
        let (mut app, song_id) = app_with_song();
        let mut state = Isolation::default();
        state.levels[0] = 10;
        let resp = app.dispatch(req(
            "isolation.snapshot.save",
            json!({ "song_id": song_id, "slot": 1, "state": state }),
        ));
        assert!(resp.ok, "got: {:?}", resp.error);
        assert_eq!(app.library.snapshot(song_id, 1).unwrap().state.levels[0], 10);
        assert!(app.tick().iter().any(|e| e.event == "snapshots"));
    }

    #[test]
    fn activate_applies_mix_persists_isolation_and_emits() {
        let (mock, mut app) = make_shared_mock();
        let song_id = seed_song(&mut app);
        // Snapshot activation sends stem gains only when the open song has stems
        // (the apply_mix guard) — fake an open song with stems.
        app.open_song = Some(OpenSong {
            song: app.library.song_by_id(song_id).unwrap(),
            stems: true,
        });
        saved(&mut app, song_id, 2, 0); // vocals muted
        let resp = app.dispatch(req("isolation.snapshot.activate", json!({ "slot": 2 })));
        assert!(resp.ok, "got: {:?}", resp.error);
        // persisted as the live isolation state
        assert!(app.library.get_isolation(song_id).mutes[0]);
        // engine got the resolved gains: stem 0 muted -> gain 0
        let sent = &mock.lock().unwrap().sent;
        assert!(sent.iter().any(|c| matches!(c, EngineCmd::SetStemGain { idx: 0, gain } if *gain == 0.0)));
        // event carries the isolation + slot
        let events = app.tick();
        let ev = events.iter().find(|e| e.event == "isolation").expect("isolation event");
        assert_eq!(ev.data["slot"], 2);
    }

    #[test]
    fn cycle_walks_occupied_slots_and_wraps() {
        let (mut app, song_id) = app_with_song();
        app.open_song = Some(OpenSong {
            song: app.library.song_by_id(song_id).unwrap(),
            stems: false,
        });
        saved(&mut app, song_id, 1, 0);
        saved(&mut app, song_id, 4, 1);
        for expected in [1u32, 4, 1] {
            let resp = app.dispatch(req("isolation.snapshot.cycle", json!(null)));
            assert!(resp.ok, "got: {:?}", resp.error);
            assert_eq!(app.snapshot_cursor, Some(expected));
        }
    }

    #[test]
    fn cycle_with_no_snapshots_is_a_no_op() {
        let (mut app, song_id) = app_with_song();
        app.open_song = Some(OpenSong {
            song: app.library.song_by_id(song_id).unwrap(),
            stems: false,
        });
        let resp = app.dispatch(req("isolation.snapshot.cycle", json!(null)));
        assert!(resp.ok);
        assert_eq!(app.snapshot_cursor, None);
    }
}
```

(Adjust `song_by_id` usage to its actual signature — it may return a reference to clone.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p server snapshot`
Expected: FAIL — unknown command / missing field.

- [ ] **Step 3: Implement**

`App` struct — add next to `current_mix`:

```rust
    /// Slot of the last snapshot applied via activate/cycle — the cycle cursor.
    /// Transient: reset on song open, never persisted.
    snapshot_cursor: Option<u32>,
```

Init `snapshot_cursor: None` in `App::new`; in `finish_open`, next to the `current_mix = Mix::default()` reset, add `self.snapshot_cursor = None;` and add to the payload after `"markers"`:

```rust
    "snapshots": self.library.list_snapshots(song_id),
```

Dispatch entries:

```rust
            "isolation.snapshot.save" => self.snapshot_save(p),
            "isolation.snapshot.activate" => self.snapshot_activate(p),
            "isolation.snapshot.cycle" => self.snapshot_cycle().map(|()| Value::Null),
            "isolation.snapshot.clear" => self.snapshot_clear(p),
```

Handlers, after the marker section:

```rust
    // --- isolation snapshots -------------------------------------------------

    fn push_snapshots_event(&self, song_id: SongId) {
        let _ = self.job_tx.send(Event {
            event: "snapshots".into(),
            data: json!({ "song_id": song_id, "snapshots": self.library.list_snapshots(song_id) }),
        });
    }

    fn snapshot_save(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default)]
            song_id: Option<SongId>,
            slot: u32,
            #[serde(default)]
            name: Option<String>,
            /// The UI passes its live fader state; socket clients may omit it
            /// to snapshot the persisted isolation state instead.
            #[serde(default)]
            state: Option<practice::model::Isolation>,
        }
        let p: P = from_params(p)?;
        let song_id = self.target_song(p.song_id)?;
        let state = p.state.unwrap_or_else(|| self.library.get_isolation(song_id));
        self.library
            .save_snapshot(song_id, p.slot, p.name, state)
            .err_str()?;
        self.push_snapshots_event(song_id);
        Ok(Value::Null)
    }

    fn snapshot_clear(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default)]
            song_id: Option<SongId>,
            slot: u32,
        }
        let p: P = from_params(p)?;
        let song_id = self.target_song(p.song_id)?;
        self.library.clear_snapshot(song_id, p.slot).err_str()?;
        self.push_snapshots_event(song_id);
        Ok(Value::Null)
    }

    fn snapshot_activate(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default)]
            song_id: Option<SongId>,
            slot: u32,
        }
        let p: P = from_params(p)?;
        self.snapshot_activate_slot(p.song_id, p.slot)?;
        Ok(Value::Null)
    }

    fn snapshot_activate_slot(&mut self, song_id: Option<SongId>, slot: u32) -> Result<(), String> {
        let song_id = self.target_song(song_id)?;
        let snap = self
            .library
            .snapshot(song_id, slot)
            .ok_or_else(|| format!("no snapshot in slot {slot}"))?;
        self.apply_snapshot(song_id, slot, snap.state)
    }

    /// Persist a snapshot's state as the live isolation, drive the engine with
    /// its resolved mix, remember the cursor, and tell clients.
    fn apply_snapshot(
        &mut self,
        song_id: SongId,
        slot: u32,
        state: practice::model::Isolation,
    ) -> Result<(), String> {
        let state = state.normalized();
        self.library.set_isolation(song_id, state.clone()).err_str()?;
        self.apply_mix(Mix {
            bass_focus: state.bass_focus,
            stems: state.resolve_gains(),
        });
        self.snapshot_cursor = Some(slot);
        let _ = self.job_tx.send(Event {
            event: "isolation".into(),
            data: json!({ "song_id": song_id, "isolation": state, "slot": slot }),
        });
        Ok(())
    }

    /// Advance to the next occupied snapshot slot (wrapping). No snapshots →
    /// silent no-op, so a mapped pedal button on a fresh song does nothing.
    fn snapshot_cycle(&mut self) -> Result<(), String> {
        let song_id = self.target_song(None)?;
        let snaps = self.library.list_snapshots(song_id);
        let Some(first) = snaps.first().cloned() else {
            return Ok(());
        };
        let next = match self.snapshot_cursor {
            Some(cur) => snaps.iter().find(|s| s.slot > cur).cloned().unwrap_or(first),
            None => first,
        };
        self.apply_snapshot(song_id, next.slot, next.state)
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p server`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server
git commit -m "feat(server): isolation snapshot save/activate/cycle/clear commands"
```

---

### Task 5: Pedal mapping + trigger execution (server crate)

**Files:**
- Create: `crates/server/src/pedal.rs`
- Modify: `crates/server/src/lib.rs` (add `pub mod pedal;`)
- Modify: `crates/server/src/app.rs` — `current_loop` tracking, `run_trigger`, `pedal.trigger` command, tests

- [ ] **Step 1: Write failing pedal.rs tests**

`crates/server/src/pedal.rs`:

```rust
//! The global pedal mapping: rows of `{trigger, action, slot?}` stored as JSON
//! in the `pedal_mapping` setting. Parsing is total — malformed rows are
//! skipped, malformed JSON yields an empty mapping.

use serde::Deserialize;

pub const PEDAL_MAPPING_KEY: &str = "pedal_mapping";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Binding {
    pub trigger: String,
    pub action: String,
    #[serde(default)]
    pub slot: Option<u32>,
}

pub fn parse_mapping(v: &serde_json::Value) -> Vec<Binding> {
    v.as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| serde_json::from_value(r.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_rows_and_skips_malformed() {
        let v = json!([
            { "trigger": "pc:0:0", "action": "play_pause" },
            { "trigger": "pc:0:1", "action": "play_marker", "slot": 2 },
            { "nope": true },
        ]);
        let m = parse_mapping(&v);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].action, "play_pause");
        assert_eq!(m[1].slot, Some(2));
    }

    #[test]
    fn non_array_is_empty() {
        assert!(parse_mapping(&json!("garbage")).is_empty());
        assert!(parse_mapping(&json!(null)).is_empty());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p server pedal`
Expected: FAIL to compile until `pub mod pedal;` is added to `crates/server/src/lib.rs` — add it (alphabetical position, after `protocol`), then the pedal.rs tests pass.

Run again: `cargo test -p server pedal` → PASS.

- [ ] **Step 3: Write failing run_trigger tests**

In `app.rs`:

```rust
#[cfg(test)]
mod pedal_trigger_tests {
    use super::*;

    fn with_mapping(app: &mut App, mapping: Value) {
        app.store
            .set_setting(crate::pedal::PEDAL_MAPPING_KEY, &mapping)
            .unwrap();
    }

    #[test]
    fn play_pause_toggles_on_transport_state() {
        let (mock, mut app) = make_shared_mock();
        with_mapping(&mut app, json!([{ "trigger": "pc:0:0", "action": "play_pause" }]));
        app.last_position = Some((0.0, 1.0, false, None)); // paused
        let resp = app.dispatch(req("pedal.trigger", json!({ "trigger": "pc:0:0" })));
        assert!(resp.ok, "got: {:?}", resp.error);
        assert!(mock.lock().unwrap().sent.iter().any(|c| matches!(c, EngineCmd::Play)));

        app.last_position = Some((5.0, 1.0, true, None)); // playing
        app.dispatch(req("pedal.trigger", json!({ "trigger": "pc:0:0" })));
        assert!(mock.lock().unwrap().sent.iter().any(|c| matches!(c, EngineCmd::Pause)));
    }

    #[test]
    fn restart_loop_seeks_to_tracked_loop_start() {
        let (mock, mut app) = make_shared_mock();
        with_mapping(&mut app, json!([{ "trigger": "pc:0:1", "action": "restart_loop" }]));
        app.dispatch(req("loop.set", json!({ "start": 8.0, "end": 16.0 })));
        app.dispatch(req("pedal.trigger", json!({ "trigger": "pc:0:1" })));
        assert!(mock.lock().unwrap().sent.iter()
            .any(|c| matches!(c, EngineCmd::SeekSecs(s) if *s == 8.0)));
        // loop.clear drops the tracked region -> restart seeks 0
        app.dispatch(req("loop.clear", json!(null)));
        mock.lock().unwrap().sent.clear();
        app.dispatch(req("pedal.trigger", json!({ "trigger": "pc:0:1" })));
        assert!(mock.lock().unwrap().sent.iter()
            .any(|c| matches!(c, EngineCmd::SeekSecs(s) if *s == 0.0)));
    }

    #[test]
    fn slot_actions_route_to_their_commands() {
        let (mock, mut app) = make_shared_mock();
        let song_id = seed_song(&mut app);
        app.open_song = Some(OpenSong {
            song: app.library.song_by_id(song_id).unwrap(),
            stems: false,
        });
        app.library.set_marker(song_id, 3, 30.0).unwrap();
        with_mapping(
            &mut app,
            json!([{ "trigger": "pc:0:2", "action": "play_marker", "slot": 3 }]),
        );
        let resp = app.dispatch(req("pedal.trigger", json!({ "trigger": "pc:0:2" })));
        assert!(resp.ok, "got: {:?}", resp.error);
        assert!(mock.lock().unwrap().sent.iter()
            .any(|c| matches!(c, EngineCmd::SeekSecs(s) if *s == 30.0)));
    }

    #[test]
    fn unmapped_trigger_is_a_no_op() {
        let (_, mut app) = make_shared_mock();
        let resp = app.dispatch(req("pedal.trigger", json!({ "trigger": "pc:0:9" })));
        assert!(resp.ok, "unmapped triggers must not error: {:?}", resp.error);
    }
}
```

- [ ] **Step 4: Run to verify failure**

Run: `cargo test -p server pedal_trigger`
Expected: FAIL — `unknown command: pedal.trigger`.

- [ ] **Step 5: Implement current_loop tracking + run_trigger**

`App` struct, next to `snapshot_cursor`:

```rust
    /// Engine loop region as last commanded (loop.set / routine block), for the
    /// pedal's restart action. The engine owns the truth; this mirrors it.
    current_loop: Option<(f64, f64)>,
```

Init `current_loop: None` in `App::new`. Updates:
- `loop_set` (~line 860): before `send_ok`, add `self.current_loop = Some((p.start, p.end));`
- Dispatch entry `"loop.clear"` becomes:

```rust
            "loop.clear" => {
                self.current_loop = None;
                self.send_ok(EngineCmd::ClearLoop)
            }
```

- `apply_block` (~line 2810): after computing `start`, add `self.current_loop = Some((start, block.span.end));`
- `finish_open`: add `self.current_loop = None;` next to the other resets.

Dispatch entry:

```rust
            "pedal.trigger" => self.pedal_trigger(p),
```

Handlers, new `// --- pedal ---` section:

```rust
    // --- pedal --------------------------------------------------------------

    fn pedal_trigger(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            trigger: String,
        }
        let p: P = from_params(p)?;
        self.run_trigger(&p.trigger)?;
        Ok(Value::Null)
    }

    /// Look a normalized MIDI trigger up in the pedal mapping and run its
    /// action. Unmapped triggers are fine (Ok); a mapped action that can't run
    /// (no marker in slot, no song open) is an error.
    fn run_trigger(&mut self, trigger: &str) -> Result<(), String> {
        let mapping = self
            .store
            .get_setting(crate::pedal::PEDAL_MAPPING_KEY)
            .ok()
            .flatten()
            .map(|v| crate::pedal::parse_mapping(&v))
            .unwrap_or_default();
        let Some(b) = mapping.into_iter().find(|b| b.trigger == trigger) else {
            return Ok(());
        };
        let slot = b
            .slot
            .ok_or_else(|| format!("pedal action {} needs a slot", b.action));
        match b.action.as_str() {
            "play_pause" => {
                let playing = self.last_position.map(|p| p.2).unwrap_or(false);
                self.audio
                    .send(if playing { EngineCmd::Pause } else { EngineCmd::Play });
                Ok(())
            }
            "restart_loop" => {
                let start = self.current_loop.map(|(s, _)| s).unwrap_or(0.0);
                self.audio.send(EngineCmd::SeekSecs(start));
                Ok(())
            }
            "play_marker" => self.marker_play_slot(None, slot?),
            "set_marker" => self.marker_set_slot(None, slot?, None).map(|_| ()),
            "activate_snapshot" => self.snapshot_activate_slot(None, slot?),
            "cycle_snapshots" => self.snapshot_cycle(),
            other => Err(format!("unknown pedal action: {other}")),
        }
    }
```

- [ ] **Step 6: Run to verify pass**

Run: `cargo test -p server`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/server
git commit -m "feat(server): pedal mapping, run_trigger, pedal.trigger command"
```

---

### Task 6: MIDI listener (server crate + both binaries)

**Files:**
- Create: `crates/server/src/midi.rs`
- Modify: `crates/server/src/lib.rs` (`pub mod midi;`)
- Modify: `crates/server/Cargo.toml` (add `midir = "0.10"`)
- Modify: `crates/server/src/app.rs` — midi channel fields, tick drain, `start_midi`, `midi.status`
- Modify: `crates/server/src/bin/dredged.rs:66-71`, `apps/desktop/src-tauri/src/main.rs:53-58`

- [ ] **Step 1: Write failing normalize tests**

`crates/server/src/midi.rs` (tests first; the listener comes in step 3):

```rust
//! MIDI input → normalized trigger strings. A background thread rescans ALSA
//! MIDI sources every few seconds and auto-connects to everything except
//! `Midi Through`, so the pedal works over USB or BLE-MIDI, hotplug included.
//! Raw messages normalize to compact trigger keys (`pc:0:0`) that the pedal
//! mapping is keyed by; everything else about the device stays out of the app.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_change_normalizes() {
        assert_eq!(normalize(&[0xC0, 2]), Some("pc:0:2".into()));
        assert_eq!(normalize(&[0xC5, 0]), Some("pc:5:0".into()));
    }

    #[test]
    fn note_on_normalizes_note_off_ignored() {
        assert_eq!(normalize(&[0x90, 60, 100]), Some("note:0:60".into()));
        assert_eq!(normalize(&[0x90, 60, 0]), None); // running-status note-off
        assert_eq!(normalize(&[0x80, 60, 64]), None);
    }

    #[test]
    fn cc_normalizes_press_release_on_value() {
        assert_eq!(normalize(&[0xB0, 64, 127]), Some("cc:0:64:press".into()));
        assert_eq!(normalize(&[0xB0, 64, 0]), Some("cc:0:64:release".into()));
    }

    #[test]
    fn junk_is_none() {
        assert_eq!(normalize(&[]), None);
        assert_eq!(normalize(&[0xF8]), None); // clock
        assert_eq!(normalize(&[0xC0]), None); // truncated
    }
}
```

- [ ] **Step 2: Run to verify failure**

Add `pub mod midi;` to `crates/server/src/lib.rs` first, then:
Run: `cargo test -p server midi`
Expected: FAIL — `normalize` not found.

- [ ] **Step 3: Implement normalize + listener**

Add to `crates/server/Cargo.toml` dependencies: `midir = "0.10"`. (midir's Linux backend uses alsa-lib; it's present on any PipeWire system, and CI already builds PipeWire.)

`midi.rs` implementation above the tests:

```rust
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RESCAN: Duration = Duration::from_secs(2);

/// Normalize a raw MIDI message to a trigger key, or None for messages the
/// pedal mapping doesn't speak (note-off, clock, sysex, truncated).
pub fn normalize(msg: &[u8]) -> Option<String> {
    let status = *msg.first()?;
    let ch = status & 0x0F;
    match status & 0xF0 {
        0xC0 => Some(format!("pc:{ch}:{}", msg.get(1)?)),
        0x90 if *msg.get(2)? > 0 => Some(format!("note:{ch}:{}", msg.get(1)?)),
        0xB0 => {
            let num = *msg.get(1)?;
            let val = *msg.get(2)?;
            let edge = if val >= 64 { "press" } else { "release" };
            Some(format!("cc:{ch}:{num}:{edge}"))
        }
        _ => None,
    }
}

/// Names of the currently connected MIDI sources, shared with the listener
/// thread. `App` reads it for the `midi.status` command.
#[derive(Clone, Default)]
pub struct MidiStatus(Arc<Mutex<Vec<String>>>);

impl MidiStatus {
    pub fn devices(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

/// Spawn the listener thread: rescan every `RESCAN`, connect to every MIDI
/// source except `Midi Through`, send each normalized trigger down `tx`.
/// Connections to vanished ports are dropped on the next rescan.
pub fn spawn(tx: mpsc::Sender<String>) -> MidiStatus {
    let status = MidiStatus::default();
    let shared = status.clone();
    std::thread::Builder::new()
        .name("midi-listen".into())
        .spawn(move || {
            let mut conns: HashMap<String, midir::MidiInputConnection<()>> = HashMap::new();
            loop {
                if let Ok(probe) = midir::MidiInput::new("dredge-probe") {
                    let ports: Vec<(String, midir::MidiInputPort)> = probe
                        .ports()
                        .into_iter()
                        .filter_map(|p| probe.port_name(&p).ok().map(|n| (n, p)))
                        .filter(|(n, _)| !n.contains("Midi Through"))
                        .collect();
                    conns.retain(|name, _| ports.iter().any(|(n, _)| n == name));
                    for (name, port) in ports {
                        if conns.contains_key(&name) {
                            continue;
                        }
                        let Ok(input) = midir::MidiInput::new("dredge") else {
                            continue;
                        };
                        let tx = tx.clone();
                        if let Ok(conn) = input.connect(
                            &port,
                            "dredge-in",
                            move |_, msg, ()| {
                                if let Some(t) = normalize(msg) {
                                    let _ = tx.send(t);
                                }
                            },
                            (),
                        ) {
                            conns.insert(name, conn);
                        }
                    }
                    *shared.0.lock().unwrap() = conns.keys().cloned().collect();
                }
                std::thread::sleep(RESCAN);
            }
        })
        .expect("spawn midi listener");
    status
}
```

(If midir's connect callback signature differs in the resolved version — first arg is a `u64` timestamp — adjust the closure to `move |_ts, msg, _data|`. `cargo check -p server` will tell you.)

- [ ] **Step 4: Wire into App**

`App` struct, next to the tuner channels:

```rust
    /// Normalized pedal triggers from the MIDI listener; drained by `tick()`.
    midi_tx: mpsc::Sender<String>,
    midi_rx: mpsc::Receiver<String>,
    /// Connected MIDI source names (None until `start_midi`).
    midi_status: Option<crate::midi::MidiStatus>,
```

`App::new`: `let (midi_tx, midi_rx) = mpsc::channel();` and init the three fields (`midi_status: None`).

Public starter (production only — tests never call it, so no threads/ALSA in CI):

```rust
    /// Start the MIDI listener thread. Called once by each binary after
    /// construction; tests skip it and push triggers through `pedal.trigger`.
    pub fn start_midi(&mut self) {
        self.midi_status = Some(crate::midi::spawn(self.midi_tx.clone()));
    }
```

In `tick()`, after the monitor drain and before the engine-event loop:

```rust
        // Pedal triggers: broadcast for the UI's learn flow, then run the
        // mapped action. Failures log rather than surface — there is no
        // requester to answer.
        while let Ok(trigger) = self.midi_rx.try_recv() {
            events.push(Event {
                event: "midi".into(),
                data: json!({ "trigger": trigger }),
            });
            if let Err(e) = self.run_trigger(&trigger) {
                eprintln!("dredge: pedal trigger {trigger}: {e}");
            }
        }
```

Dispatch entry:

```rust
            "midi.status" => Ok(json!({
                "devices": self.midi_status.as_ref().map(|s| s.devices()).unwrap_or_default(),
            })),
```

- [ ] **Step 5: Start it in both binaries**

`crates/server/src/bin/dredged.rs` — after the `App::new` block (line 66-70):

```rust
    app.lock().unwrap().start_midi();
```

`apps/desktop/src-tauri/src/main.rs` — after the `App::new` block (line 53-57):

```rust
    app.lock().unwrap().start_midi();
```

- [ ] **Step 6: Add a tick test and run everything**

In `app.rs` (any pedal/midi test module):

```rust
    #[test]
    fn tick_broadcasts_and_runs_midi_triggers() {
        let (mock, mut app) = make_shared_mock();
        app.store
            .set_setting(
                crate::pedal::PEDAL_MAPPING_KEY,
                &json!([{ "trigger": "pc:0:0", "action": "play_pause" }]),
            )
            .unwrap();
        app.midi_tx.send("pc:0:0".into()).unwrap();
        let events = app.tick();
        assert!(events.iter().any(|e| e.event == "midi" && e.data["trigger"] == "pc:0:0"));
        assert!(mock.lock().unwrap().sent.iter().any(|c| matches!(c, EngineCmd::Play)));
    }
```

Run: `cargo test -p server && cargo build -p server && cargo check -p dredge 2>/dev/null || (cd apps/desktop/src-tauri && cargo check)`
Expected: all pass; both binaries compile.

- [ ] **Step 7: Commit**

```bash
git add crates/server apps/desktop/src-tauri Cargo.lock
git commit -m "feat(server): MIDI listener thread, trigger normalization, midi.status"
```

---

### Task 7: Frontend stores — types, events, actions

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts` — `OpenSong` (~line 166), settings keys (~line 473), new stores (~line 337), `actions` (isolation section ~line 1271 and a new pedal/marker section), `initEvents` switch (~line 1656), `persistIsolation` (~line 1287)

- [ ] **Step 1: Add types + stores + settings key**

Types near `OpenSong`:

```ts
export interface Marker {
  slot: number;
  pos: number;
}

export interface IsolationSnapshot {
  slot: number;
  name: string | null;
  state: Isolation;
}

/** One row of the global pedal mapping (the `pedal_mapping` setting). */
export interface PedalBinding {
  trigger: string;
  action: string;
  slot?: number;
}
```

(`Isolation` is the existing wire type used by `OpenSong.isolation` — import/reference the same one.)

`OpenSong` gains:

```ts
  markers: Marker[];
  snapshots: IsolationSnapshot[];
```

Settings key constant next to the others (~line 479):

```ts
export const PEDAL_MAPPING = "pedal_mapping";
```

New stores near `workspaceReset` (~line 337):

```ts
/** Snapshot slot last applied server-side; cleared by any manual fader edit. */
export const activeSnapshotSlot = writable<number | null>(null);

/** Last normalized MIDI trigger seen (learn flow); `seq` forces reactivity on
 * repeated identical triggers. */
export const lastMidiTrigger = writable<{ trigger: string; seq: number } | null>(null);
```

- [ ] **Step 2: Event handlers**

Module-level counter near `initEvents`:

```ts
let midiSeq = 0;
```

In the `initEvents` switch, after the existing cases:

```ts
      case "markers": {
        const d = ev.data as { song_id: number; markers: Marker[] };
        openSong.update((o) => (o && o.song.id === d.song_id ? { ...o, markers: d.markers } : o));
        break;
      }
      case "snapshots": {
        const d = ev.data as { song_id: number; snapshots: IsolationSnapshot[] };
        openSong.update((o) =>
          o && o.song.id === d.song_id ? { ...o, snapshots: d.snapshots } : o,
        );
        break;
      }
      case "isolation": {
        // A snapshot was applied server-side: the engine already has the mix;
        // mirror it into the UI stores and keep the bass-focus/pitch-octave
        // coupling (matching the song.open restore path).
        const d = ev.data as { song_id: number; isolation: Isolation; slot: number | null };
        const open = get(openSong);
        if (!open || open.song.id !== d.song_id) break;
        stemMix.set(isolationToStemMix(d.isolation));
        bassFocus.set(d.isolation.bass_focus);
        activeSnapshotSlot.set(d.slot);
        const p = get(pitch);
        if (p.octaveUp !== d.isolation.bass_focus) {
          pitch.set({ ...p, octaveUp: d.isolation.bass_focus });
          void cmd("pitch", {
            semitones: p.semitones,
            cents: p.cents,
            octave_up: d.isolation.bass_focus,
          });
        }
        break;
      }
      case "midi": {
        midiSeq += 1;
        lastMidiTrigger.set({ trigger: (ev.data as { trigger: string }).trigger, seq: midiSeq });
        break;
      }
```

(Mirror the exact store/helper names from the `openSong` restore at stores.ts:763-801 — `isolationToStemMix`, `stemMix`, `bassFocus`, `pitch` are all already imported/defined there. Match the pitch-coupling of `actions.bassFocus` (stores.ts:897) if it differs from the openSong restore.)

- [ ] **Step 3: Actions + snapshot-highlight invalidation**

In `persistIsolation` (~line 1287), first line of the body:

```ts
      activeSnapshotSlot.set(null);
```

New actions in the isolation section (after `captureBlock`, ~line 1333):

```ts
    /** The live isolation-box state in wire shape (what snapshot.save stores). */
    captureIsolation(): Isolation {
      const m = get(stemMix);
      return { bass_focus: get(bassFocus), levels: m.levels, mutes: m.mutes, solos: m.solos };
    },
    saveSnapshot: (slot: number) =>
      cmd("isolation.snapshot.save", { slot, state: actions.captureIsolation() }),
    activateSnapshot: (slot: number) => cmd("isolation.snapshot.activate", { slot }),
    clearSnapshot: (slot: number) => cmd("isolation.snapshot.clear", { slot }),
    cycleSnapshots: () => cmd("isolation.snapshot.cycle"),

    // --- markers / pedal ---
    setMarker: (slot: number) => cmd("marker.set", { slot }), // pos defaults to playhead
    clearMarker: (slot: number) => cmd("marker.clear", { slot }),
    playMarker: (slot: number) => cmd("marker.play", { slot }),
    setPedalMapping(rows: PedalBinding[]): Promise<void> {
      return this.setSetting(PEDAL_MAPPING, rows);
    },
```

(If `stemMix`'s shape uses different field names than `levels/mutes/solos`, mirror what `isolationToStemMix` produces — `captureIsolation` must be its inverse.)

- [ ] **Step 4: Check + run existing tests**

Run: `cd apps/desktop && pnpm vitest run && pnpm exec svelte-check --tsconfig ./tsconfig.json 2>&1 | tail -5`
Expected: vitest green; svelte-check no new errors. (If svelte-check is normally run via `just lint`, use that instead.)

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src
git commit -m "feat(ui): marker/snapshot/pedal stores, actions, and event handling"
```

---

### Task 8: Waveform marker pips

**Files:**
- Modify: `apps/desktop/src/lib/waveform-hit.ts` (+ its test file `waveform-hit.test.ts`)
- Modify: `apps/desktop/src/components/Waveform.svelte` — draw pass (~line 462, before the playhead pass), pointer-up seek (~line 863)

- [ ] **Step 1: Write failing hit-test test**

In `apps/desktop/src/lib/waveform-hit.test.ts` (follow the file's existing fixtures — `View` objects are `{ startSec, endSec, width }`):

```ts
describe("hitMarkerPip", () => {
  const view = { startSec: 0, endSec: 100, width: 1000 };
  const markers = [
    { slot: 1, pos: 10 },
    { slot: 2, pos: 50 },
  ];

  it("hits a pip within its box", () => {
    // marker 2 at x=500; pip box extends right from the stem
    expect(hitMarkerPip(view, markers, 503, 24 + 4, 24)?.slot).toBe(2);
  });

  it("misses outside the pip band", () => {
    expect(hitMarkerPip(view, markers, 503, 80, 24)).toBeNull();
  });

  it("misses horizontally", () => {
    expect(hitMarkerPip(view, markers, 300, 24 + 4, 24)).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd apps/desktop && pnpm vitest run lib/waveform-hit.test.ts`
Expected: FAIL — `hitMarkerPip` not exported.

- [ ] **Step 3: Implement hit test**

In `lib/waveform-hit.ts` (reuse the module's existing `View` type import; add):

```ts
/** Marker pip box: a numbered flag hanging right of the marker stem. */
export const MARKER_PIP_W = 12;
export const MARKER_PIP_H = 14;

export function hitMarkerPip(
  view: View,
  markers: { slot: number; pos: number }[],
  x: number,
  y: number,
  laneTop: number,
): { slot: number; pos: number } | null {
  if (y < laneTop || y > laneTop + MARKER_PIP_H) return null;
  for (const m of markers) {
    const mx = secToX(view, m.pos);
    if (x >= mx - 2 && x <= mx + MARKER_PIP_W) return m;
  }
  return null;
}
```

Run: `pnpm vitest run lib/waveform-hit.test.ts` → PASS.

- [ ] **Step 4: Draw pass + click-to-seek**

In `Waveform.svelte`:

Reactive markers near the other `$openSong` reads:

```ts
  const markers = $derived($openSong?.markers ?? []);
```

Draw pass inserted immediately **before** the playhead pass (~line 462), using the same colors object `c` the playhead uses (pick its accent constant from `lib/waveform-colors.ts` — the one the count-in glow uses):

```ts
    // --- marker pips: numbered per-song positions, under the playhead
    for (const m of markers) {
      const x = secToX(view, m.pos);
      if (x < -MARKER_PIP_W || x > view.width + MARKER_PIP_W) continue;
      ctx.fillStyle = c.accent;
      ctx.fillRect(x, LANE_H, 1, MARKER_PIP_H); // stem
      ctx.fillRect(x, LANE_H, MARKER_PIP_W, 10); // flag
      ctx.fillStyle = c.bg;
      ctx.font = "8px sans-serif";
      ctx.textBaseline = "top";
      ctx.fillText(String(m.slot), x + 3, LANE_H + 1);
    }
```

(Import `MARKER_PIP_W`/`MARKER_PIP_H`/`hitMarkerPip` from `lib/waveform-hit.ts`. If `c` has no `accent`, use the exact color constant the count-in playhead glow uses in this file.)

In `onPointerUp`'s non-moved plain-click branch (~line 863), before the generic `placePlayhead(...)` call — using the same local x/y the branch already computes:

```ts
      const pip = hitMarkerPip(view, markers, cx, cy, LANE_H);
      if (pip) {
        void placePlayhead(pip.pos);
        return;
      }
```

(If the branch has no `cy`, derive it the same way `cx` is derived from the pointer event.)

- [ ] **Step 5: Check + commit**

Run: `cd apps/desktop && pnpm vitest run && cd ../.. && just lint`
Expected: green.

```bash
git add apps/desktop/src
git commit -m "feat(ui): numbered marker pips on the waveform, click to seek"
```

---

### Task 9: Snapshot chips in the isolation control box

**Files:**
- Modify: `apps/desktop/src/lib/isolation.ts` (+ test) — `nextFreeSlot`
- Modify: `apps/desktop/src/components/Isolation.svelte` (~line 99, after the `.channels` block)

- [ ] **Step 1: Failing test for nextFreeSlot**

In `apps/desktop/src/lib/isolation.test.ts`:

```ts
describe("nextFreeSlot", () => {
  it("starts at 1", () => {
    expect(nextFreeSlot([])).toBe(1);
  });
  it("fills gaps first", () => {
    expect(nextFreeSlot([{ slot: 1 }, { slot: 3 }])).toBe(2);
  });
  it("appends past the end", () => {
    expect(nextFreeSlot([{ slot: 1 }, { slot: 2 }])).toBe(3);
  });
});
```

Run: `pnpm vitest run lib/isolation.test.ts` → FAIL.

- [ ] **Step 2: Implement**

In `lib/isolation.ts`:

```ts
/** Lowest snapshot slot number not yet in use (1-based). */
export function nextFreeSlot(snaps: { slot: number }[]): number {
  const used = new Set(snaps.map((s) => s.slot));
  let slot = 1;
  while (used.has(slot)) slot += 1;
  return slot;
}
```

Run: `pnpm vitest run lib/isolation.test.ts` → PASS.

- [ ] **Step 3: Chips row**

In `Isolation.svelte`, imports: add `actions` helpers already imported; add `activeSnapshotSlot` from stores, `nextFreeSlot` from `$lib/isolation` (match the file's import style). After the `{#if hasStems}` `.channels` block closes (~line 99), still inside the stems branch:

```svelte
    <div class="rule"></div>
    <div class="snapshots">
      {#each $openSong.snapshots as s (s.slot)}
        <Button
          variant="chip"
          active={$activeSnapshotSlot === s.slot}
          title={s.name ?? `snapshot ${s.slot} — right-click to clear`}
          onclick={() => void actions.activateSnapshot(s.slot)}
          oncontextmenu={(e: MouseEvent) => {
            e.preventDefault();
            void actions.clearSnapshot(s.slot);
          }}>{s.slot}</Button>
      {/each}
      <Button
        variant="chip"
        title="save current state as a snapshot"
        onclick={() => void actions.saveSnapshot(nextFreeSlot($openSong.snapshots))}>+</Button>
    </div>
```

Style `.snapshots` alongside the existing rows (horizontal flex, small gap — match the `.focus` row's spacing). If the `Button` chip variant doesn't forward `oncontextmenu`, wrap the chip in a plain `<span oncontextmenu=...>` or extend the widget the way other forwarded handlers are done — follow the widget's existing prop pattern.

The active chip uses the widget's `active` accent state (theme accent — no hardcoded colors).

- [ ] **Step 4: Check + commit**

Run: `cd apps/desktop && pnpm vitest run && cd ../.. && just lint`
Expected: green.

```bash
git add apps/desktop/src
git commit -m "feat(ui): isolation snapshot chips — save, activate, clear"
```

---

### Task 10: Pedal dock tab

**Files:**
- Create: `apps/desktop/src/components/Pedal.svelte`
- Modify: `apps/desktop/src/lib/stores.ts:431` (`ALL_TABS` — insert `"pedal"` after `"devices"`)
- Modify: `apps/desktop/src/App.svelte:44-54` (`TAB_VIEWS` — add `pedal: Pedal` + import)

- [ ] **Step 1: Register the tab**

`stores.ts:431`:

```ts
export const ALL_TABS = ["library", "structure", "loops", "routines", "export", "profile", "devices", "pedal", "settings", "guide"] as const;
```

`App.svelte`: `import Pedal from "./components/Pedal.svelte";` and add `pedal: Pedal,` to `TAB_VIEWS`. (Existing saved workspaces auto-gain the tab via `reconcileWorkspace` — no migration needed.)

- [ ] **Step 2: Build the component**

`components/Pedal.svelte`, modeled on `Devices.svelte` (same `<section class="group">` + `SectionHead` + `Button` idiom, `asyncAction` for error surfacing). Full component:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { cmd } from "../lib/ipc";
  import {
    actions,
    lastMidiTrigger,
    openSong,
    settings,
    PEDAL_MAPPING,
    type PedalBinding,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import SectionHead from "../lib/ui/SectionHead.svelte";

  const ACTIONS: { id: string; label: string; slot: boolean }[] = [
    { id: "play_pause", label: "play / pause", slot: false },
    { id: "restart_loop", label: "restart loop", slot: false },
    { id: "play_marker", label: "play from marker", slot: true },
    { id: "set_marker", label: "set marker", slot: true },
    { id: "activate_snapshot", label: "activate snapshot", slot: true },
    { id: "cycle_snapshots", label: "cycle snapshots", slot: false },
  ];
  const MARKER_SLOTS = [1, 2, 3, 4, 5, 6];

  let devices = $state<string[]>([]);
  let learning = $state<number | null>(null);
  let learnSeq = 0;

  const rows = $derived(($settings[PEDAL_MAPPING] as PedalBinding[] | undefined) ?? []);
  const markers = $derived($openSong?.markers ?? []);

  async function refreshDevices(): Promise<void> {
    const r = await cmd<{ devices: string[] }>("midi.status");
    devices = r.devices;
  }

  onMount(() => {
    void refreshDevices();
    const t = setInterval(() => void refreshDevices(), 5000);
    return () => clearInterval(t);
  });

  // Learn flow: while a row is armed, the next midi event fills its trigger.
  $effect(() => {
    const ev = $lastMidiTrigger;
    if (learning === null || !ev || ev.seq <= learnSeq) return;
    const next = rows.map((r, i) => (i === learning ? { ...r, trigger: ev.trigger } : r));
    learning = null;
    void actions.setPedalMapping(next);
  });

  function arm(i: number): void {
    learnSeq = $lastMidiTrigger?.seq ?? 0;
    learning = learning === i ? null : i;
  }

  function setRow(i: number, patch: Partial<PedalBinding>): void {
    void actions.setPedalMapping(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  }

  function addRow(): void {
    void actions.setPedalMapping([...rows, { trigger: "", action: "play_pause" }]);
  }

  function removeRow(i: number): void {
    void actions.setPedalMapping(rows.filter((_, idx) => idx !== i));
  }

  function needsSlot(action: string): boolean {
    return ACTIONS.find((a) => a.id === action)?.slot ?? false;
  }

  function markerAt(slot: number): number | null {
    return markers.find((m) => m.slot === slot)?.pos ?? null;
  }

  function fmt(pos: number): string {
    const m = Math.floor(pos / 60);
    const s = pos - m * 60;
    return `${m}:${s.toFixed(1).padStart(4, "0")}`;
  }
</script>

<section class="group">
  <SectionHead>device</SectionHead>
  {#if devices.length}
    {#each devices as d}<p class="dev">{d}</p>{/each}
  {:else}
    <p class="dev none">no MIDI device</p>
  {/if}
</section>

<section class="group">
  <SectionHead>mapping</SectionHead>
  {#each rows as row, i}
    <div class="row">
      <Button
        variant="chip"
        active={learning === i}
        title="press a pedal button to assign"
        onclick={() => arm(i)}>{learning === i ? "…" : row.trigger || "learn"}</Button>
      <select
        value={row.action}
        onchange={(e) => setRow(i, { action: (e.target as HTMLSelectElement).value })}>
        {#each ACTIONS as a}<option value={a.id}>{a.label}</option>{/each}
      </select>
      {#if needsSlot(row.action)}
        <input
          class="slot"
          type="number"
          min="1"
          value={row.slot ?? 1}
          onchange={(e) => setRow(i, { slot: Number((e.target as HTMLInputElement).value) })} />
      {/if}
      <Button variant="chip" title="remove" onclick={() => removeRow(i)}>×</Button>
    </div>
  {/each}
  <Button onclick={addRow}>add binding</Button>
</section>

{#if $openSong}
  <section class="group">
    <SectionHead>markers</SectionHead>
    {#each MARKER_SLOTS as slot}
      <div class="row">
        <span class="mslot">{slot}</span>
        <span class="mtime">{markerAt(slot) !== null ? fmt(markerAt(slot)!) : "—"}</span>
        <Button variant="chip" title="set from playhead" onclick={() => void actions.setMarker(slot)}>set</Button>
        {#if markerAt(slot) !== null}
          <Button variant="chip" onclick={() => void actions.playMarker(slot)}>play</Button>
          <Button variant="chip" onclick={() => void actions.clearMarker(slot)}>×</Button>
        {/if}
      </div>
    {/each}
  </section>
{/if}

<style>
  .group { display: flex; flex-direction: column; gap: 6px; margin-bottom: 14px; }
  .row { display: flex; align-items: center; gap: 6px; }
  .dev { margin: 0; font-size: 12px; }
  .dev.none { opacity: 0.5; }
  .slot { width: 3.5em; }
  .mslot { width: 1.5em; opacity: 0.7; font-size: 12px; }
  .mtime { width: 4.5em; font-variant-numeric: tabular-nums; font-size: 12px; }
</style>
```

Adapt to the real widget kit as you go: if a `NumberField` widget fits better than the raw `<input type="number">`, use it; match `Devices.svelte`'s classes/`SectionHead` usage and the existing `select` styling used elsewhere (check `SettingsPanel`). If dredge has a shared time formatter in `lib/format.ts`, use it instead of the local `fmt`. Keep copy quiet (no explainer text — repo convention).

- [ ] **Step 3: Check + commit**

Run: `cd apps/desktop && pnpm vitest run && cd ../.. && just lint`
Expected: green.

```bash
git add apps/desktop/src
git commit -m "feat(ui): pedal tab — MIDI mapping editor with learn flow + markers row"
```

---

### Task 11: Full gate + build + runtime verification

- [ ] **Step 1: Full test + lint gate**

Run: `just check`
Expected: cargo tests, vitest, clippy, fmt, svelte-check all green. Fix anything that surfaces.

- [ ] **Step 2: Release build**

Run: `just build`
Expected: builds `target/release/dredge` and `target/release/dredged`. (Repo rule: the desktop launcher runs this release binary — always rebuild after committing.)

- [ ] **Step 3: Headless end-to-end against the socket**

With the release UI (or daemon) running and a song open, exercise the full path without hardware:

```bash
just cmd '{"id":1,"cmd":"midi.status"}'
just cmd '{"id":2,"cmd":"settings.set","params":{"key":"pedal_mapping","value":[{"trigger":"pc:0:0","action":"play_pause"},{"trigger":"pc:0:1","action":"set_marker","slot":1},{"trigger":"pc:0:2","action":"play_marker","slot":1},{"trigger":"pc:0:3","action":"cycle_snapshots"}]}}'
just cmd '{"id":3,"cmd":"pedal.trigger","params":{"trigger":"pc:0:0"}}'
just cmd '{"id":4,"cmd":"pedal.trigger","params":{"trigger":"pc:0:1"}}'
just cmd '{"id":5,"cmd":"pedal.trigger","params":{"trigger":"pc:0:2"}}'
```

Expected: `midi.status` lists `SINCO` when the pedal is on in U mode; trigger 0 toggles playback; trigger 1 sets marker 1 at the playhead (pip appears on the waveform); trigger 2 seeks there and plays.

- [ ] **Step 4: Runtime smoke of the UI**

`just check`/svelte-check miss Svelte runtime effect loops — smoke-test in the running app (vite dev or the built binary): open the pedal tab, add a binding, arm learn, stomp a pedal switch (or fire `pedal.trigger` is not enough here — learn needs a real `midi` event, so stomp the pedal or `aseqdump`-verify it's connected), save/activate/clear a snapshot chip, set/play/clear a marker. Watch the console for `effect_update_depth_exceeded`.

- [ ] **Step 5: Hardware checklist for Shawn**

The webview isn't chrome-debuggable, so finish with a human checklist (pedal on, U mode):
1. Stomp each switch — pedal tab learn flow captures `pc:0:0`…`pc:0:3`.
2. Stomp two-switch combos (1+2, 3+4) — note what triggers they produce; record them in the campaign doc.
3. Map: SW1 play/pause, SW2 restart loop, SW3 play marker 1, SW4 cycle snapshots — verify each behaves with a guitar-in-hands workflow.
4. Restart the app — mapping, markers, snapshots all survive.

- [ ] **Step 6: Update campaign record + commit**

Append verification results (including the combo trigger keys) to `docs/superpowers/campaigns/foot-pedal-control.md`. Then:

```bash
git add -A
git commit -m "docs: pedal-control verification results + combo triggers"
```

---

## Self-review checklist (run after writing, fixed inline)

- Spec coverage: markers (Tasks 1-2, 8, 10), snapshots (3-4, 9), MIDI+mapping (5-7, 10), learn flow (7, 10), song.open payload (2, 4), settings persistence (5, 7), testing section (all TDD steps + Task 11). Out-of-scope items from the spec stay out.
- Types consistent: `Marker {slot: u32, pos: f64}` ↔ TS `{slot, pos}`; `IsolationSnapshot {slot, name, state}` ↔ TS; trigger strings and action ids identical across `pedal.rs`, `run_trigger`, and `Pedal.svelte`'s `ACTIONS`.
- Known adaptation points (deliberate, flagged where they occur): library `entry()` accessor name, midir callback signature, waveform color constant, Button chip `oncontextmenu` forwarding, shared time formatter.
