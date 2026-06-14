# Transient Looping + Dynamic Loop Naming — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make waveform looping transient-by-default (loop now, no save), make saving a deliberate act that lands in the Loops tab, and name saved loops dynamically from the song's sections (with a manual override).

**Architecture:** The naming algorithm is a pure function in the `practice` crate, unit-tested. The `server` is the single source of truth: it computes a loop's effective `name` on create / bounds-change / section-change, unless a `name_override` is set. The frontend just renders `loop.name`; the waveform's two selection glyphs become **loop (transient)** and **save**, and the Loops tab gains a **fit to section** action.

**Tech Stack:** Rust (rusqlite, serde), Svelte 5 + TypeScript (Tauri), cargo test + vitest.

Spec: `docs/superpowers/specs/2026-06-14-transient-looping-and-dynamic-naming-design.md`

---

## File Structure

**Backend (Rust):**
- Create `crates/practice/src/naming.rs` — pure `loop_name()` + occurrence labeling + tests.
- Modify `crates/practice/src/lib.rs` — declare `pub mod naming;`.
- Modify `crates/practice/src/model.rs` — `LoopRegion.name_override: Option<String>`.
- Modify `crates/practice/src/store.rs` — schema V6 `ALTER TABLE loops ADD COLUMN name_override`; `NewLoop.name_override`; insert/update/read columns; `clear_override`/recompute support.
- Modify `crates/server/src/app.rs` — `auto_name_loop()` helper; rewire `loop_create`, `loop_update`, `section_replace`, `quick_rate`; add `loop_fit` + `"loop.fit"` dispatch arm.

**Frontend (Svelte/TS):**
- Modify `apps/desktop/src/lib/stores.ts` — `LoopRegion.name_override`; `loopsOpen` store; `createLoop` (no name) / `saveLoop` / `fitLoop`; `updateLoop` empty-name = clear override.
- Modify `apps/desktop/src/App.svelte` — `loopsOpen` → switch to loops tab.
- Modify `apps/desktop/src/components/Waveform.svelte` — glyph 1 = transient loop, glyph 2 = save; drop `autoLoopName`/`loopSelection` persistence.
- Modify `apps/desktop/src/components/Loops.svelte` — "fit to section" button; rename revert-to-dynamic on empty.
- Modify `apps/desktop/src/components/Library.svelte` — Enter submits the rename-track modal.

---

## Task 1: Pure loop-naming algorithm in `practice`

**Files:**
- Create: `crates/practice/src/naming.rs`
- Modify: `crates/practice/src/lib.rs`

- [ ] **Step 1: Declare the module**

In `crates/practice/src/lib.rs`, add the module declaration alongside the others (e.g. after `pub mod model;`):

```rust
pub mod naming;
```

- [ ] **Step 2: Write the failing tests**

Create `crates/practice/src/naming.rs`:

