# Library management — delete, rename, force re-analyze

Date: 2026-06-13

## Problem

Earworm can import, open, and annotate tracks, but offers no way to **remove** a
track from the library. `Store::delete_song` exists with full `ON DELETE
CASCADE`, yet it is wired to no dispatch command, so no client can reach it. Two
secondary gaps: there is no way to **edit a track's title/artist**, and no way
to **discard and regenerate** a track's cached analysis when it comes out wrong.

A related concern raised the request: generated data should persist until a
track is deleted, so an analyzed track loads fast without recomputation.
Investigation shows this already holds — analysis is cached in the SQLite
`analysis` table and returned directly by `song.open`; waveform peaks are cached
at `~/.cache/earworm/peaks/<file_hash>.json`; stems at
`<stems_dir>/<file_hash>/`. The architecture already persists generated data.
The missing half is **deletion that cleans all of it up** — those off-DB caches
are keyed by `file_hash`, outside the DB's cascade, so a naive delete orphans
them on disk.

## Goals

- Expose track deletion that leaves **no orphaned rows or files** anywhere.
- Allow editing a track's title/artist.
- Allow forcing a re-analysis that discards and regenerates cached analysis.
- Never touch the user's original audio file.

## Non-goals

- Bulk operations (multi-select delete, etc.).
- Re-deriving junction loops automatically after a re-analyze (see Open
  Questions — resolved: left manual).
- Any change to how generated data is *produced* or *cached* — only its
  removal and a forced regeneration.

## Design

### Backend commands (`crates/server/src/app.rs`)

Three additions, following existing `song.*` / `analysis.*` naming.

**`song.delete`** `{song_id}` — full removal, detailed below.

**`song.update`** `{song_id, title, artist?}` — edits metadata. Updates the
`songs` row, rewrites the sidecar via the existing `write_sidecar_for` (so
title/artist survive a future re-import), emits `library_changed`. Response
carries the updated `Song` so a client showing it as the open song can refresh
its header.

**`analysis.run`** gains an optional **`force: bool`** (default `false`). When
true, it skips the existing "already cached → return `cached`" short-circuit and
re-runs analysis. On success the existing `save_analysis` upsert overwrites the
prior row; a failed re-run leaves the prior analysis intact. No new command and
no `delete_analysis` method are required.

### Deletion — no orphans (`song.delete`)

Capture the song's `file_hash` and `path` from the row **before** deleting it,
then remove, in order:

1. **DB rows** — `Store::delete_song(id)`. Already cascades to `sections`,
   `loops`, `plans`, `reps`, `resurfacing`, and `analysis` via `ON DELETE
   CASCADE`. No schema change.
2. **Peaks cache** — `~/.cache/earworm/peaks/<file_hash>.json`. Add
   `engine::peaks::remove_cache(file_hash)` so path logic stays owned by the
   peaks module (mirrors `load_or_compute`).
3. **Stems cache** — `<stems_dir>/<file_hash>/` via `std::fs::remove_dir_all`;
   path from the existing `stems_cache_dir(file_hash)`.
4. **Sidecar** — `<audio_path>.earworm.json`. Add
   `practice::sidecar::remove_sidecar(audio_path)`.
5. **Source audio file** — **never touched.**

All filesystem removals are **best-effort**: a missing or locked file logs to
stderr but does not fail the command. The DB is the source of truth, matching
how `write_sidecar_for` already treats sidecar IO as non-fatal. A missing file
(e.g. peaks never generated, no sidecar written) is a clean no-op, not an error.

After the sweep, emit `library_changed`.

**Open-song safety:** if the deleted song is the currently open song
(`self.open_song`), first send `EngineCmd::Pause` and set `self.open_song =
None` before/around the delete, so playback of a now-deleted track stops.

### Frontend (`apps/desktop/src`)

**Store actions** (`lib/stores.ts`, mirroring `deleteLoop` / `importSong`):

- `deleteSong(id)` → `cmd("song.delete", {song_id})`. If `id` is the open song,
  clear the `openSong` store (UI returns to empty state). Then `refreshSongs()`.
  The `library_changed` event already refreshes other clients.
- `updateSong(id, title, artist)` → `cmd("song.update", ...)`; `refreshSongs()`,
  and if it is the open song, patch `openSong`'s `song`.
- `reanalyze()` → the existing analysis-run action with `force: true`.

**UI placement:**

- **Library list rows** (`Library.svelte`): each row is currently one
  open-on-click button. Add hover-revealed actions on the right — a **rename**
  (✎) and **delete** (✕) affordance — keeping the row a one-tap open while
  management stays unobtrusive. **Delete is confirmation-gated** (it removes
  practice history); reuse `ExitModal` or a lightweight inline confirm. Rename
  opens a small inline edit / minimal modal for title + artist.
- **Re-analyze** lives with the open song's analysis affordance (wherever
  `analysis.run` is triggered today), as a "discard and regenerate" action,
  **confirmation-gated**.

### Testing

- **Store (Rust, in-memory DB, `store.rs` tests):** `delete_song` removes the
  song and all cascaded rows (assert `sections`/`loops`/`plans`/`reps`/
  `resurfacing`/`analysis` gone). `song.update` (or the store setter it calls)
  persists title/artist.
- **Cleanup units:** `engine::peaks::remove_cache` and
  `practice::sidecar::remove_sidecar` each delete an existing file and no-op
  cleanly on a missing one (tempdir-based, matching the existing
  `cache_roundtrip` and sidecar tests).
- **App-level (`app.rs`):** `song.delete` on the open song clears `open_song`;
  `analysis.run {force:true}` re-runs past a cached row.
- **Frontend (vitest):** `deleteSong` clears `openSong` when deleting the open
  track; `updateSong` patches the open song's metadata. Mock `cmd` as
  `ipc.test.ts` does.

## Open questions (resolved)

- **Cleanup scope on delete:** remove regenerable caches (peaks, stems) **and**
  the sidecar; keep the user's source audio file. *Resolved: yes.*
- **Re-derive junctions after a forced re-analyze?** A forced re-analyze can
  change downbeats, leaving junction loops snapped to the old grid stale until
  re-derived. *Resolved: do not mutate the user's loops behind their back;
  re-deriving stays the manual action that already exists.*
