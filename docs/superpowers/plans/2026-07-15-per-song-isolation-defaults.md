# Per-Song Isolation Defaults Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remember each song's isolation-box state (bass focus + per-stem levels/mutes/solos) in its bundle manifest and restore it verbatim on open.

**Architecture:** A new `Isolation` struct persists in `BundleManifest`; the library exposes normalized get/set; a light `isolation.set` dispatch command persists on edit and `song.open` returns the saved state. The frontend loads it into the `bassFocus`/`stemMix` stores, pushes it to the engine via the existing `bass_focus`/`stems.gains` commands, and debounce-saves on every isolation edit. Pure persistence — no new audio path.

**Tech Stack:** Rust (serde, rusqlite-adjacent in-memory library index), Svelte 5 stores, Tauri dispatch, vitest, cargo test.

Spec: `docs/superpowers/specs/2026-07-15-per-song-isolation-defaults-design.md`

---

### Task 1: `Isolation` model type + normalization

**Files:**
- Modify: `crates/practice/src/model.rs`
- Test: `crates/practice/src/model.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing tests**

Add near the existing `Mix` tests (or a new `#[cfg(test)] mod isolation_tests`):

```rust
#[cfg(test)]
mod isolation_tests {
    use super::*;

    #[test]
    fn default_is_full_band_no_focus() {
        let i = Isolation::default();
        assert!(!i.bass_focus);
        assert_eq!(i.normalized().levels, vec![100; STEM_COUNT]);
        assert_eq!(i.normalized().mutes, vec![false; STEM_COUNT]);
        assert_eq!(i.normalized().solos, vec![false; STEM_COUNT]);
    }

    #[test]
    fn normalize_pads_short_to_stem_count() {
        let i = Isolation { bass_focus: true, levels: vec![10, 20, 30, 40], mutes: vec![true], solos: vec![] };
        let n = i.normalized();
        assert_eq!(n.levels, vec![10, 20, 30, 40, 100, 100]);
        assert_eq!(n.mutes, vec![true, false, false, false, false, false]);
        assert_eq!(n.solos, vec![false; STEM_COUNT]);
        assert!(n.bass_focus);
    }

    #[test]
    fn normalize_truncates_long_to_stem_count() {
        let i = Isolation { bass_focus: false, levels: vec![1; STEM_COUNT + 3], mutes: vec![true; STEM_COUNT + 2], solos: vec![true; STEM_COUNT + 1] };
        let n = i.normalized();
        assert_eq!(n.levels.len(), STEM_COUNT);
        assert_eq!(n.mutes.len(), STEM_COUNT);
        assert_eq!(n.solos.len(), STEM_COUNT);
    }

    #[test]
    fn serde_round_trip() {
        let i = Isolation { bass_focus: true, levels: vec![50; STEM_COUNT], mutes: vec![false; STEM_COUNT], solos: vec![true; STEM_COUNT] };
        let s = serde_json::to_string(&i).unwrap();
        let back: Isolation = serde_json::from_str(&s).unwrap();
        assert_eq!(i, back);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p practice isolation_tests`
Expected: FAIL — `Isolation` not found.

- [ ] **Step 3: Implement `Isolation`**

Add to `crates/practice/src/model.rs` near `Mix`:

```rust
/// Saved per-song isolation-box state: the bass-focus toggle plus each stem's
/// fader level, mute, and solo. Restored verbatim on `song.open`. Distinct from
/// `Mix` (resolved gains) because it preserves the mute/solo toggles, not just
/// the resulting sound. Stored as `Vec`s and normalized to `STEM_COUNT` on read
/// so a state saved under an older stem vocabulary still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Isolation {
    #[serde(default)]
    pub bass_focus: bool,
    #[serde(default)]
    pub levels: Vec<u8>,
    #[serde(default)]
    pub mutes: Vec<bool>,
    #[serde(default)]
    pub solos: Vec<bool>,
}

impl Default for Isolation {
    /// Full band, no listening aid — a freshly opened song's isolation state.
    fn default() -> Self {
        Self {
            bass_focus: false,
            levels: vec![100; STEM_COUNT],
            mutes: vec![false; STEM_COUNT],
            solos: vec![false; STEM_COUNT],
        }
    }
}

impl Isolation {
    /// Pad/truncate every vector to exactly `STEM_COUNT`: missing stems default
    /// to full level, unmuted, unsoloed; extras are dropped.
    pub fn normalized(&self) -> Isolation {
        fn fit<T: Clone>(v: &[T], fill: T) -> Vec<T> {
            let mut out = v.to_vec();
            out.truncate(STEM_COUNT);
            while out.len() < STEM_COUNT {
                out.push(fill.clone());
            }
            out
        }
        Isolation {
            bass_focus: self.bass_focus,
            levels: fit(&self.levels, 100),
            mutes: fit(&self.mutes, false),
            solos: fit(&self.solos, false),
        }
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p practice isolation_tests`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/practice/src/model.rs
git commit -m "feat(practice): Isolation model type with STEM_COUNT normalization"
```

---

### Task 2: Manifest field + library get/set

**Files:**
- Modify: `crates/practice/src/bundle.rs` (add field to `BundleManifest`)
- Modify: `crates/practice/src/library.rs` (get/set)
- Test: `crates/practice/src/library.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Add the manifest field**