```rust
//! Dynamic loop names derived from a song's sections. A loop is named after the
//! section(s) it covers; the occurrence number distinguishes the Nth section of
//! a given name (the 2nd `verse` is `verse 2`). Names recompute as bounds or
//! sections change, unless the user pins a manual override (handled by the
//! caller). Section labels are used verbatim (lowercase `verse`, letters `A`).

use crate::model::Section;

/// Boundary tolerance (seconds). An edge within EPS of a section boundary counts
/// as "on" it — header-drag selections hit boundaries exactly; this absorbs
/// float noise and lets "fit to section" read as full coverage.
const EPS: f64 = 0.05;

/// `name occurrence` for a section — 1-based count among same-named sections, in
/// `position` order. The only section named `chorus` is `chorus 1`.
fn occurrence_label(section: &Section, sections: &[Section]) -> String {
    let mut ordered: Vec<&Section> = sections.iter().collect();
    ordered.sort_by_key(|s| s.position);
    let n = ordered
        .iter()
        .filter(|s| s.name == section.name)
        .take_while(|s| s.id != section.id)
        .count()
        + 1;
    format!("{} {}", section.name, n)
}

/// Sections the loop `[start, end]` overlaps, in `position` order. A section
/// barely touched at a shared boundary (within EPS) does not count.
fn overlapping<'a>(start: f64, end: f64, sections: &'a [Section]) -> Vec<&'a Section> {
    let mut ordered: Vec<&Section> = sections
        .iter()
        .filter(|s| s.start < end - EPS && s.end > start + EPS)
        .collect();
    ordered.sort_by_key(|s| s.position);
    ordered
}

/// `riff m:ss.t–m:ss.t` — the fallback when the loop covers no section.
/// Mirrors `server::app::fmt_ts`.
fn fmt_ts(secs: f64) -> String {
    let tenths = (secs * 10.0).round() as i64;
    format!("{}:{:02}.{}", tenths / 600, tenths % 600 / 10, tenths % 10)
}

/// Compute a loop's display name from its bounds and the song's sections,
/// disambiguating against `existing` loop names with a `(n)` suffix.
pub fn loop_name(start: f64, end: f64, sections: &[Section], existing: &[String]) -> String {
    let base = base_name(start, end, sections);
    disambiguate(base, existing)
}

fn base_name(start: f64, end: f64, sections: &[Section]) -> String {
    let hit = overlapping(start, end, sections);
    match hit.as_slice() {
        [] => format!("riff {}–{}", fmt_ts(start), fmt_ts(end)),
        [s] => {
            let full = start <= s.start + EPS && end >= s.end - EPS;
            let label = occurrence_label(s, sections);
            if full { label } else { format!("sub {label}") }
        }
        [first, .., last] => {
            let left = edge_label(first, sections, start <= first.start + EPS);
            let right = edge_label(last, sections, end >= last.end - EPS);
            format!("{left} → {right}")
        }
    }
}

/// `verse 2` when the loop fully covers this endpoint section, `sub verse 2`
/// when it only partially covers it.
fn edge_label(section: &Section, sections: &[Section], full: bool) -> String {
    let label = occurrence_label(section, sections);
    if full { label } else { format!("sub {label}") }
}

/// Append `(2)`, `(3)`, … until the name is unique among `existing`.
fn disambiguate(base: String, existing: &[String]) -> String {
    if !existing.iter().any(|n| n == &base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base} ({n})");
        if !existing.iter().any(|x| x == &candidate) {
            return candidate;
        }
        n += 1;
    }
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

    // intro[0,10] verse[10,30] verse[30,50] chorus[50,70]
    fn song() -> Vec<Section> {
        vec![
            sec(1, "intro", 0.0, 10.0, 0),
            sec(2, "verse", 10.0, 30.0, 1),
            sec(3, "verse", 30.0, 50.0, 2),
            sec(4, "chorus", 50.0, 70.0, 3),
        ]
    }

    #[test]
    fn full_single_section_uses_occurrence() {
        assert_eq!(loop_name(30.0, 50.0, &song(), &[]), "verse 2");
    }

    #[test]
    fn first_occurrence_is_one() {
        assert_eq!(loop_name(0.0, 10.0, &song(), &[]), "intro 1");
    }

    #[test]
    fn strict_subset_is_sub() {
        assert_eq!(loop_name(34.0, 46.0, &song(), &[]), "sub verse 2");
    }

    #[test]
    fn spans_multiple_names_first_and_last() {
        // verse2[30,50] fully + chorus[50,70] fully
        assert_eq!(loop_name(30.0, 70.0, &song(), &[]), "verse 2 → chorus 1");
    }

    #[test]
    fn partial_end_section_is_sub() {
        // starts at verse2.start, ends inside chorus
        assert_eq!(loop_name(30.0, 60.0, &song(), &[]), "verse 2 → sub chorus 1");
    }

    #[test]
    fn partial_start_section_is_sub() {
        assert_eq!(loop_name(40.0, 70.0, &song(), &[]), "sub verse 2 → chorus 1");
    }

    #[test]
    fn middle_sections_dropped() {
        // intro..chorus spanning everything: only first & last named
        assert_eq!(loop_name(0.0, 70.0, &song(), &[]), "intro 1 → chorus 1");
    }

    #[test]
    fn boundary_within_eps_reads_as_full() {
        assert_eq!(loop_name(30.02, 49.97, &song(), &[]), "verse 2");
    }

    #[test]
    fn no_section_falls_back_to_timestamp() {
        assert_eq!(loop_name(83.0, 105.2, &[], &[]), "riff 1:23.0–1:45.2");
    }

    #[test]
    fn collision_gets_numeric_suffix() {
        let existing = vec!["verse 2".to_string()];
        assert_eq!(loop_name(30.0, 50.0, &song(), &existing), "verse 2 (2)");
    }

    #[test]
    fn collision_skips_taken_suffixes() {
        let existing = vec!["verse 2".to_string(), "verse 2 (2)".to_string()];
        assert_eq!(loop_name(30.0, 50.0, &song(), &existing), "verse 2 (3)");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail to compile then fail**

Run: `cargo test -p practice naming`
Expected: compiles after Step 1's `pub mod naming;`; all `naming::tests::*` PASS (the implementation is included above). If any fail, fix `naming.rs` until green.

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/naming.rs crates/practice/src/lib.rs
git commit -m "feat(practice): dynamic loop-naming algorithm from sections"
```

