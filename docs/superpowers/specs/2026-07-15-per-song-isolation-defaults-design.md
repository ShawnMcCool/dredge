# Remember isolation settings per song — design

**Date:** 2026-07-15
**Status:** approved, pre-implementation

## Problem

When you set up the isolation box for a song — bass focus, stem fader levels,
mute/solo toggles — that state is lost the moment you open another song and
comes back. Reopening a song resets isolation to default. The user wants each
song to remember its isolation state and reload the way it was left.

## Core idea

The isolation box's full state becomes a per-song field in the bundle manifest
(`dredge.json`, the canonical per-song store). It is saved on every isolation
edit and restored verbatim on open. This is **pure persistence**: the engine's
bass-focus filter and per-stem gains are already driven live by the existing
`bass_focus` and `stems.gains` commands. This feature only remembers those
settings and replays them on open — it adds no new audio path.

Fidelity: **full isolation state**, including the mute/solo *toggles* — not just
the resolved sound. A stem left soloed comes back soloed (highlighted), not as
bare fader levels.

## Data model (Rust, `crates/practice/src/model.rs`)

New struct, deliberately distinct from the existing `Mix` (which folds mute/solo
into resolved gains and therefore cannot restore the toggles):

```rust
/// Saved per-song isolation-box state: the bass-focus toggle plus each stem's
/// fader level, mute, and solo. Restored verbatim on `song.open`. Distinct from
/// `Mix` (resolved gains) because it preserves the mute/solo toggles, not just
/// the resulting sound.
pub struct Isolation {
    pub bass_focus: bool,
    pub levels: Vec<u8>,   // 0..=100 per stem, STEM_NAMES order
    pub mutes: Vec<bool>,
    pub solos: Vec<bool>,
}
```

`Default` = bass focus off, all faders 100, nothing muted or soloed — today's
fresh-open state.

Stored as `Vec` and **normalized to `STEM_COUNT` on read** — pad missing stems
to full (level 100, unmuted, unsoloed), truncate extras. This is the same
forward-compat approach as `stems_compat`: a state saved under the old 4-stem
vocabulary loads correctly under the current 6-stem vocabulary.

## Manifest (`crates/practice/src/bundle.rs`)

Add to `BundleManifest`:

```rust
#[serde(default)]
pub isolation: Isolation,
```

`#[serde(default)]` means every existing `dredge.json` with no `isolation` field
decodes to `Default` — behaves exactly as today, no migration needed.

## Library (`crates/practice/src/library.rs`)

Mirror the `set_recordings` pattern (mutate the in-memory manifest, atomic
rewrite):

- `set_isolation(&mut self, song_id, iso: Isolation) -> Result<()>`
- `get_isolation(&self, song_id) -> Isolation` — returns the **normalized**
  state (padded/truncated to `STEM_COUNT`), or `Default` if the song has none.

## Dispatch (`crates/server/src/app.rs`)

- New command `"isolation.set"` with params
  `{ song_id, bass_focus, levels, mutes, solos }` → `library.set_isolation`.
  Light (a small manifest rewrite), so no lock-phasing. It does **not** touch the
  engine — the live gains are already applied by `stems.gains` / `bass_focus`.
- Add `isolation` to the `song.open` payload via `get_isolation`.

## Frontend (`apps/desktop/src/lib/stores.ts`)

- `OpenSong` interface gains an `isolation` field.
- Two pure helpers with colocated vitest tests:
  - `isolationToStemMix(iso) -> StemMix`
  - `stemMixToIsolation(mix, bassFocus) -> Isolation`
- **On `openSong`:** replace `stemMix.set(defaultStemMix())` with a load from
  `data.isolation` into the `stemMix` and `bassFocus` stores, then push to the
  engine — `stems.gains` (only when `data.stems`) and the `bass_focus` command.
  This also fixes a latent bug: today `bassFocus` is never reset on open, so it
  can leak from the previously open song.
- **On edit:** a debounced (~350 ms trailing) `persistIsolation()` sends
  `isolation.set` with the live `bassFocus` + `stemMix`. Wired into
  `setStemLevel`, `toggleStemMute`, `toggleStemSolo`, `bassFocus`, and
  `resetStemMix`. A fader drag therefore writes the manifest once, when it
  settles, not once per tick.
- **Routines excluded by construction:** `applyRoutineMix` writes the stores
  directly, never through these action methods, so routine playback never
  overwrites the saved default. `resetWorkspace` already leaves isolation
  untouched.

## Behavior decisions

- **No save button, no undo.** Auto-save on every edit, consistent with notes,
  loops, and sections.
- **Reset stems (⟲) persists the cleared state** as the new default: it calls
  `resetStemMix`, which persists, so reopening comes back clean.

## Testing

**Rust:**
- `Isolation` serde round-trip.
- Normalization: 4-entry save pads to 6 at full; over-length truncates.
- `BundleManifest` decodes to `Isolation::default()` when the field is absent.
- `set_isolation` / `get_isolation` write and read back through `dredge.json`.

**Frontend (vitest):**
- `isolationToStemMix` / `stemMixToIsolation` round-trip.
- Restore maps `Isolation` into the `stemMix` + `bassFocus` stores correctly.
- `persistIsolation` fires on the five actions and does **not** fire on a
  routine-driven mix change.

## Edge cases

- **Song without stems:** only bass focus is audible; stem state is saved
  harmlessly and, on restore, is not pushed to a stemless engine (`applyStemMix`
  already no-ops without stems).
- **Copied bundle:** `isolation` rides inside `dredge.json`, so it is portable
  like sections, loops, and notes — the folder loads the same on another
  machine.
- **Stem-vocabulary change:** normalization fills any newly added stems at full
  level, unmuted.
```
