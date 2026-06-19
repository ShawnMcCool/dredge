# Portable song bundles

**Date:** 2026-06-19
**Status:** approved, ready for planning

## Problem

Compute stems and structure on a strong PC, copy the result to a drive, move it
to a weak PC that cannot run Demucs or the analysis pipeline, and have dredge
there load everything — audio, stems, beats/downbeats/sections, sections, loops,
notes — with no recomputation.

Today the precomputed heavy artifacts do *not* travel:

- **Audio** is referenced by an absolute `song.path`; the original file is never
  copied, so the path is meaningless on another machine.
- **Stems** live in a content-addressed cache `~/.local/share/dredge/stems/<file_hash>/{vocals,drums,bass,other}.wav`.
- **Analysis** (bpm/beats/downbeats/sections) is cached in the SQLite `analysis` table.
- **Notes** live in the SQLite `section_notes` table.

Only sections/loops travel, via the `<audio>.dredge.json` sidecar.

## Decision

A song becomes a **directory** — the directory *is* the song. The library is a
flat folder of these bundles. The bundle is **canonical storage**, not an export
format: import creates it, every edit writes into it, and moving a song to
another PC is copying its folder while dredge is closed. There is no export step
and no second source of truth.

The user zips folders himself if he wants a single file; the app does not pack
or unpack zips.

## Layout

```
<library root>/                       # default: dirs::audio_dir()/dredge
  <Artist - Title>/                   # slugified, "-2" suffix on collision
    dredge.json                       # manifest — the source of truth
    audio.<ext>                       # imported audio, copied in once
    stems/
      vocals.wav  drums.wav  bass.wav  other.wav
~/.local/share/dredge/
  dredge.db                           # settings ONLY (no song data)
  dredge.log                          # logs
```

- **Library root** defaults to `dirs::audio_dir()/dredge`. `dirs::audio_dir()`
  resolves `XDG_MUSIC_DIR` on Linux and `~/Music` on macOS/Windows. If it
  returns `None`, fall back to `$HOME/Music/dredge`. Overridable by a
  `library_root` setting (see Settings).
- **Bundle folder name** is human-readable — `slug("Artist - Title")`, with a
  `-2`, `-3`… suffix on collision. The content hash lives only inside
  `dredge.json` (for dedup); the folder name is cosmetic and may be renamed by
  the user without breaking anything (identity is the manifest, not the path).

## The manifest

`dredge.json` is the existing `Sidecar` struct, renamed `BundleManifest` and
extended:

```rust
struct BundleManifest {
    version: u32,
    song: Song,             // includes title, artist, file_hash, duration_secs
    sections: Vec<Section>,
    loops: Vec<LoopRegion>,
    notes: Vec<SectionNotes>,   // (label, NotesDoc) pairs — moved off the DB
    analysis: Option<Analysis>, // bpm/beats/downbeats/sections — moved off the DB
}
```

- `song.path` becomes the managed path to `audio.<ext>` inside the bundle, not
  an external reference.
- Written atomically (tmp + rename), the same discipline the old sidecar used.
- `version` starts at the next number after the current sidecar version.

## The DB's new role (2A)

SQLite keeps **only the `settings` table**. It is no longer a source of truth
for anything song-related. The `songs`, `sections`, `loops`, `analysis`,
`section_notes`, and `profiles` tables are removed from the song-storage path
(profiles are machine-local timing and do not travel; they may be dropped or
kept settings-side — see Open items).

On startup dredge scans `<library root>/*/dredge.json` into an **in-memory
index**. Manifests are tiny, so the scan is fast for a personal library. There
is no persisted index.

**IDs** (`SongId`, `SectionId`, `LoopId`) are assigned at import/creation time
and stored in the manifest, so they are stable across rescans and travel with
the bundle. They must be unique within a running library; since they are no
longer DB-autoincremented, generation moves to the import/creation code (e.g. a
monotonic counter seeded from the max id seen during the startup scan, or a
random/uuid-derived i64). A song's identity for dedup is its `file_hash`.

## Import flow

`song.import(path)`:

1. Hash the audio (existing heavy phase, outside the lock).
2. If a bundle with that `file_hash` already exists in the index, no-op (dedup).
3. Otherwise: create `<library root>/<slug>/`, **copy** the audio in as
   `audio.<ext>`, write an initial `dredge.json` (song + empty
   sections/loops/notes, no analysis).
4. The original source file is never read again.

`song.open` decodes `audio.<ext>` and `stems/*.wav` directly from the bundle —
no content-addressed cache lookup.

## Edit flow

- Sections / loops / notes edits mutate the in-memory model **and** rewrite that
  bundle's `dredge.json` atomically. Always-persisted; no save button.
- **Stem separation** (`stems.separate`) writes `stems/*.wav` into the bundle
  instead of the content-addressed cache.
- **Analysis** writes into the manifest instead of the `analysis` table.
- The heavy-command `*_phased` lock discipline is unchanged; the slow phases
  just target the bundle directory.

## Removals / one-time reset

- Delete `crates/practice/src/sidecar.rs` and its read/write call sites; the
  `Sidecar` struct migrates into `BundleManifest`.
- Retire the content-addressed `~/.local/share/dredge/stems/<hash>/` cache and
  the `stems_dir` / `stems_cache_dir` machinery; stems live in bundles.
- Remove the song-data SQLite tables and their `store.rs` accessors, leaving the
  `settings` table.
- **Wipe existing state** — delete the current `dredge.db` and `stems/`. The
  user does not care about current library state, so there is no migration code.

## Settings

Add a `library_root` setting (JSON value in the `settings` table), default
`dirs::audio_dir()/dredge` (fallback `$HOME/Music/dredge`), surfaced in the
settings tab. Changing it points dredge at a different library directory (e.g. a
synced drive); existing bundles are **not** moved automatically.

## Out of scope / YAGNI

- App-driven zip pack/unpack (user zips folders himself).
- Migrating the existing library (wiped instead).
- Moving bundles when `library_root` changes.
- Profile-run history portability (machine-local timing data).

## Open items for planning

- Exact ID-generation scheme (monotonic-from-scan vs random i64).
- Fate of the `profiles` table (drop vs keep in settings-side DB).
- Whether `notes` in the manifest is `Vec<(label, NotesDoc)>` or a map.
- Frontend: the settings tab gains a library-root field; library list now comes
  from the bundle scan, not a DB query (wire shapes unchanged, source changed).