---

## Task 2: `name_override` on the loop model + store

**Files:**
- Modify: `crates/practice/src/model.rs:43-51`
- Modify: `crates/practice/src/store.rs` (schema, NewLoop, insert/update/read)

- [ ] **Step 1: Add the field to the wire type**

In `crates/practice/src/model.rs`, change `LoopRegion`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopRegion {
    pub id: LoopId,
    pub song_id: SongId,
    pub name: String,
    /// Manual name pinned by the user; when `Some`, the dynamic namer leaves
    /// this loop alone. `None` means `name` is algorithm-derived.
    #[serde(default)]
    pub name_override: Option<String>,
    pub start: f64,
    pub end: f64,
    pub kind: LoopKind,
}
```

- [ ] **Step 2: Add schema V6 migration**

In `crates/practice/src/store.rs`, after the `SCHEMA_V5` const (line ~101) add:

```rust
/// v6: optional manual name override on loops (NULL = dynamic name).
const SCHEMA_V6: &str = "
ALTER TABLE loops ADD COLUMN name_override TEXT;
";
```

In `fn migrate`, after the `version < 5` block, add:

```rust
        if version < 6 {
            self.conn.execute_batch(SCHEMA_V6)?;
            self.conn.pragma_update(None, "user_version", 6)?;
        }
```

- [ ] **Step 3: Extend `NewLoop` and the loop read/write fns**

In `crates/practice/src/store.rs`, change `NewLoop`:

```rust
pub struct NewLoop<'a> {
    pub name: &'a str,
    pub name_override: Option<&'a str>,
    pub start: f64,
    pub end: f64,
    pub kind: LoopKind,
}
```

Replace `insert_loop`:

```rust
    pub fn insert_loop(&self, song_id: SongId, l: NewLoop) -> Result<LoopRegion> {
        let kind_json = serde_json::to_string(&l.kind)?;
        self.conn.execute(
            "INSERT INTO loops (song_id, name, name_override, start_secs, end_secs, kind_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![song_id.0, l.name, l.name_override, l.start, l.end, kind_json],
        )?;
        Ok(LoopRegion {
            id: LoopId(self.conn.last_insert_rowid()),
            song_id,
            name: l.name.to_owned(),
            name_override: l.name_override.map(str::to_owned),
            start: l.start,
            end: l.end,
            kind: l.kind,
        })
    }
```

Replace the row mapping in `loop_by_id` and `list_loops` (both build a `LoopRegion`) to read the new column. The SELECT becomes
`SELECT id, song_id, name, name_override, start_secs, end_secs, kind_json FROM loops ...`
and the closure body becomes:

```rust
            let kind_json: String = row.get(6)?;
            Ok(LoopRegion {
                id: LoopId(row.get(0)?),
                song_id: SongId(row.get(1)?),
                name: row.get(2)?,
                name_override: row.get(3)?,
                start: row.get(4)?,
                end: row.get(5)?,
                kind: serde_json::from_str(&kind_json).map_err(json_err)?,
            })
