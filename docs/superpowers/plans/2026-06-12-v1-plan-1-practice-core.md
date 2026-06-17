# dredge v1 — Plan 1: Workspace + `practice` crate

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold the Rust workspace and build the complete `practice` crate — the pure-logic practice-intelligence core (domain model, tempo curves, junction derivation, plan runner, spaced resurfacing, SQLite store, JSON sidecar).

**Architecture:** Cargo workspace with `crates/practice`, `crates/engine`, `crates/server` (latter two stubs for now). `practice` is pure logic + persistence: no audio types, no async. All times are `f64` seconds; rates are playback-speed multipliers (1.0 = full speed). SQLite via rusqlite (bundled), dates via the `time` crate, serialization via serde.

**Tech Stack:** Rust 2021, rusqlite 0.32 (bundled), serde/serde_json, time 0.3, thiserror.

**Spec:** `docs/superpowers/specs/2026-06-12-dredge-design.md`

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `rustfmt.toml`
- Create: `crates/practice/Cargo.toml`, `crates/practice/src/lib.rs`
- Create: `crates/engine/Cargo.toml`, `crates/engine/src/lib.rs`
- Create: `crates/server/Cargo.toml`, `crates/server/src/lib.rs`

- [x] **Step 1: Write workspace files**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/practice", "crates/engine", "crates/server"]

[workspace.package]
edition = "2021"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
time = { version = "0.3", features = ["serde", "macros", "formatting", "parsing"] }
rusqlite = { version = "0.32", features = ["bundled"] }
```

`.gitignore`:
```
/target
node_modules
```

`rustfmt.toml`:
```toml
edition = "2021"
```

`crates/practice/Cargo.toml`:
```toml
[package]
name = "practice"
version = "0.1.0"
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
time.workspace = true
rusqlite.workspace = true
```

`crates/engine/Cargo.toml` and `crates/server/Cargo.toml`: same shape, names `engine` / `server`, no dependencies yet.

Each `src/lib.rs` starts as:
```rust
//! (one-line crate description)
```

- [x] **Step 2: Verify build**

Run: `cargo build && cargo test`
Expected: compiles, 0 tests pass.

- [x] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: scaffold cargo workspace (practice/engine/server)"
```

---

### Task 2: Domain model (`practice::model`)

**Files:**
- Create: `crates/practice/src/model.rs`
- Modify: `crates/practice/src/lib.rs`
- Test: inline `#[cfg(test)]` in `model.rs`

- [x] **Step 1: Write failing serde round-trip test**