In `crates/practice/src/bundle.rs`, in `struct BundleManifest`, after `routines`:

```rust
    #[serde(default)]
    pub isolation: Isolation,
```

Ensure `Isolation` is imported (the file already imports model types; add `Isolation` to that `use` list, e.g. `use crate::model::{..., Isolation, ...};`).

Also update any struct-literal construction of `BundleManifest` in this file (e.g. in `scan_library` around the `notes: vec![]` default site) to include `isolation: Isolation::default()`.

- [ ] **Step 2: Write the failing library test**

In `crates/practice/src/library.rs` test module (mirror an existing `set_recordings`/manifest test that builds a temp library), add:

```rust
#[test]
fn isolation_persists_and_reloads() {
    let (mut lib, song_id, dir) = /* existing helper that builds a one-song temp library */;

    let iso = Isolation {
        bass_focus: true,
        levels: vec![80, 0, 100, 100, 100, 50],
        mutes: vec![false, true, false, false, false, false],
        solos: vec![false, false, true, false, false, false],
    };
    lib.set_isolation(song_id, iso.clone()).unwrap();

    // in-memory
    assert_eq!(lib.get_isolation(song_id), iso.normalized());

    // on disk: re-scan the bundle dir and confirm the manifest carries it
    let m = crate::bundle::read_manifest(&dir).unwrap();
    assert_eq!(m.isolation, iso);
}

#[test]
fn isolation_defaults_when_absent() {
    let (lib, song_id, _dir) = /* fresh one-song temp library, no isolation set */;
    assert_eq!(lib.get_isolation(song_id), Isolation::default());
}
```

Adapt the `/* ... */` bindings to whatever helper the existing tests in this file use to construct a temp library + song (search for `set_recordings` or `write_manifest` in the test module and copy that setup). If no helper exists, build it inline the way the nearest existing test does.

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p practice isolation_persists`
Expected: FAIL — `set_isolation` / `get_isolation` not found.

- [ ] **Step 4: Implement get/set**

In `crates/practice/src/library.rs`, mirroring `set_recordings` (~line 449):

```rust
pub fn set_isolation(&mut self, song_id: SongId, iso: Isolation) -> Result<()> {
    let entry = self.entry_mut(song_id)?;
    entry.manifest.isolation = iso;
    Self::persist(entry)
}