```

(Apply to both `loop_by_id` — single row — and `list_loops` — query_map.)

- [ ] **Step 4: Replace `update_loop` to carry name + override**

```rust
    /// Rename and/or move a loop in place; kind is untouched. `name` is the
    /// effective display name; `name_override` is the pinned manual name (NULL
    /// reverts to dynamic).
    pub fn update_loop(
        &self,
        id: LoopId,
        name: &str,
        name_override: Option<&str>,
        start: f64,
        end: f64,
    ) -> Result<LoopRegion> {
        self.conn.execute(
            "UPDATE loops SET name = ?2, name_override = ?3, start_secs = ?4, end_secs = ?5
             WHERE id = ?1",
            params![id.0, name, name_override, start, end],
        )?;
        self.loop_by_id(id)?.ok_or(crate::error::Error::NotFound)
    }
```

- [ ] **Step 5: Add a store roundtrip test**

In `crates/practice/src/store.rs` tests module (find `mod tests`; if none exists for loops, add this test to the existing tests module), add:

```rust
    #[test]
    fn loop_override_roundtrips() {
        let store = Store::open_in_memory().unwrap();
        let song = store
            .insert_song(NewSong {
                title: "t",
                artist: None,
                path: "p",
                file_hash: "h",
                duration_secs: 100.0,
            })
            .unwrap();
        let l = store
            .insert_loop(
                song.id,
                NewLoop {
                    name: "verse 1",
                    name_override: None,
                    start: 0.0,
                    end: 10.0,
                    kind: LoopKind::Manual,
                },
            )
            .unwrap();
        assert_eq!(l.name_override, None);
        let pinned = store
            .update_loop(l.id, "my name", Some("my name"), 0.0, 10.0)
            .unwrap();
        assert_eq!(pinned.name_override.as_deref(), Some("my name"));
        let back = store.list_loops(song.id).unwrap();
        assert_eq!(back[0].name_override.as_deref(), Some("my name"));
    }
```

- [ ] **Step 6: Build & test (callers still broken — that's expected; fix them in Task 3)**

Run: `cargo test -p practice`
Expected: `practice` tests PASS. (`practice` has no callers of `insert_loop`/`update_loop` outside itself, so it compiles. The `server` crate breaks until Task 3.)

- [ ] **Step 7: Commit**

```bash
git add crates/practice/src/model.rs crates/practice/src/store.rs
git commit -m "feat(practice): name_override column + NewLoop/update_loop plumbing"
```

---

## Task 3: Server wiring — compute names, fit-to-section command

**Files:**
- Modify: `crates/server/src/app.rs` (dispatch arm, `loop_create`, `loop_update`, `section_replace`, `quick_rate`, new `loop_fit`)

- [ ] **Step 1: Add the naming helper**

In `crates/server/src/app.rs`, add a method on the `impl App` block (near the loop handlers, ~line 1560). It pulls sections + the other loops' names and calls the pure namer:

```rust
    /// Effective dynamic name for a loop on this song, disambiguated against
    /// every *other* loop's name. `exclude` is the loop being (re)named.
    fn auto_name_loop(
        &self,
        song_id: SongId,
        start: f64,
        end: f64,
        exclude: Option<LoopId>,
    ) -> Result<String, String> {
        let sections = self.store.list_sections(song_id).err_str()?;
        let existing: Vec<String> = self
            .store
            .list_loops(song_id)
            .err_str()?
            .into_iter()
            .filter(|l| Some(l.id) != exclude)
            .map(|l| l.name)
            .collect();
        Ok(practice::naming::loop_name(start, end, &sections, &existing))
    }
```

- [ ] **Step 2: Rewire `loop_create` to compute the name (ignore any client name)**

Replace the body of `loop_create` (lines ~1561-1583). The `P` struct drops `name`:

```rust
    fn loop_create(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            start: f64,
            end: f64,
        }
        let p: P = from_params(p)?;
        let name = self.auto_name_loop(p.song_id, p.start, p.end, None)?;
        let l = self
            .store
            .insert_loop(
                p.song_id,
                NewLoop {
                    name: &name,
                    name_override: None,
                    start: p.start,
                    end: p.end,
                    kind: LoopKind::Manual,
                },
            )
            .err_str()?;
        self.write_sidecar_for(p.song_id);
        serde_json::to_value(l).err_str()
    }
