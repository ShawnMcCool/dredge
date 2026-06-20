# Bundle folder tracks the song name

## Problem

Renaming a song's title/artist in the library edits only the manifest. The
bundle directory on disk keeps the slug it was assigned at import
(`Library::update_song` → `persist` rewrites `dredge.json` in place and never
touches the folder). Because import frequently guesses artist/title wrong, the
user corrects the name in the UI and the folder is then permanently stale: the
filesystem no longer matches what Dredge shows, with no in-app way to fix it.

## Goal

On rename, the bundle directory tracks the displayed `Title — Artist`. The
filesystem stays browsable and self-describing. Renames are rare, so cost is
secondary to correctness and simplicity.

## Approach

Treat a rename as **close → mutate disk → reopen**, reloading from disk rather
than hand-patching in-memory state. This reuses the existing scan/rebase path
and the existing phased-open path, so there is no bespoke in-memory mutation to
get subtly wrong.

`song.update` becomes a **phased command** (like `song.open` / `song.import`):
the lock holds only for the guard + the fast disk work; the multi-second audio
re-decode of the reopen runs off-lock so the tick pump and other clients never
wait behind it.

Rejected alternatives:

- **Lazy/derived dir name** (compute the folder from the manifest on demand):
  the dir *is* the index identity and the place jobs write; making it derived
  ripples through `bundle_dir`, the jobs, and scan for no gain.
- **Background reconcile job**: async folder moves out from under the user/jobs;
  more moving parts than the problem warrants.
- **Client-driven reopen** (frontend re-issues `song.open`): minimal backend
  change but splits the flow across client/server and leaves raw-socket/daemon
  callers pointing at a closed song. Server-side phasing is atomic for every
  client.

## Flow

`song.update` carries `{song_id, title, artist?}` (unchanged wire shape).

1. **Guard (under lock).** If `self.separating` or `self.analyzing` contains the
   song id, reject the whole command with a clear error
   (`"can't rename while stems/analysis are running for this song"`) and change
   nothing. These jobs captured the old absolute path up front
   (`app.rs:755`, analysis equivalent) and write into it from a spawned thread;
   moving the dir under them would silently lose their output. This is the only
   case that causes data loss, so it is a hard reject (chosen over deferring or
   half-applying the rename).

2. **Disk mutation (under lock — fast IO only).** In `Library`:
   - Compute the new slug via the existing `bundle::slug(title, artist)` (already
     handles empty → `untitled` and slash/control sanitization, so a blank or
     junk rename cannot produce a broken folder).
   - **No-op guard:** if the new slug equals the current folder's `file_name`,
     skip the directory rename entirely.
   - **Collision guard:** otherwise pick the destination with the existing
     `bundle::unique_bundle_dir(root, slug)` so renaming onto a name another
     bundle already holds disambiguates to `-2`/`-3` rather than clobbering it.
     (Because the no-op guard already returned when the slug matched our own
     folder, `unique_bundle_dir` never collides with self.)
   - `std::fs::rename(old_dir, dest)` — atomic within the library root's
     filesystem. **Ordering:** rename the directory *first*; only on success
     update the manifest's `title`/`artist` and `write_manifest` into the new
     dir. If the rename fails, abort with disk and manifest untouched and return
     the error — no partial state.
   - Rebuild the in-memory `Entry` from disk so it matches byte-for-byte,
     including the audio-path rebase that `Library::load` performs
     (`library.rs:57`). Extract that rebase into a small shared helper
     (e.g. `load_entry(dir) -> Result<Entry>`) and call it from both `load` and
     here, rather than duplicating the logic.

3. **Reopen (phased, off-lock).** Mirror `open_phased` (`app.rs:62`):
   - Phase 1 returns from the lock whether the renamed song was the currently
     open song, plus the data the decode needs (`song`, `stems_cache`).
   - If it was open: run `open_decode(&song, &stems_cache)` with no lock held
     (the heavy phase), then take the lock again and `finish_open(song,
     decoded)` — the same call `song.open` uses, which installs the audio and
     emits `song_opened`.
   - If it was not the open song: no decode phase; nothing to reopen.
   The user has accepted that this resets playback, playhead, and workspace
   state for the open song — the visible reload is the point.

4. **Notify.** Emit `library_changed` as today (and `song_opened` falls out of
   `finish_open` when a reopen happened).

## Components touched

- **`crates/practice/src/library.rs`**
  - `update_song` gains the slug-compute, no-op/collision guards, `fs::rename`,
    manifest write into the new dir, and entry rebuild. Returns the updated
    `Song` (and enough for the caller to know the dir moved, if needed).
  - New `load_entry(dir) -> Result<Entry>` helper (manifest read + path rebase),
    called from both `load` and `update_song`.
- **`crates/server/src/app.rs`**
  - Add `"song.update"` to the `dispatch_shared` match with an `update_phased`
    function shaped like `open_phased`: lock→guard+disk-rename→(off-lock
    decode)→lock+finish_open.
  - Keep an inline `song_update` in `App::dispatch` for the non-shared path
    (matching how `song.open` is inlined), so direct `dispatch` callers still
    work; it may decode under the lock, the same accepted fallback as open.
  - The job-in-flight guard reads the existing `separating`/`analyzing` sets.

## Error handling

- Job in flight → reject, no changes (metadata included).
- `fs::rename` failure → reject, no changes (rename is attempted before the
  manifest write, so a failure leaves the original folder and manifest intact).
- Slug helper guarantees a non-empty, filesystem-safe name; collision helper
  guarantees no clobber.

## Testing

- **`library.rs` unit tests:**
  - rename moves the directory and rebases `song.path` onto the new dir.
  - rename to a slug another bundle already holds disambiguates (`-2`), no
    clobber.
  - no-op rename (slug unchanged) leaves the directory in place, updates
    metadata.
  - simulated `fs::rename` failure leaves dir + manifest unchanged. *(If a
    rename failure can't be induced portably in a unit test, cover the ordering
    by asserting the manifest is only written into the destination dir.)*
- **`app.rs` test:** rename is rejected when the song id is in `separating` /
  `analyzing`, and nothing is mutated.
- **Reopen:** an `app.rs` test that renaming the currently-open song reissues a
  decode and the open song's `path` points at the new dir.

## Out of scope

- Reconciling already-stale folders from past renames in bulk (a one-time
  migration could be added later; the rename path fixes them as they're edited).
- Renaming the audio *file* inside the bundle (`audio.<ext>` stays as-is; only
  the directory tracks the name).