pub fn get_isolation(&self, song_id: SongId) -> Isolation {
    self.entries
        .get(&song_id.0)
        .map(|e| e.manifest.isolation.normalized())
        .unwrap_or_default()
}
```

Add `Isolation` to the model imports at the top of `library.rs` if not already present.

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p practice isolation`
Expected: PASS (all isolation tests including Task 1's).

- [ ] **Step 6: Commit**

```bash
git add crates/practice/src/bundle.rs crates/practice/src/library.rs
git commit -m "feat(practice): persist Isolation in the bundle manifest"
```

---

### Task 3: `isolation.set` command + `song.open` payload

**Files:**
- Modify: `crates/server/src/app.rs`
- Test: `crates/server/src/app.rs` (inline `#[cfg(test)]` if the module has dispatch tests) — otherwise rely on the practice-layer tests and manual `just cmd` verification noted below.

- [ ] **Step 1: Add the params struct and handler**

Near the other param structs in `app.rs`, add:

```rust
#[derive(serde::Deserialize)]
struct IsolationSetParams {
    song_id: SongId,
    #[serde(default)]
    bass_focus: bool,
    #[serde(default)]
    levels: Vec<u8>,
    #[serde(default)]
    mutes: Vec<bool>,
    #[serde(default)]
    solos: Vec<bool>,
}
```

Add the handler method (mirroring `section_notes_set`):

```rust
fn isolation_set(&mut self, p: Value) -> Result<Value, String> {
    let p: IsolationSetParams = from_params(p)?;
    self.library
        .set_isolation(
            p.song_id,
            practice::model::Isolation {
                bass_focus: p.bass_focus,
                levels: p.levels,
                mutes: p.mutes,
                solos: p.solos,
            },
        )
        .map_err(err_str)?;
    Ok(json!({ "ok": true }))
}
```

(Match the exact error-collapse idiom used by neighboring handlers — `err_str` / `ErrStr` per `app.rs` convention.)

- [ ] **Step 2: Route the command**

In the `match` that dispatches commands (near `"section.notes.set" => self.section_notes_set(p)`), add:

```rust
            "isolation.set" => self.isolation_set(p),
```

- [ ] **Step 3: Add `isolation` to the `song.open` payload**

In `finish_open` (~line 2307), add to the `json!` map:

```rust
            "isolation": self.library.get_isolation(song_id),
```

- [ ] **Step 4: Build to verify it compiles**

Run: `cargo build -p server`
Expected: builds clean.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/app.rs
git commit -m "feat(server): isolation.set command + isolation in song.open payload"
```

---

### Task 4: Frontend pure helpers + tests

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts` (export `Isolation` type + two helpers)
- Test: `apps/desktop/src/lib/isolation.test.ts` (new) — OR colocate in an existing stores-adjacent test if that is the pattern

**Note:** the helpers are pure; if `stores.ts` is hard to import in vitest (Tauri seams), move the two helpers into a new `apps/desktop/src/lib/isolation.ts` and import them from `stores.ts`. Prefer the standalone module — it matches the `lib/*.ts` + colocated `*.test.ts` convention.

- [ ] **Step 1: Write the failing test** (`apps/desktop/src/lib/isolation.test.ts`)

```ts
import { describe, it, expect } from "vitest";
import { isolationToStemMix, stemMixToIsolation, type Isolation } from "./isolation";

const N = 6;

describe("isolation <-> stemMix", () => {
  it("round-trips full state", () => {
    const iso: Isolation = {
      bass_focus: true,
      levels: [80, 0, 100, 100, 100, 50],
      mutes: [false, true, false, false, false, false],
      solos: [false, false, true, false, false, false],
    };
    const mix = isolationToStemMix(iso);
    expect(mix.levels).toEqual(iso.levels);
    expect(mix.mutes).toEqual(iso.mutes);
    expect(mix.solos).toEqual(iso.solos);
    const back = stemMixToIsolation(mix, iso.bass_focus);
    expect(back).toEqual(iso);
  });

  it("pads a short saved state to the stem count", () => {
    const iso: Isolation = { bass_focus: false, levels: [10, 20, 30, 40], mutes: [], solos: [] };
    const mix = isolationToStemMix(iso);
    expect(mix.levels).toEqual([10, 20, 30, 40, 100, 100]);
    expect(mix.mutes).toEqual(Array(N).fill(false));
    expect(mix.solos).toEqual(Array(N).fill(false));
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd apps/desktop && pnpm vitest run lib/isolation.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `apps/desktop/src/lib/isolation.ts`**

```ts
import { STEM_LABELS, type StemMix } from "./stores";

export interface Isolation {
  bass_focus: boolean;
  levels: number[];
  mutes: boolean[];
  solos: boolean[];
}

/** Pad/truncate `v` to the stem count, filling missing entries with `fill`. */
function fit<T>(v: T[], fill: T): T[] {
  const out = v.slice(0, STEM_LABELS.length);
  while (out.length < STEM_LABELS.length) out.push(fill);
  return out;
}

export function isolationToStemMix(iso: Isolation): StemMix {
  return {
    levels: fit(iso.levels ?? [], 100),
    mutes: fit(iso.mutes ?? [], false),
    solos: fit(iso.solos ?? [], false),
  };
}

export function stemMixToIsolation(mix: StemMix, bassFocus: boolean): Isolation {
  return {
    bass_focus: bassFocus,
    levels: mix.levels.slice(),
    mutes: mix.mutes.slice(),
    solos: mix.solos.slice(),
  };
}
```

If importing `STEM_LABELS`/`StemMix` from `stores.ts` pulls Tauri into the test and breaks it, hardcode the stem count locally (`const STEM_COUNT = 6`) and define a local `StemMix` shape instead — keep the module import-light.

- [ ] **Step 4: Run to verify it passes**

Run: `cd apps/desktop && pnpm vitest run lib/isolation.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/isolation.ts apps/desktop/src/lib/isolation.test.ts
git commit -m "feat(ui): pure isolation<->stemMix helpers"
```

---

### Task 5: Wire restore + debounced persist into stores

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`

- [ ] **Step 1: Extend `OpenSong` and import helpers**

Add to the `OpenSong` interface (~line 165):

```ts
  isolation: import("./isolation").Isolation;
```

(or a top-of-file `import { isolationToStemMix, stemMixToIsolation, type Isolation } from "./isolation";` and use `isolation: Isolation`.)

- [ ] **Step 2: Add the debounced persist helper**

Near the stems actions (~line 1281), add a module-level debounce and a method:

```ts
let isolationPersistTimer: ReturnType<typeof setTimeout> | null = null;

// inside the actions object:
  /** Debounced save of the live isolation state to the open song's manifest.
   *  A fader drag thus writes once, when it settles — not once per tick. */
  persistIsolation(): void {
    const open = get(openSong);
    if (!open) return;
    const song_id = open.song.id;
    const iso = stemMixToIsolation(get(stemMix), get(bassFocus));
    if (isolationPersistTimer) clearTimeout(isolationPersistTimer);
    isolationPersistTimer = setTimeout(() => {
      isolationPersistTimer = null;
      void cmd("isolation.set", { song_id, ...iso });
    }, 350);
  },
```

- [ ] **Step 3: Call persist from the five edit actions**

Append `this.persistIsolation();` to each of: `setStemLevel`, `toggleStemMute`, `toggleStemSolo`, `resetStemMix` (after their `applyStemMix()`), and to `bassFocus` (after `cmd("bass_focus", ...)`). Example for `setStemLevel`:

```ts
  async setStemLevel(idx: number, level: number): Promise<void> {
    stemMix.update((m) => ({ ...m, levels: m.levels.map((v, i) => (i === idx ? level : v)) }));
    await this.applyStemMix();
    this.persistIsolation();
  },
```

- [ ] **Step 4: Restore on open**

In `openSong` (~line 775), replace `stemMix.set(defaultStemMix());` with:

```ts
      const iso = data.isolation ?? { bass_focus: false, levels: [], mutes: [], solos: [] };
      stemMix.set(isolationToStemMix(iso));
      bassFocus.set(iso.bass_focus);
      // push restored state to the engine (gains only meaningful with stems)
      if (data.stems) void cmd("stems.gains", { gains: this.stemGainsVector(get(stemMix)) });
      void cmd("bass_focus", { on: iso.bass_focus });
```

- [ ] **Step 5: Run frontend tests + typecheck**

Run: `cd apps/desktop && pnpm vitest run && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: tests PASS, svelte-check clean (0 errors).

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/lib/stores.ts
git commit -m "feat(ui): restore + debounce-persist per-song isolation state"
```

---

### Task 6: Full gate + runtime smoke + build

**Files:** none (verification).

- [ ] **Step 1: Full suite + lint**

Run: `just check`
Expected: `cargo test --workspace` + `pnpm vitest run` pass; clippy `-D warnings` clean; fmt clean; svelte-check clean.

- [ ] **Step 2: Runtime smoke (per ui-runtime-smoke-test memory)**

Run `just dev` (vite :5173) and verify in the browser: open a song, move a stem fader / mute / solo / toggle bass focus, open a different song, reopen the first — the isolation box comes back exactly as left. Confirm no `effect_update_depth_exceeded` in the console. (This seam touches the backend manifest, which the WebKitGTK app can't be driven headless — vite + a browser is the check.)

Alternatively verify persistence headlessly: `just cmd '{"id":1,"cmd":"isolation.set","params":{"song_id":<id>,"bass_focus":true,"levels":[50,0,100,100,100,100],"mutes":[false,true,false,false,false,false],"solos":[false,false,false,false,false,false]}}'` then `just cmd '{"id":2,"cmd":"song.open","params":{"song_id":<id>}}'` and confirm the `isolation` field in the response echoes it. Also inspect the song's `dredge.json` for the `isolation` block.

- [ ] **Step 3: Release build (per stale-release-binary memory)**

Run: `just build`
Expected: `target/release/dredge` + `dredged` built. Note for the user: restart the running app to pick up the new binary.

- [ ] **Step 4: No commit** (verification only). If smoke surfaced fixes, commit them with a descriptive message.
```