```

- [ ] **Step 3: Rewire `loop_update` for override vs. dynamic semantics**

Replace `loop_update` (lines ~1586-1611). `name` semantics: `Some(non-empty)` → pin override; `Some("")` → clear override + recompute; `None` → bounds-only, recompute if not overridden.

```rust
    fn loop_update(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            loop_id: LoopId,
            name: Option<String>,
            start: Option<f64>,
            end: Option<f64>,
        }
        let p: P = from_params(p)?;
        let old = self
            .store
            .loop_by_id(p.loop_id)
            .err_str()?
            .ok_or_else(|| format!("loop not found: {}", p.loop_id.0))?;
        let start = p.start.unwrap_or(old.start);
        let end = p.end.unwrap_or(old.end);

        // Decide the override after this update:
        // - explicit non-empty name -> pin it
        // - explicit empty name      -> clear (revert to dynamic)
        // - no name field            -> keep whatever was pinned
        let override_after: Option<String> = match p.name {
            Some(ref n) if !n.trim().is_empty() => Some(n.trim().to_string()),
            Some(_) => None,
            None => old.name_override.clone(),
        };

        let name = match &override_after {
            Some(n) => n.clone(),
            None => self.auto_name_loop(old.song_id, start, end, Some(p.loop_id))?,
        };

        let updated = self
            .store
            .update_loop(p.loop_id, &name, override_after.as_deref(), start, end)
            .err_str()?;
        self.write_sidecar_for(old.song_id);
        serde_json::to_value(updated).err_str()
    }
```

- [ ] **Step 4: Add `loop_fit` (snap each edge to nearest section boundary, recompute)**

Add after `loop_update`:

```rust
    /// Snap each edge of a loop to the nearest section boundary, then recompute
    /// its dynamic name (a no-op on its name if it carries an override).
    fn loop_fit(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            loop_id: LoopId,
        }
        let p: P = from_params(p)?;
        let old = self
            .store
            .loop_by_id(p.loop_id)
            .err_str()?
            .ok_or_else(|| format!("loop not found: {}", p.loop_id.0))?;
        let sections = self.store.list_sections(old.song_id).err_str()?;
        // gather every section boundary, snap each edge to the nearest one
        let mut bounds: Vec<f64> = Vec::new();
        for s in &sections {
            bounds.push(s.start);
            bounds.push(s.end);
        }
        let snap = |t: f64| -> f64 {
            bounds
                .iter()
                .copied()
                .min_by(|a, b| {
                    (a - t).abs().partial_cmp(&(b - t).abs()).unwrap()
                })
                .unwrap_or(t)
        };
        let (mut start, mut end) = if bounds.is_empty() {
            (old.start, old.end)
        } else {
            (snap(old.start), snap(old.end))
        };
        if end <= start {
            // degenerate snap (both edges to same boundary) — leave as-was
            start = old.start;
            end = old.end;
        }
        let name = match &old.name_override {
            Some(n) => n.clone(),
            None => self.auto_name_loop(old.song_id, start, end, Some(p.loop_id))?,
        };
        let updated = self
            .store
            .update_loop(p.loop_id, &name, old.name_override.as_deref(), start, end)
            .err_str()?;
        self.write_sidecar_for(old.song_id);
        serde_json::to_value(updated).err_str()
    }
```

- [ ] **Step 5: Register the dispatch arm**

In the dispatch `match` (line ~373), add after `"loop.delete"`:

```rust
            "loop.fit" => self.loop_fit(p),
```

- [ ] **Step 6: Recompute non-overridden names on `section_replace`**

In `section_replace` (lines ~1468-1500), after `refresh_junctions` and before `write_sidecar_for`, recompute manual loops' dynamic names:

```rust
        let junctions = self.refresh_junctions(p.song_id, 2.0, 2.0)?;
        self.recompute_loop_names(p.song_id)?;
        self.write_sidecar_for(p.song_id);