`crates/practice/src/model.rs` (types + test together; test first mentally, but Rust needs the types to compile — write both, verify the test exercises real behavior):

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SongId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectionId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LoopId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanId(pub i64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub id: SongId,
    pub title: String,
    pub artist: Option<String>,
    pub path: String,
    pub file_hash: String,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: SectionId,
    pub song_id: SongId,
    pub name: String,
    pub start: f64,
    pub end: f64,
    /// 0-based order within the song.
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoopKind {
    Manual,
    Junction { from_section: SectionId, to_section: SectionId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopRegion {
    pub id: LoopId,
    pub song_id: SongId,
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub kind: LoopKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "curve", rename_all = "snake_case")]
pub enum TempoCurve {
    /// Constant rate (the submaximal-dwell default, e.g. 0.9).
    Dwell { rate: f64 },
    /// start + step per rep, clamped at target.
    Ladder { start: f64, step: f64, target: f64 },
    /// Every `period`-th rep at `high`, others at `low` (touch target early).
    Oscillate { low: f64, high: f64, period: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum PlanStep {
    /// Playback-only passes before play-along (audiation gate).
    ListenFirst { loop_id: LoopId, reps: u32 },
    PlayReps { loop_id: LoopId, reps: u32, curve: TempoCurve },
    /// Interleaved rotation over several loops.
    Rotation {
        loop_ids: Vec<LoopId>,
        rounds: u32,
        reps_per_visit: u32,
        curve: TempoCurve,
    },
    /// Alternate audible pass / silent pass (play from memory).
    RecallTest { loop_id: LoopId, alternations: u32, rate: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: PlanId,
    pub song_id: SongId,
    pub name: String,
    pub steps: Vec<PlanStep>,
}

/// Self-rating after reps — musician-friendly three-point scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Miss,
    Shaky,
    Solid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_steps_roundtrip_through_json() {
        let steps = vec![
            PlanStep::ListenFirst { loop_id: LoopId(1), reps: 3 },
            PlanStep::PlayReps {
                loop_id: LoopId(1),
                reps: 5,
                curve: TempoCurve::Oscillate { low: 0.7, high: 1.0, period: 3 },
            },
            PlanStep::Rotation {
                loop_ids: vec![LoopId(1), LoopId(2)],
                rounds: 2,
                reps_per_visit: 2,
                curve: TempoCurve::Dwell { rate: 0.9 },
            },
            PlanStep::RecallTest { loop_id: LoopId(2), alternations: 4, rate: 1.0 },
        ];
        let json = serde_json::to_string(&steps).unwrap();
        let back: Vec<PlanStep> = serde_json::from_str(&json).unwrap();
        assert_eq!(steps, back);
        // tagged representation is the sidecar/socket contract — pin it
        assert!(json.contains("\"step\":\"listen_first\""));
        assert!(json.contains("\"curve\":\"oscillate\""));
    }
}
```

`lib.rs` adds:
```rust
pub mod model;
```

- [x] **Step 2: Run tests**

Run: `cargo test -p practice`
Expected: `plan_steps_roundtrip_through_json` PASS.

- [x] **Step 3: Commit**

```bash
git add -A && git commit -m "feat(practice): domain model with serde contracts"
```

---

### Task 3: Tempo curves (`practice::tempo`)

**Files:**
- Create: `crates/practice/src/tempo.rs`
- Modify: `crates/practice/src/lib.rs` (add `pub mod tempo;`)

- [x] **Step 1: Write failing tests**

`crates/practice/src/tempo.rs`:
```rust
use crate::model::TempoCurve;

impl TempoCurve {
    /// Playback rate for 0-based rep index. Clamped to [0.25, 2.0].
    pub fn rate_for_rep(&self, rep: u32) -> f64 {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dwell_is_constant() {
        let c = TempoCurve::Dwell { rate: 0.9 };
        assert_eq!(c.rate_for_rep(0), 0.9);
        assert_eq!(c.rate_for_rep(99), 0.9);
    }

    #[test]
    fn ladder_climbs_and_clamps_at_target() {
        let c = TempoCurve::Ladder { start: 0.6, step: 0.1, target: 0.9 };
        assert_eq!(c.rate_for_rep(0), 0.6);
        assert_eq!(c.rate_for_rep(2), 0.8);
        assert_eq!(c.rate_for_rep(3), 0.9);
        assert_eq!(c.rate_for_rep(50), 0.9);
    }

    #[test]
    fn oscillate_touches_high_every_period() {
        let c = TempoCurve::Oscillate { low: 0.7, high: 1.0, period: 3 };
        // reps 0,1 low; rep 2 high; repeat
        assert_eq!(c.rate_for_rep(0), 0.7);
        assert_eq!(c.rate_for_rep(1), 0.7);
        assert_eq!(c.rate_for_rep(2), 1.0);
        assert_eq!(c.rate_for_rep(5), 1.0);
    }

    #[test]
    fn oscillate_period_zero_treated_as_one() {
        let c = TempoCurve::Oscillate { low: 0.7, high: 1.0, period: 0 };
        assert_eq!(c.rate_for_rep(0), 1.0);
    }

    #[test]
    fn rates_clamped_to_engine_range() {
        let c = TempoCurve::Dwell { rate: 5.0 };
        assert_eq!(c.rate_for_rep(0), 2.0);
        let c = TempoCurve::Ladder { start: 0.1, step: 0.0, target: 0.1 };
        assert_eq!(c.rate_for_rep(0), 0.25);
    }
}
```

- [x] **Step 2: Run tests, verify failure**

Run: `cargo test -p practice tempo`
Expected: panics on `todo!()`.

- [x] **Step 3: Implement**

Replace `todo!()`:
```rust
        let raw = match *self {
            TempoCurve::Dwell { rate } => rate,
            TempoCurve::Ladder { start, step, target } => {
                (start + step * rep as f64).min(target)
            }
            TempoCurve::Oscillate { low, high, period } => {
                let period = period.max(1);
                if (rep + 1) % period == 0 { high } else { low }
            }
        };
        raw.clamp(0.25, 2.0)
```

- [x] **Step 4: Run tests, verify pass**

Run: `cargo test -p practice tempo`
Expected: 5 PASS.

- [x] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(practice): tempo curves (dwell/ladder/oscillate)"
```

---

### Task 4: Junction derivation (`practice::junction`)

**Files:**
- Create: `crates/practice/src/junction.rs`
- Modify: `crates/practice/src/lib.rs` (add `pub mod junction;`)

- [x] **Step 1: Write failing tests**

`crates/practice/src/junction.rs`:
```rust
use crate::model::{LoopKind, LoopRegion, LoopId, Section};

/// A junction loop spans the tail of one section into the head of the next.
/// `tail`/`head` are window lengths in seconds; windows are clamped so the
/// loop never extends outside the two sections. Sections are taken in
/// `position` order; non-adjacent gaps are included (the gap is part of the
/// transition). Returned loops have `id: LoopId(0)` (unsaved sentinel).
pub fn derive_junctions(sections: &[Section], tail: f64, head: f64) -> Vec<LoopRegion> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SectionId, SongId};

    fn sec(id: i64, name: &str, start: f64, end: f64, position: i32) -> Section {
        Section {
            id: SectionId(id),
            song_id: SongId(1),
            name: name.into(),
            start,
            end,
            position,
        }
    }

    #[test]
    fn derives_one_loop_per_adjacent_pair() {
        let secs = vec![
            sec(1, "Verse", 10.0, 30.0, 0),
            sec(2, "Chorus", 30.0, 50.0, 1),
            sec(3, "Bridge", 50.0, 60.0, 2),
        ];
        let loops = derive_junctions(&secs, 2.0, 2.0);
        assert_eq!(loops.len(), 2);
        assert_eq!(loops[0].name, "Verse→Chorus");
        assert_eq!(loops[0].start, 28.0);
        assert_eq!(loops[0].end, 32.0);
        assert_eq!(
            loops[0].kind,
            LoopKind::Junction { from_section: SectionId(1), to_section: SectionId(2) }
        );
    }

    #[test]
    fn windows_clamped_to_section_bounds() {
        let secs = vec![sec(1, "A", 0.0, 1.0, 0), sec(2, "B", 1.0, 1.5, 1)];
        let loops = derive_junctions(&secs, 5.0, 5.0);
        assert_eq!(loops[0].start, 0.0); // clamped to A.start
        assert_eq!(loops[0].end, 1.5); // clamped to B.end
    }

    #[test]
    fn gap_between_sections_is_included() {
        let secs = vec![sec(1, "A", 0.0, 10.0, 0), sec(2, "B", 14.0, 20.0, 1)];
        let loops = derive_junctions(&secs, 1.0, 1.0);
        assert_eq!(loops[0].start, 9.0);
        assert_eq!(loops[0].end, 15.0);
    }

    #[test]
    fn unsorted_input_is_ordered_by_position() {
        let secs = vec![sec(2, "B", 30.0, 50.0, 1), sec(1, "A", 10.0, 30.0, 0)];
        let loops = derive_junctions(&secs, 2.0, 2.0);
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].name, "A→B");
    }

    #[test]
    fn fewer_than_two_sections_yields_nothing() {
        assert!(derive_junctions(&[], 2.0, 2.0).is_empty());
        assert!(derive_junctions(&[sec(1, "A", 0.0, 10.0, 0)], 2.0, 2.0).is_empty());
    }
}
```

- [x] **Step 2: Run tests, verify failure**

Run: `cargo test -p practice junction`
Expected: panics on `todo!()`.

- [x] **Step 3: Implement**

```rust
    let mut ordered: Vec<&Section> = sections.iter().collect();
    ordered.sort_by_key(|s| s.position);
    ordered
        .windows(2)
        .map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            LoopRegion {
                id: LoopId(0),
                song_id: a.song_id,
                name: format!("{}→{}", a.name, b.name),
                start: (a.end - tail).max(a.start),
                end: (b.start + head).min(b.end),
                kind: LoopKind::Junction { from_section: a.id, to_section: b.id },
            }
        })
        .collect()
```

- [x] **Step 4: Run tests, verify pass**

Run: `cargo test -p practice junction`
Expected: 5 PASS.

- [x] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(practice): junction loop derivation from adjacent sections"
```

---

### Task 5: Plan runner (`practice::runner`)

**Files:**
- Create: `crates/practice/src/runner.rs`
- Modify: `crates/practice/src/lib.rs` (add `pub mod runner;`)

The runner is pull-based: callers ask `current()` for the rep to perform
(loop, rate, mode); when the engine reports a loop-wrap (or UI skips), call
`advance()`. `current() == None` means the plan is finished.

- [x] **Step 1: Write failing tests**

`crates/practice/src/runner.rs`:
```rust
use crate::model::{LoopId, Plan, PlanStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepMode {
    /// Playback only — instrument down, ears on.
    Listen,
    /// Play along with the recording.
    Play,
    /// Recording muted — play the passage from memory.
    RecallSilent,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct RepSpec {
    pub loop_id: LoopId,
    pub rate: f64,
    pub mode: RepMode,
    pub step_idx: usize,
    pub rep_idx: u32,
}

pub struct PlanRunner {
    plan: Plan,
    step_idx: usize,
    rep_idx: u32,
}

impl PlanRunner {
    pub fn new(plan: Plan) -> Self {
        Self { plan, step_idx: 0, rep_idx: 0 }
    }

    pub fn current(&self) -> Option<RepSpec> {
        todo!()
    }

    pub fn advance(&mut self) {
        todo!()
    }

    /// Skip the rest of the current step.
    pub fn skip_step(&mut self) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlanId, SongId, TempoCurve};

    fn plan(steps: Vec<PlanStep>) -> Plan {
        Plan { id: PlanId(1), song_id: SongId(1), name: "p".into(), steps }
    }

    fn drain(mut r: PlanRunner) -> Vec<RepSpec> {
        let mut out = Vec::new();
        while let Some(spec) = r.current() {
            out.push(spec);
            r.advance();
        }
        out
    }

    #[test]
    fn listen_first_yields_listen_reps() {
        let r = PlanRunner::new(plan(vec![PlanStep::ListenFirst {
            loop_id: LoopId(7),
            reps: 2,
        }]));
        let reps = drain(r);
        assert_eq!(reps.len(), 2);
        assert!(reps.iter().all(|s| s.mode == RepMode::Listen && s.loop_id == LoopId(7)));
        assert!(reps.iter().all(|s| s.rate == 1.0));
        assert_eq!(reps[1].rep_idx, 1);
    }

    #[test]
    fn play_reps_apply_curve_per_rep() {
        let r = PlanRunner::new(plan(vec![PlanStep::PlayReps {
            loop_id: LoopId(1),
            reps: 3,
            curve: TempoCurve::Ladder { start: 0.6, step: 0.1, target: 1.0 },
        }]));
        let rates: Vec<f64> = drain(r).iter().map(|s| s.rate).collect();
        assert_eq!(rates, vec![0.6, 0.7, 0.8]);
    }

    #[test]
    fn steps_run_in_sequence() {
        let r = PlanRunner::new(plan(vec![
            PlanStep::ListenFirst { loop_id: LoopId(1), reps: 1 },
            PlanStep::PlayReps {
                loop_id: LoopId(1),
                reps: 1,
                curve: TempoCurve::Dwell { rate: 0.9 },
            },
        ]));
        let reps = drain(r);
        assert_eq!(reps.len(), 2);
        assert_eq!(reps[0].mode, RepMode::Listen);
        assert_eq!(reps[1].mode, RepMode::Play);
        assert_eq!(reps[1].step_idx, 1);
    }

    #[test]
    fn rotation_interleaves_loops_round_robin() {
        let r = PlanRunner::new(plan(vec![PlanStep::Rotation {
            loop_ids: vec![LoopId(1), LoopId(2), LoopId(3)],
            rounds: 2,
            reps_per_visit: 1,
            curve: TempoCurve::Dwell { rate: 0.8 },
        }]));
        let ids: Vec<i64> = drain(r).iter().map(|s| s.loop_id.0).collect();
        // interleaved: 1,2,3,1,2,3 — NOT 1,1,2,2,3,3
        assert_eq!(ids, vec![1, 2, 3, 1, 2, 3]);
    }

    #[test]
    fn rotation_curve_advances_per_visit_to_same_loop() {
        let r = PlanRunner::new(plan(vec![PlanStep::Rotation {
            loop_ids: vec![LoopId(1), LoopId(2)],
            rounds: 2,
            reps_per_visit: 2,
            curve: TempoCurve::Ladder { start: 0.6, step: 0.1, target: 1.0 },
        }]));
        let reps = drain(r);
        // loop 1 visits: round0 (rate for visit 0), round1 (rate for visit 1)
        let loop1_rates: Vec<f64> = reps
            .iter()
            .filter(|s| s.loop_id == LoopId(1))
            .map(|s| s.rate)
            .collect();
        assert_eq!(loop1_rates, vec![0.6, 0.6, 0.7, 0.7]);
    }

    #[test]
    fn recall_test_alternates_play_and_silent() {
        let r = PlanRunner::new(plan(vec![PlanStep::RecallTest {
            loop_id: LoopId(5),
            alternations: 2,
            rate: 1.0,
        }]));
        let modes: Vec<RepMode> = drain(r).iter().map(|s| s.mode).collect();
        assert_eq!(
            modes,
            vec![RepMode::Play, RepMode::RecallSilent, RepMode::Play, RepMode::RecallSilent]
        );
    }

    #[test]
    fn skip_step_jumps_to_next_step() {
        let mut r = PlanRunner::new(plan(vec![
            PlanStep::ListenFirst { loop_id: LoopId(1), reps: 10 },
            PlanStep::ListenFirst { loop_id: LoopId(2), reps: 1 },
        ]));
        r.advance();
        r.skip_step();
        assert_eq!(r.current().unwrap().loop_id, LoopId(2));
        r.advance();
        assert!(r.current().is_none());
    }

    #[test]
    fn empty_plan_is_immediately_finished() {
        let r = PlanRunner::new(plan(vec![]));
        assert!(r.current().is_none());
    }

    #[test]
    fn empty_rotation_step_is_skipped() {
        let r = PlanRunner::new(plan(vec![PlanStep::Rotation {
            loop_ids: vec![],
            rounds: 3,
            reps_per_visit: 1,
            curve: TempoCurve::Dwell { rate: 0.8 },
        }]));
        assert!(drain(r).is_empty());
    }
}
```

- [x] **Step 2: Run tests, verify failure**

Run: `cargo test -p practice runner`
Expected: panics on `todo!()`.

- [x] **Step 3: Implement**

Key insight: a step has a deterministic total rep count; `rep_idx` is the
flat index within the step. Everything derives from those two numbers.

```rust
impl PlanRunner {
    fn step_total_reps(step: &PlanStep) -> u32 {
        match step {
            PlanStep::ListenFirst { reps, .. } => *reps,
            PlanStep::PlayReps { reps, .. } => *reps,
            PlanStep::Rotation { loop_ids, rounds, reps_per_visit, .. } => {
                loop_ids.len() as u32 * rounds * reps_per_visit
            }
            PlanStep::RecallTest { alternations, .. } => alternations * 2,
        }
    }

    pub fn current(&self) -> Option<RepSpec> {
        let mut step_idx = self.step_idx;
        let mut rep_idx = self.rep_idx;
        // skip exhausted/empty steps
        loop {
            let step = self.plan.steps.get(step_idx)?;
            if rep_idx < Self::step_total_reps(step) {
                return Some(Self::spec(step, step_idx, rep_idx));
            }
            step_idx += 1;
            rep_idx = 0;
        }
    }

    fn spec(step: &PlanStep, step_idx: usize, rep_idx: u32) -> RepSpec {
        match step {
            PlanStep::ListenFirst { loop_id, .. } => RepSpec {
                loop_id: *loop_id,
                rate: 1.0,
                mode: RepMode::Listen,
                step_idx,
                rep_idx,
            },
            PlanStep::PlayReps { loop_id, curve, .. } => RepSpec {
                loop_id: *loop_id,
                rate: curve.rate_for_rep(rep_idx),
                mode: RepMode::Play,
                step_idx,
                rep_idx,
            },
            PlanStep::Rotation { loop_ids, reps_per_visit, curve, .. } => {
                let visit = rep_idx / reps_per_visit.max(1);
                let slot = (visit as usize) % loop_ids.len();
                let visits_to_this_loop = visit / loop_ids.len() as u32;
                RepSpec {
                    loop_id: loop_ids[slot],
                    rate: curve.rate_for_rep(visits_to_this_loop),
                    mode: RepMode::Play,
                    step_idx,
                    rep_idx,
                }
            }
            PlanStep::RecallTest { loop_id, rate, .. } => RepSpec {
                loop_id: *loop_id,
                rate: *rate,
                mode: if rep_idx % 2 == 0 { RepMode::Play } else { RepMode::RecallSilent },
                step_idx,
                rep_idx,
            },
        }
    }

    pub fn advance(&mut self) {
        // normalize to the position current() resolved to, then step once
        if let Some(spec) = self.current() {
            self.step_idx = spec.step_idx;
            self.rep_idx = spec.rep_idx + 1;
        }
    }

    pub fn skip_step(&mut self) {
        if let Some(spec) = self.current() {
            self.step_idx = spec.step_idx + 1;
            self.rep_idx = 0;
        }
    }
}
```

- [x] **Step 4: Run tests, verify pass**

Run: `cargo test -p practice runner`
Expected: 9 PASS.

- [x] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(practice): pull-based plan runner with rotation and recall modes"
```

---

### Task 6: Spaced resurfacing (`practice::schedule`)

**Files:**
- Create: `crates/practice/src/schedule.rs`
- Modify: `crates/practice/src/lib.rs` (add `pub mod schedule;`)

- [x] **Step 1: Write failing tests**

`crates/practice/src/schedule.rs`:
```rust
use crate::model::{LoopId, Rating};
use serde::{Deserialize, Serialize};
use time::Date;

/// Interval ladder in days. Solid advances a rung, Shaky repeats the rung,
/// Miss resets to the first rung.
pub const INTERVALS: [i64; 5] = [1, 2, 4, 7, 14];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Resurfacing {
    pub loop_id: LoopId,
    pub interval_idx: usize,
    pub due_on: Date,
}

/// Compute the next schedule state after a rated practice on `today`.
pub fn next_state(prev: Option<Resurfacing>, loop_id: LoopId, rating: Rating, today: Date) -> Resurfacing {
    todo!()
}

/// Loops due on or before `today`, soonest-due first.
pub fn due(items: &[Resurfacing], today: Date) -> Vec<LoopId> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn first_rating_schedules_first_interval() {
        let s = next_state(None, LoopId(1), Rating::Solid, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 0);
        assert_eq!(s.due_on, date!(2026 - 06 - 13));
    }

    #[test]
    fn solid_advances_the_ladder() {
        let prev = Resurfacing { loop_id: LoopId(1), interval_idx: 1, due_on: date!(2026 - 06 - 12) };
        let s = next_state(Some(prev), LoopId(1), Rating::Solid, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 2);
        assert_eq!(s.due_on, date!(2026 - 06 - 16)); // +4 days
    }

    #[test]
    fn solid_clamps_at_top_rung() {
        let prev = Resurfacing { loop_id: LoopId(1), interval_idx: 4, due_on: date!(2026 - 06 - 12) };
        let s = next_state(Some(prev), LoopId(1), Rating::Solid, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 4);
        assert_eq!(s.due_on, date!(2026 - 06 - 26)); // +14 days
    }

    #[test]
    fn shaky_repeats_the_rung() {
        let prev = Resurfacing { loop_id: LoopId(1), interval_idx: 2, due_on: date!(2026 - 06 - 12) };
        let s = next_state(Some(prev), LoopId(1), Rating::Shaky, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 2);
        assert_eq!(s.due_on, date!(2026 - 06 - 16));
    }

    #[test]
    fn miss_resets_to_first_rung() {
        let prev = Resurfacing { loop_id: LoopId(1), interval_idx: 3, due_on: date!(2026 - 06 - 12) };
        let s = next_state(Some(prev), LoopId(1), Rating::Miss, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 0);
        assert_eq!(s.due_on, date!(2026 - 06 - 13));
    }

    #[test]
    fn due_returns_overdue_and_today_sorted() {
        let items = vec![
            Resurfacing { loop_id: LoopId(1), interval_idx: 0, due_on: date!(2026 - 06 - 14) },
            Resurfacing { loop_id: LoopId(2), interval_idx: 0, due_on: date!(2026 - 06 - 10) },
            Resurfacing { loop_id: LoopId(3), interval_idx: 0, due_on: date!(2026 - 06 - 12) },
        ];
        assert_eq!(due(&items, date!(2026 - 06 - 12)), vec![LoopId(2), LoopId(3)]);
    }
}
```

- [x] **Step 2: Run tests, verify failure**

Run: `cargo test -p practice schedule`
Expected: panics on `todo!()`.

- [x] **Step 3: Implement**

```rust
pub fn next_state(prev: Option<Resurfacing>, loop_id: LoopId, rating: Rating, today: Date) -> Resurfacing {
    let interval_idx = match (prev, rating) {
        (None, _) | (_, Rating::Miss) => 0,
        (Some(p), Rating::Shaky) => p.interval_idx,
        (Some(p), Rating::Solid) => (p.interval_idx + 1).min(INTERVALS.len() - 1),
    };
    Resurfacing {
        loop_id,
        interval_idx,
        due_on: today + time::Duration::days(INTERVALS[interval_idx]),
    }
}

pub fn due(items: &[Resurfacing], today: Date) -> Vec<LoopId> {
    let mut hits: Vec<&Resurfacing> = items.iter().filter(|r| r.due_on <= today).collect();
    hits.sort_by_key(|r| r.due_on);
    hits.into_iter().map(|r| r.loop_id).collect()
}
```

- [x] **Step 4: Run tests, verify pass**

Run: `cargo test -p practice schedule`
Expected: 6 PASS.

- [x] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(practice): spaced resurfacing ladder"
```

---

### Task 7: SQLite store (`practice::store`)

**Files:**
- Create: `crates/practice/src/store.rs`, `crates/practice/src/error.rs`
- Modify: `crates/practice/src/lib.rs` (add `pub mod store; pub mod error;`)
- Test: `crates/practice/tests/store.rs` (integration-style, in-memory DB)

- [x] **Step 1: Write error type and store skeleton**

`crates/practice/src/error.rs`:
```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, Error>;
```

`crates/practice/src/store.rs` — schema and full API. Schema (single
migration v1, `PRAGMA user_version` tracks version):

```sql
CREATE TABLE songs (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    artist TEXT,
    path TEXT NOT NULL,
    file_hash TEXT NOT NULL UNIQUE,
    duration_secs REAL NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE sections (
    id INTEGER PRIMARY KEY,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    start_secs REAL NOT NULL,
    end_secs REAL NOT NULL,
    position INTEGER NOT NULL
);
CREATE TABLE loops (
    id INTEGER PRIMARY KEY,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    start_secs REAL NOT NULL,
    end_secs REAL NOT NULL,
    kind_json TEXT NOT NULL
);
CREATE TABLE plans (
    id INTEGER PRIMARY KEY,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    steps_json TEXT NOT NULL
);
CREATE TABLE reps (
    id INTEGER PRIMARY KEY,
    loop_id INTEGER NOT NULL REFERENCES loops(id) ON DELETE CASCADE,
    plan_id INTEGER REFERENCES plans(id) ON DELETE SET NULL,
    mode TEXT NOT NULL,
    rate REAL NOT NULL,
    rating TEXT,
    is_retest INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE resurfacing (
    loop_id INTEGER PRIMARY KEY REFERENCES loops(id) ON DELETE CASCADE,
    interval_idx INTEGER NOT NULL,
    due_on TEXT NOT NULL
);
```

Public API (signatures — implement with straightforward rusqlite calls;
`kind_json`/`steps_json` round-trip through serde_json; enable
`PRAGMA foreign_keys = ON` on open):

```rust
pub struct Store { conn: rusqlite::Connection }

pub struct NewSong<'a> { pub title: &'a str, pub artist: Option<&'a str>, pub path: &'a str, pub file_hash: &'a str, pub duration_secs: f64 }
pub struct NewSection<'a> { pub name: &'a str, pub start: f64, pub end: f64, pub position: i32 }
pub struct NewLoop<'a> { pub name: &'a str, pub start: f64, pub end: f64, pub kind: LoopKind }
pub struct NewRep { pub loop_id: LoopId, pub plan_id: Option<PlanId>, pub mode: String, pub rate: f64, pub rating: Option<Rating>, pub is_retest: bool }

impl Store {
    pub fn open(path: &std::path::Path) -> Result<Self>;
    pub fn open_in_memory() -> Result<Self>;

    pub fn insert_song(&self, s: NewSong) -> Result<Song>;
    pub fn song_by_hash(&self, hash: &str) -> Result<Option<Song>>;
    pub fn list_songs(&self) -> Result<Vec<Song>>;

    /// Replace all sections for a song atomically (UI saves whole lane).
    pub fn replace_sections(&mut self, song_id: SongId, sections: &[NewSection]) -> Result<Vec<Section>>;
    pub fn list_sections(&self, song_id: SongId) -> Result<Vec<Section>>;

    pub fn insert_loop(&self, song_id: SongId, l: NewLoop) -> Result<LoopRegion>;
    pub fn delete_loop(&self, id: LoopId) -> Result<()>;
    pub fn list_loops(&self, song_id: SongId) -> Result<Vec<LoopRegion>>;

    pub fn save_plan(&self, song_id: SongId, name: &str, steps: &[PlanStep]) -> Result<Plan>;
    pub fn list_plans(&self, song_id: SongId) -> Result<Vec<Plan>>;

    pub fn record_rep(&self, r: NewRep) -> Result<()>;

    pub fn upsert_resurfacing(&self, r: Resurfacing) -> Result<()>;
    pub fn all_resurfacing(&self) -> Result<Vec<Resurfacing>>;

    /// Latest retest rating per loop — the retention metric.
    pub fn retention(&self, song_id: SongId) -> Result<Vec<(LoopId, Rating, String)>>;
}
```

- [x] **Step 2: Write failing integration tests**

`crates/practice/tests/store.rs`:
```rust
use practice::model::*;
use practice::schedule::Resurfacing;
use practice::store::*;
use time::macros::date;

fn store_with_song() -> (Store, Song) {
    let store = Store::open_in_memory().unwrap();
    let song = store
        .insert_song(NewSong {
            title: "Song",
            artist: Some("Band"),
            path: "/tmp/song.flac",
            file_hash: "abc123",
            duration_secs: 240.0,
        })
        .unwrap();
    (store, song)
}

#[test]
fn song_roundtrip_and_hash_lookup() {
    let (store, song) = store_with_song();
    assert_eq!(store.song_by_hash("abc123").unwrap().unwrap().id, song.id);
    assert!(store.song_by_hash("nope").unwrap().is_none());
    assert_eq!(store.list_songs().unwrap().len(), 1);
}

#[test]
fn sections_replace_atomically_in_position_order() {
    let (mut store, song) = store_with_song();
    store
        .replace_sections(
            song.id,
            &[
                NewSection { name: "Chorus", start: 30.0, end: 50.0, position: 1 },
                NewSection { name: "Verse", start: 10.0, end: 30.0, position: 0 },
            ],
        )
        .unwrap();
    let sections = store.list_sections(song.id).unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].name, "Verse"); // ordered by position
    // replace drops the old set
    store
        .replace_sections(song.id, &[NewSection { name: "Solo", start: 0.0, end: 5.0, position: 0 }])
        .unwrap();
    assert_eq!(store.list_sections(song.id).unwrap().len(), 1);
}

#[test]
fn loops_roundtrip_with_kind() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(
            song.id,
            NewLoop {
                name: "Verse→Chorus",
                start: 28.0,
                end: 32.0,
                kind: LoopKind::Junction { from_section: SectionId(1), to_section: SectionId(2) },
            },
        )
        .unwrap();
    let loops = store.list_loops(song.id).unwrap();
    assert_eq!(loops, vec![l.clone()]);
    store.delete_loop(l.id).unwrap();
    assert!(store.list_loops(song.id).unwrap().is_empty());
}

#[test]
fn plans_roundtrip_steps_json() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(song.id, NewLoop { name: "A", start: 0.0, end: 4.0, kind: LoopKind::Manual })
        .unwrap();
    let steps = vec![PlanStep::PlayReps {
        loop_id: l.id,
        reps: 5,
        curve: TempoCurve::Dwell { rate: 0.9 },
    }];
    let plan = store.save_plan(song.id, "warmup", &steps).unwrap();
    assert_eq!(store.list_plans(song.id).unwrap(), vec![plan.clone()]);
    assert_eq!(plan.steps, steps);
}

#[test]
fn retention_reports_latest_retest_per_loop() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(song.id, NewLoop { name: "A", start: 0.0, end: 4.0, kind: LoopKind::Manual })
        .unwrap();
    store
        .record_rep(NewRep { loop_id: l.id, plan_id: None, mode: "play".into(), rate: 0.9, rating: Some(Rating::Shaky), is_retest: true })
        .unwrap();
    store
        .record_rep(NewRep { loop_id: l.id, plan_id: None, mode: "play".into(), rate: 1.0, rating: Some(Rating::Solid), is_retest: true })
        .unwrap();
    // non-retest reps don't count
    store
        .record_rep(NewRep { loop_id: l.id, plan_id: None, mode: "play".into(), rate: 1.0, rating: Some(Rating::Miss), is_retest: false })
        .unwrap();
    let retention = store.retention(song.id).unwrap();
    assert_eq!(retention.len(), 1);
    assert_eq!(retention[0].0, l.id);
    assert_eq!(retention[0].1, Rating::Solid);
}

#[test]
fn resurfacing_upserts() {
    let (store, song) = store_with_song();
    let l = store
        .insert_loop(song.id, NewLoop { name: "A", start: 0.0, end: 4.0, kind: LoopKind::Manual })
        .unwrap();
    let r1 = Resurfacing { loop_id: l.id, interval_idx: 0, due_on: date!(2026 - 06 - 13) };
    let r2 = Resurfacing { loop_id: l.id, interval_idx: 1, due_on: date!(2026 - 06 - 15) };
    store.upsert_resurfacing(r1).unwrap();
    store.upsert_resurfacing(r2).unwrap();
    assert_eq!(store.all_resurfacing().unwrap(), vec![r2]);
}

#[test]
fn deleting_song_cascades() {
    let (store, song) = store_with_song();
    store
        .insert_loop(song.id, NewLoop { name: "A", start: 0.0, end: 4.0, kind: LoopKind::Manual })
        .unwrap();
    store.delete_song(song.id).unwrap();
    assert!(store.list_loops(song.id).unwrap().is_empty());
    assert!(store.list_songs().unwrap().is_empty());
}
```

Note: the cascade test requires `pub fn delete_song(&self, id: SongId) -> Result<()>` — add it to the API.

- [x] **Step 3: Run tests, verify failure**

Run: `cargo test -p practice --test store`
Expected: compile errors (Store not implemented) — that's the failing state for scaffolding tasks.

- [x] **Step 4: Implement Store**

Implementation notes (write the obvious rusqlite code):
- `open()`: `Connection::open(path)`, then `PRAGMA foreign_keys = ON`, then `migrate()`.
- `migrate()`: if `PRAGMA user_version` is 0, run the schema SQL in one `execute_batch`, then `PRAGMA user_version = 1`.
- Ratings stored as `"miss" | "shaky" | "solid"` text (serde_json's representation without quotes — use a small `rating_to_str`/`rating_from_str` helper, don't serialize with quotes into the column).
- `replace_sections` uses a transaction: `DELETE FROM sections WHERE song_id = ?`, then inserts; needs `&mut self` for `self.conn.transaction()`.
- `retention()`:
```sql
SELECT r.loop_id, r.rating, r.created_at FROM reps r
JOIN loops l ON l.id = r.loop_id
WHERE l.song_id = ?1 AND r.is_retest = 1 AND r.rating IS NOT NULL
  AND r.id = (SELECT MAX(id) FROM reps r2 WHERE r2.loop_id = r.loop_id AND r2.is_retest = 1 AND r2.rating IS NOT NULL)
```
- `due_on` stored as ISO `YYYY-MM-DD` text; parse with `time::Date::parse` and the `format_description!("[year]-[month]-[day]")` macro.

- [x] **Step 5: Run tests, verify pass**

Run: `cargo test -p practice --test store`
Expected: 7 PASS.

- [x] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(practice): sqlite store with migrations and retention query"
```

---

### Task 8: JSON sidecar (`practice::sidecar`)

**Files:**
- Create: `crates/practice/src/sidecar.rs`
- Modify: `crates/practice/src/lib.rs` (add `pub mod sidecar;`)
- Test: inline `#[cfg(test)]` using `tempfile` (add `tempfile = "3"` to `[dev-dependencies]`)

- [x] **Step 1: Write failing tests**

`crates/practice/src/sidecar.rs`:
```rust
use crate::error::Result;
use crate::model::{LoopRegion, Plan, Section, Song};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Everything a user could lose, mirrored as plain JSON next to the audio
/// file: `<audio path>.dredge.json`. Written atomically (tmp + rename).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub version: u32,
    pub song: Song,
    pub sections: Vec<Section>,
    pub loops: Vec<LoopRegion>,
    pub plans: Vec<Plan>,
}

pub fn sidecar_path(audio_path: &Path) -> PathBuf {
    todo!()
}

pub fn write_sidecar(s: &Sidecar) -> Result<PathBuf> {
    todo!()
}

pub fn read_sidecar(audio_path: &Path) -> Result<Option<Sidecar>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn sample(dir: &Path) -> Sidecar {
        Sidecar {
            version: 1,
            song: Song {
                id: SongId(1),
                title: "T".into(),
                artist: None,
                path: dir.join("song.flac").to_string_lossy().into_owned(),
                file_hash: "h".into(),
                duration_secs: 10.0,
            },
            sections: vec![],
            loops: vec![LoopRegion {
                id: LoopId(1),
                song_id: SongId(1),
                name: "riff".into(),
                start: 1.0,
                end: 2.0,
                kind: LoopKind::Manual,
            }],
            plans: vec![],
        }
    }

    #[test]
    fn path_appends_dredge_json() {
        assert_eq!(
            sidecar_path(Path::new("/x/song.flac")),
            PathBuf::from("/x/song.flac.dredge.json")
        );
    }

    #[test]
    fn write_then_read_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let s = sample(dir.path());
        let written_to = write_sidecar(&s).unwrap();
        assert!(written_to.exists());
        let back = read_sidecar(Path::new(&s.song.path)).unwrap().unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn missing_sidecar_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_sidecar(&dir.path().join("nope.flac")).unwrap().is_none());
    }

    #[test]
    fn no_partial_file_left_on_write() {
        // atomicity contract: tmp file is renamed, never left behind
        let dir = tempfile::tempdir().unwrap();
        let s = sample(dir.path());
        write_sidecar(&s).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["song.flac.dredge.json".to_string()]);
    }
}
```

- [x] **Step 2: Run tests, verify failure**

Run: `cargo test -p practice sidecar`
Expected: panics on `todo!()`.

- [x] **Step 3: Implement**

```rust
pub fn sidecar_path(audio_path: &Path) -> PathBuf {
    let mut os = audio_path.as_os_str().to_owned();
    os.push(".dredge.json");
    PathBuf::from(os)
}

pub fn write_sidecar(s: &Sidecar) -> Result<PathBuf> {
    let path = sidecar_path(Path::new(&s.song.path));
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

pub fn read_sidecar(audio_path: &Path) -> Result<Option<Sidecar>> {
    let path = sidecar_path(audio_path);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
```

- [x] **Step 4: Run tests, verify pass**

Run: `cargo test -p practice sidecar`
Expected: 4 PASS.

- [x] **Step 5: Full crate check and commit**

Run: `cargo test -p practice && cargo clippy -p practice -- -D warnings && cargo fmt --check`
Expected: all green (fix any clippy/fmt fallout first).

```bash
git add -A && git commit -m "feat(practice): atomic JSON sidecar mirror"
```

---

## Self-review checklist (run after Task 8)

- Spec coverage for this plan's scope: domain ✔, tempo curves ✔ (dwell/ladder/oscillate), junction derivation ✔ (time-windowed, clamped), plan runner ✔ (listen-first, rotation interleave, recall alternation, skip), resurfacing ✔, SQLite + migrations + retention ✔, sidecar mirror ✔.
- Deliberately deferred to later plans: engine (Plan 2), socket/dispatcher (Plan 3), UI (Plan 4), capture (v2), stems (v3).