```

Add the helper near `auto_name_loop`:

```rust
    /// Recompute the dynamic name of every non-overridden manual loop on the
    /// song (called when sections change). Overridden and junction loops are
    /// left untouched.
    fn recompute_loop_names(&mut self, song_id: SongId) -> Result<(), String> {
        let loops = self.store.list_loops(song_id).err_str()?;
        for l in &loops {
            if l.name_override.is_some() || !matches!(l.kind, LoopKind::Manual) {
                continue;
            }
            let existing: Vec<String> = loops
                .iter()
                .filter(|o| o.id != l.id)
                .map(|o| o.name.clone())
                .collect();
            let name = practice::naming::loop_name(
                l.start,
                l.end,
                &self.store.list_sections(song_id).err_str()?,
                &existing,
            );
            if name != l.name {
                self.store
                    .update_loop(l.id, &name, None, l.start, l.end)
                    .err_str()?;
            }
        }
        Ok(())
    }
```

- [ ] **Step 7: Fix `quick_rate`'s insert to compute a dynamic name**

In `quick_rate` (line ~795), the loop is saved with `region.name`. Route it through the namer so kept quick-loops match the scheme (the timestamp fallback still applies when there are no sections). Replace the `insert_loop` call:

```rust
        let name = self.auto_name_loop(region.song_id, region.start, region.end, None)?;
        let saved = self
            .store
            .insert_loop(
                region.song_id,
                NewLoop {
                    name: &name,
                    name_override: None,
                    start: region.start,
                    end: region.end,
                    kind: region.kind,
                },
            )
            .err_str()?;
```

- [ ] **Step 8: Build and test the workspace**

Run: `cargo test --workspace`
Expected: PASS. If `loop_create`'s old `name` param is referenced anywhere else, the compiler will point to it — there are no other callers (the frontend stops sending `name` in Task 5).

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/app.rs
git commit -m "feat(server): dynamic loop naming on create/update/section-change + loop.fit"
```

---

## Task 4: Frontend store plumbing

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`

- [ ] **Step 1: Add `name_override` to the wire type**

In `apps/desktop/src/lib/stores.ts`, extend `LoopRegion` (lines 32-39):

```typescript
export interface LoopRegion {
  id: number;
  song_id: number;
  name: string;
  name_override: string | null;
  start: number;
  end: number;
  kind: LoopKind;
}
```

- [ ] **Step 2: Add the `loopsOpen` request store**

Next to `sectionsOpen` (line ~292) add:

```typescript
/** Request: jump to the loops tab (e.g. after saving a loop). */
export const loopsOpen = writable(false);
```

- [ ] **Step 3: Replace `createLoop` (no client name) + add `saveLoop` and `fitLoop`**

Replace `createLoop` (lines 561-572) with:

```typescript
  /** Persist a loop for the current span; the server names it dynamically. */
  async createLoop(start: number, end: number): Promise<LoopRegion> {
    const open = get(openSong);
    if (!open) throw new Error("no song open");
    const l = await cmd<LoopRegion>("loop.create", {
      song_id: open.song.id,
      start,
      end,
    });
    await this.refreshLoops();
    return l;
  },

  /** Deliberate save from the waveform: persist + surface the loops tab.
   *  Does not change what's currently playing. */
  async saveLoop(start: number, end: number): Promise<LoopRegion> {
    const l = await this.createLoop(start, end);
    loopsOpen.set(true);
    return l;
  },

  /** Snap a loop's edges to the nearest section boundaries (renames it). */
  async fitLoop(loopId: number): Promise<void> {
    await cmd("loop.fit", { loop_id: loopId });
    await this.refreshLoops();
  },
```

- [ ] **Step 4: Build the frontend type-check**

Run: `cd apps/desktop && pnpm svelte-check 2>&1 | tail -20`
Expected: errors only in `Waveform.svelte` (still calls the old `createLoop(name, …)`) and any `loopName` usage — fixed in Tasks 6-7. `stores.ts` itself type-clean.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/stores.ts
git commit -m "feat(desktop): loopsOpen store, saveLoop/fitLoop, server-named createLoop"
```

---

## Task 5: App.svelte — switch to loops tab on request

**Files:**
- Modify: `apps/desktop/src/App.svelte`

- [ ] **Step 1: Import `loopsOpen`**

In the `stores` import block (lines 20-35), add `loopsOpen,` (keep alpha-ish order near `libraryCollapsed`).

- [ ] **Step 2: Add the effect mirroring `sectionsOpen`**

After the `sectionsOpen` effect (lines 81-86) add:

```svelte
  $effect(() => {
    if ($loopsOpen) {
      tab = "loops";
      loopsOpen.set(false);
    }
  });
```

- [ ] **Step 3: Type-check**

Run: `cd apps/desktop && pnpm svelte-check 2>&1 | tail -20`
Expected: no new errors in `App.svelte`.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/App.svelte
git commit -m "feat(desktop): jump to loops tab when loopsOpen is set"
```

---

## Task 6: Waveform.svelte — loop (transient) + save glyphs

**Files:**
- Modify: `apps/desktop/src/components/Waveform.svelte`

- [ ] **Step 1: Replace `autoLoopName` / `loopSelection` / `playSelection`**

Replace the three functions (lines 523-543) with two: `loopSelection` (transient — the old play behavior) and `saveSelection` (persist + open loops tab). Delete `autoLoopName` entirely.

```typescript
  /** Primary glyph: loop the selection now — transient, nothing saved. */
  async function loopSelection() {
    const sel = get(selection);
    if (!sel) return;
    await actions.setTransportLoop(sel.start, sel.end);
    await actions.seek(sel.start);
    await actions.play();
  }

  /** Secondary glyph: save the selection to the loops list (server names it),
   *  then surface the loops tab. Does not change playback. */
  async function saveSelection() {
    const sel = get(selection);
    if (!sel) return;
    await actions.saveLoop(sel.start, sel.end);
    selection.set(null);
  }
```

- [ ] **Step 2: Swap the glyph buttons**

Replace the two buttons (lines 701-702):

```svelte
      <button class="sa-btn" onclick={loopSelection} title="loop selection" aria-label="loop selection">⟳</button>
      <button class="sa-btn" onclick={saveSelection} title="save loop" aria-label="save loop">🖫</button>
```

(The save glyph `🖫` is the floppy-save symbol; if it renders poorly in testing, fall back to `+`.)

- [ ] **Step 3: Update the comment above the glyph constants**

The comment at lines 545-548 says "Loop/play glyph buttons". Change "play" → "save":

```typescript
  // Loop/save glyph buttons for the current selection. Placement is dynamic:
```

- [ ] **Step 4: Type-check + manual smoke**

Run: `cd apps/desktop && pnpm svelte-check 2>&1 | tail -20`
Expected: no errors in `Waveform.svelte`.

Manual (after a full build/run in Task 9): drag a selection → ⟳ starts looping immediately with no new list entry; 🖫 adds an entry and flips the right panel to the loops tab without changing playback.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/components/Waveform.svelte
git commit -m "feat(desktop): waveform loop glyph is transient; save glyph persists + opens loops"
```

---

## Task 7: Loops.svelte — fit-to-section + revert-to-dynamic rename

**Files:**
- Modify: `apps/desktop/src/components/Loops.svelte`

- [ ] **Step 1: Drop the positional `autoName`; rename reverts to dynamic on empty**

Replace `autoName` + `commitRename` (lines 10-23) with:

```typescript
  function startRename(id: number) {
    renamingId = id;
    renameValue = ""; // empty + submit reverts to the dynamic name
  }

  async function commitRename() {
    if (renamingId === null) return;
    const id = renamingId;
    renamingId = null;
    // empty string clears the override server-side (back to the dynamic name)
    await actions.updateLoop(id, { name: renameValue.trim() });
  }
```

Update the two `commitRename(i)` call sites (lines 50-52) to `commitRename()` (no arg):

```svelte
            onblur={() => commitRename()}
            onkeydown={(e) => {
              if (e.key === "Enter") commitRename();
              else if (e.key === "Escape") renamingId = null;
            }}
```

(The `{#each ... as l, i}` index `i` is now only used for the key; leave it.)

- [ ] **Step 2: Add a "fit to section" button per loop**

In the loop row, after the rename/name block and before the start input (after line 66, the `{/if}` that closes the name button), add a fit button. It only makes sense when the song has sections:

```svelte
        {#if $openSong.sections.length > 0}
          <Button variant="chip" onclick={() => actions.fitLoop(l.id)} title="snap edges to sections">fit</Button>
        {/if}
```

- [ ] **Step 3: Type-check**

Run: `cd apps/desktop && pnpm svelte-check 2>&1 | tail -20`
Expected: no errors in `Loops.svelte`.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/Loops.svelte
git commit -m "feat(desktop): loops tab fit-to-section + revert-to-dynamic rename"
```

---

## Task 8: Library.svelte — Enter submits the rename-track modal

**Files:**
- Modify: `apps/desktop/src/components/Library.svelte:111-117`

- [ ] **Step 1: Add Enter-to-submit on the modal inputs**

Replace the two `<label class="field">` inputs (lines 112-113) so Enter calls `saveRename`:

```svelte
  <label class="field">title <input bind:value={renameTitle} onkeydown={(e) => e.key === "Enter" && saveRename()} /></label>
  <label class="field">artist <input bind:value={renameArtist} onkeydown={(e) => e.key === "Enter" && saveRename()} /></label>
```

- [ ] **Step 2: Type-check**

Run: `cd apps/desktop && pnpm svelte-check 2>&1 | tail -20`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/Library.svelte
git commit -m "feat(desktop): Enter submits the rename-track form"
```

---

## Task 9: Full gate + manual verification

- [ ] **Step 1: Run the full suite + lint**

Run: `just check`
Expected: `cargo test --workspace`, `pnpm vitest run`, clippy `-D warnings`, `cargo fmt --check`, and `svelte-check` all PASS. Fix any fallout (most likely: `cargo fmt` formatting on the new Rust, or a stray reference to the removed `loopName`/positional auto-name).

- [ ] **Step 2: Manual verification in the running app**

Build & run (`just build && just run`, or `just dev`), open a song that has sections, and confirm:
- Drag across section headers → selection snaps to the outer section boundaries (already-existing lane-drag behavior — verify it still works).
- ⟳ on a selection → loops immediately; **no** new entry in the Loops tab.
- 🖫 on a selection → a new entry appears, named from the sections (e.g. `verse 2`, `verse 2 → chorus 1`), and the right panel switches to the loops tab; playback unchanged.
- Two saves of the same span → second is `… (2)`.
- Loops tab "fit" on a hand-drawn loop → edges snap to section boundaries and the name updates.
- Double-click a loop name, type a name, Enter → pinned; editing sections no longer renames it. Double-click, clear, Enter → reverts to the dynamic name.
- Edit a section name in the Sections tab → non-overridden loop names update to match.
- Library ✎ rename track → Enter submits.

- [ ] **Step 3: Note any deviations** and address before considering the feature done.

---

## Self-Review notes

- **Spec coverage:** transient loop glyph (T6) ✓; save glyph + loops tab (T5,T6) ✓; header-drag selection (pre-existing — verified in T9) ✓; dynamic naming rules + occurrence + ε + collision + fallback (T1) ✓; override via rename + Enter (T7) ✓; fit-to-section (T3,T7) ✓; recompute on bounds + on sections change (T3) ✓; data model `name_override` + V6 (T2) ✓; server source of truth (T3) ✓; rename-track Enter (T8) ✓.
- **Type consistency:** `NewLoop` carries `name_override` everywhere it's constructed (insert_loop callers: loop_create, quick_rate); `update_loop` signature `(id, name, name_override, start, end)` used in loop_update, loop_fit, recompute_loop_names; frontend `createLoop(start, end)` matches the new `loop.create` params (no `name`).
- **Pre-existing behavior:** the lane-drag-across-headers selection is already implemented (`Waveform.svelte` lane drag, lines 438-447) — no code task, only a verification step (T9).
