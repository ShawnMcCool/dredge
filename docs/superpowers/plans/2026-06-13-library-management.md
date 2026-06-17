# Library Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add track deletion (with full orphan cleanup), metadata rename, and a forced re-analyze to Dredge's library.

**Architecture:** Three new/extended dispatch commands on `server::app::App` (`song.delete`, `song.update`, `analysis.run { force }`) backed by a new `Store::update_song` and two best-effort filesystem cleanup helpers (`engine::peaks::remove_cache`, `practice::sidecar::remove_sidecar`). The DB cascade already handles relational rows; the new work is sweeping the off-DB caches keyed by `file_hash` and the sidecar keyed by audio path, plus stopping playback when the open song is deleted. The frontend gets three store actions and unobtrusive row affordances.

**Tech Stack:** Rust (rusqlite, serde_json), Svelte 5 + Tauri, vitest, `cargo test`.

---

## File Structure

- `crates/practice/src/store.rs` — add `update_song`. (Modify)
- `crates/practice/tests/store.rs` — test `update_song`. (Modify)
- `crates/practice/src/sidecar.rs` — add `remove_sidecar` + test. (Modify)
- `crates/engine/src/peaks.rs` — add `remove_cache` + test. (Modify)
- `crates/server/src/app.rs` — add `song.delete`, `song.update` dispatch arms + handlers; add `force` to `analysis.run`. (Modify)
- `crates/server/tests/app_library.rs` — test delete + update through the dispatcher. (Modify)
- `crates/server/tests/app_analysis.rs` — test forced re-run. (Modify)
- `apps/desktop/src/lib/stores.ts` — add `deleteSong`, `updateSong`, `reanalyze` actions. (Modify)
- `apps/desktop/src/lib/library.test.ts` — vitest for the new store actions. (Create)
- `apps/desktop/src/components/Library.svelte` — per-row rename/delete affordances + confirm modal. (Modify)
- `apps/desktop/src/components/Sections.svelte` — re-analyze button. (Modify)

---

## Task 1: `Store::update_song`

**Files:**
- Modify: `crates/practice/src/store.rs` (after `delete_song`, ~line 232)
- Test: `crates/practice/tests/store.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/practice/tests/store.rs` (uses the existing `store_with_song` helper):

```rust
#[test]
fn update_song_changes_title_and_artist() {
    let (store, song) = store_with_song();
    let updated = store
        .update_song(song.id, "New Title", Some("New Band"))
        .unwrap();
    assert_eq!(updated.title, "New Title");
    assert_eq!(updated.artist.as_deref(), Some("New Band"));
    // path and hash are untouched
    assert_eq!(updated.path, song.path);
    assert_eq!(updated.file_hash, song.file_hash);
    // persisted: a fresh list reflects the change
    let listed = store.list_songs().unwrap();
    assert_eq!(listed[0].title, "New Title");
    assert_eq!(listed[0].artist.as_deref(), Some("New Band"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p practice --test store update_song_changes_title_and_artist`
Expected: FAIL — `no method named update_song`.

- [ ] **Step 3: Write minimal implementation**

In `crates/practice/src/store.rs`, immediately after the `delete_song` method:

```rust
    pub fn update_song(&self, id: SongId, title: &str, artist: Option<&str>) -> Result<Song> {
        self.conn.execute(
            "UPDATE songs SET title = ?1, artist = ?2 WHERE id = ?3",
            params![title, artist, id.0],
        )?;
        let song = self.conn.query_row(
            "SELECT id, title, artist, path, file_hash, duration_secs
             FROM songs WHERE id = ?1",
            params![id.0],
            Self::song_from_row,
        )?;
        Ok(song)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p practice --test store update_song_changes_title_and_artist`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/practice/src/store.rs crates/practice/tests/store.rs
git commit -m "feat(practice): Store::update_song for metadata edits"
```

---

## Task 2: `sidecar::remove_sidecar`

**Files:**
- Modify: `crates/practice/src/sidecar.rs`
- Test: `crates/practice/src/sidecar.rs` (its `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add inside the `mod tests` block in `crates/practice/src/sidecar.rs` (reuses the existing `sample` helper):

```rust
    #[test]
    fn remove_sidecar_deletes_then_noops() {
        let dir = tempfile::tempdir().unwrap();
        let s = sample(dir.path());
        write_sidecar(&s).unwrap();
        let audio = Path::new(&s.song.path);
        assert!(sidecar_path(audio).exists());

        remove_sidecar(audio).unwrap();
        assert!(!sidecar_path(audio).exists());

        // a second remove on the now-missing file is a clean no-op
        remove_sidecar(audio).unwrap();
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p practice --lib sidecar::tests::remove_sidecar_deletes_then_noops`
Expected: FAIL — `cannot find function remove_sidecar`.

- [ ] **Step 3: Write minimal implementation**

In `crates/practice/src/sidecar.rs`, after `read_sidecar`:

```rust
/// Delete the sidecar for an audio file. A missing sidecar is a no-op, not
/// an error — deletion cleanup must not fail on an absent file.
pub fn remove_sidecar(audio_path: &Path) -> std::io::Result<()> {
    let path = sidecar_path(audio_path);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p practice --lib sidecar::tests::remove_sidecar_deletes_then_noops`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/practice/src/sidecar.rs
git commit -m "feat(practice): remove_sidecar for delete cleanup"
```

---

## Task 3: `peaks::remove_cache`

**Files:**
- Modify: `crates/engine/src/peaks.rs`
- Test: `crates/engine/src/peaks.rs` (its `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add inside `mod tests` in `crates/engine/src/peaks.rs`:

```rust
    #[test]
    fn remove_cache_deletes_then_noops() {
        let buf = SongBuffer {
            data: vec![0.2f32; FRAMES_PER_BUCKET * CHANNELS],
        };
        let hash = format!("rm-{}", std::process::id());
        load_or_compute(&buf, &hash).unwrap();
        let path = dirs::cache_dir()
            .unwrap()
            .join("dredge/peaks")
            .join(format!("{hash}.json"));
        assert!(path.exists());

        remove_cache(&hash).unwrap();
        assert!(!path.exists());

        // second remove on the missing file is a clean no-op
        remove_cache(&hash).unwrap();
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p engine peaks::tests::remove_cache_deletes_then_noops`
Expected: FAIL — `cannot find function remove_cache`.

- [ ] **Step 3: Write minimal implementation**

In `crates/engine/src/peaks.rs`, after `load_or_compute`:

```rust
/// Delete the cached peaks for a song hash. A missing cache (or no cache dir)
/// is a no-op — deletion cleanup must not fail on an absent file.
pub fn remove_cache(file_hash: &str) -> std::io::Result<()> {
    let Some(base) = dirs::cache_dir() else {
        return Ok(());
    };
    let path = base.join("dredge/peaks").join(format!("{file_hash}.json"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p engine peaks::tests::remove_cache_deletes_then_noops`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/peaks.rs
git commit -m "feat(engine): peaks::remove_cache for delete cleanup"
```

---

## Task 4: `song.update` dispatch command

**Files:**
- Modify: `crates/server/src/app.rs` (dispatch match ~line 338; new handler in the `--- library ---` section ~line 1117)
- Test: `crates/server/tests/app_library.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/server/tests/app_library.rs`:

```rust
#[test]
fn update_changes_metadata_and_syncs_sidecar() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": id}));
    // create a loop so a sidecar exists to be rewritten
    req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "name": "x", "start": 0.0, "end": 1.0}),
    );

    let updated = req(
        &mut app,
        "song.update",
        json!({"song_id": id, "title": "Renamed", "artist": "New Band"}),
    );
    assert_eq!(updated["title"], "Renamed");
    assert_eq!(updated["artist"], "New Band");

    // persisted in the library list
    let listed = req(&mut app, "song.list", Value::Null);
    assert_eq!(listed[0]["title"], "Renamed");
    // sidecar reflects the new title
    let sc = practice::sidecar::read_sidecar(&wav).unwrap().unwrap();
    assert_eq!(sc.song.title, "Renamed");
    assert_eq!(sc.song.artist.as_deref(), Some("New Band"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p server --test app_library update_changes_metadata_and_syncs_sidecar`
Expected: FAIL — dispatch returns `ok:false` (`unknown command: song.update`), tripping the `req` assert.

- [ ] **Step 3: Add the dispatch arm**

In `crates/server/src/app.rs`, in the `match cmd` block, directly after the `"song.list" => ...` line, add **only** the `song.update` arm (the `song.delete` arm is added in Task 5, alongside its handler, so this task compiles on its own):

```rust
            "song.update" => self.song_update(p),
```

- [ ] **Step 4: Write the handler**

In the `// --- library ---` section of `crates/server/src/app.rs` (e.g. just after `song_import`), add:

```rust
    fn song_update(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            title: String,
            artist: Option<String>,
        }
        let p: P = from_params(p)?;
        let song = self
            .store
            .update_song(p.song_id, &p.title, p.artist.as_deref())
            .err_str()?;
        // keep the open song's header in sync if it's the one we renamed
        if let Some(o) = self.open_song.as_mut() {
            if o.song.id == p.song_id {
                o.song = song.clone();
            }
        }
        self.write_sidecar_for(p.song_id);
        let _ = self.job_tx.send(Event {
            event: "library_changed".into(),
            data: Value::Null,
        });
        serde_json::to_value(song).err_str()
    }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p server --test app_library update_changes_metadata_and_syncs_sidecar`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/app.rs crates/server/tests/app_library.rs
git commit -m "feat(server): song.update command for metadata edits"
```

---

## Task 5: `song.delete` dispatch command

**Files:**
- Modify: `crates/server/src/app.rs` (dispatch match; new handler in `--- library ---`)
- Test: `crates/server/tests/app_library.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/server/tests/app_library.rs`:

```rust
#[test]
fn delete_removes_song_clears_open_and_sweeps_sidecar() {
    let (mut app, _dir, wav) = test_app();
    let song = req(&mut app, "song.import", json!({"path": wav}));
    let id = song["id"].as_i64().unwrap();
    req(&mut app, "song.open", json!({"song_id": id}));
    // a loop write produces a sidecar next to the audio file
    req(
        &mut app,
        "loop.create",
        json!({"song_id": id, "name": "x", "start": 0.0, "end": 1.0}),
    );
    assert!(practice::sidecar::read_sidecar(&wav).unwrap().is_some());
    let _ = app.tick(); // drain the import's library_changed

    req(&mut app, "song.delete", json!({"song_id": id}));

    // gone from the library
    let listed = req(&mut app, "song.list", Value::Null);
    assert!(listed.as_array().unwrap().is_empty());
    // open song cleared (status reports a null song_id)
    let status = req(&mut app, "status", Value::Null);
    assert!(status["song_id"].is_null());
    // sidecar swept
    assert!(practice::sidecar::read_sidecar(&wav).unwrap().is_none());
    // the original audio file is untouched
    assert!(wav.exists());
    // library_changed announced
    let events = app.tick();
    assert!(
        events.iter().any(|e| e.event == "library_changed"),
        "expected library_changed in {events:?}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p server --test app_library delete_removes_song_clears_open_and_sweeps_sidecar`
Expected: FAIL — `unknown command: song.delete` trips the `req` assert.

- [ ] **Step 3: Add the dispatch arm**

In `crates/server/src/app.rs`, directly after the `"song.update" => self.song_update(p),` line added in Task 4:

```rust
            "song.delete" => self.song_delete(p),
```

- [ ] **Step 4: Write the handler**

In the `// --- library ---` section, after `song_update`:

```rust
    fn song_delete(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
        }
        let p: P = from_params(p)?;
        // capture path + hash before the row is gone — cleanup needs them
        let song = self.song_row(p.song_id)?;

        // stop playback and drop the handle if we're deleting the open song
        if self.open_song.as_ref().map(|o| o.song.id) == Some(p.song_id) {
            self.audio.send(EngineCmd::Pause);
            self.open_song = None;
        }

        // DB rows cascade (sections, loops, plans, reps, resurfacing, analysis)
        self.store.delete_song(p.song_id).err_str()?;

        // best-effort off-DB cleanup; the DB is the source of truth, so a
        // failed file removal logs but does not fail the command
        if let Err(e) = engine::peaks::remove_cache(&song.file_hash) {
            eprintln!("dredge: peaks cleanup failed for {}: {e}", song.file_hash);
        }
        let stems = self.stems_cache_dir(&song.file_hash);
        if let Err(e) = std::fs::remove_dir_all(&stems) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("dredge: stems cleanup failed for {}: {e}", song.file_hash);
            }
        }
        if let Err(e) = practice::sidecar::remove_sidecar(Path::new(&song.path)) {
            eprintln!("dredge: sidecar cleanup failed for {}: {e}", song.path);
        }

        let _ = self.job_tx.send(Event {
            event: "library_changed".into(),
            data: Value::Null,
        });
        Ok(Value::Null)
    }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p server --test app_library delete_removes_song_clears_open_and_sweeps_sidecar`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/app.rs crates/server/tests/app_library.rs
git commit -m "feat(server): song.delete with full off-DB cleanup"
```

---

## Task 6: `analysis.run { force }`

**Files:**
- Modify: `crates/server/src/app.rs` (`analysis_run`, ~line 1003)
- Test: `crates/server/tests/app_analysis.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/server/tests/app_analysis.rs` (reuses `setup`, `req`, `wait_for_progress`, `FakeAnalyzer`):

```rust
#[test]
fn force_reruns_past_the_cache() {
    let mut ctx = setup(Arc::new(FakeAnalyzer));

    req(&mut ctx.app, "analysis.run", json!({"song_id": ctx.song_id}));
    assert_eq!(wait_for_progress(&mut ctx.app)["state"], "done");

    // a plain run now short-circuits to cached
    let out = req(&mut ctx.app, "analysis.run", json!({"song_id": ctx.song_id}));
    assert_eq!(out["state"], "cached");

    // force bypasses the cache and re-runs
    let out = req(
        &mut ctx.app,
        "analysis.run",
        json!({"song_id": ctx.song_id, "force": true}),
    );
    assert_eq!(out["state"], "running");
    assert_eq!(wait_for_progress(&mut ctx.app)["state"], "done");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p server --test app_analysis force_reruns_past_the_cache`
Expected: FAIL — the forced run returns `cached` (assert on `"running"` fails), because `force` is ignored.

- [ ] **Step 3: Add the `force` flag**

In `crates/server/src/app.rs`, in `analysis_run`, change the params struct and the cache short-circuit:

```rust
        #[derive(Deserialize)]
        struct P {
            song_id: SongId,
            #[serde(default)]
            force: bool,
        }
        let p: P = from_params(p)?;
        let song = self.song_row(p.song_id)?;
        if !p.force && self.store.get_analysis(p.song_id).err_str()?.is_some() {
            return Ok(json!({"state": "cached"}));
        }
```

(Everything below the short-circuit — the `analyzing` guard, availability check, thread spawn — is unchanged. On success the existing `analysis_rx` drain calls `save_analysis`, which upserts and overwrites the prior row.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p server --test app_analysis force_reruns_past_the_cache`
Expected: PASS.

- [ ] **Step 5: Run the existing analysis tests to confirm no regression**

Run: `cargo test -p server --test app_analysis`
Expected: PASS (including `run_reports_done_then_caches`).

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/app.rs crates/server/tests/app_analysis.rs
git commit -m "feat(server): analysis.run force flag for re-analyze"
```

---

## Task 7: Frontend store actions

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts` (in the `export const actions = {` object)
- Create: `apps/desktop/src/lib/library.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/desktop/src/lib/library.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const cmdMock = vi.fn();
vi.mock("./ipc", () => ({
  cmd: (...args: unknown[]) => cmdMock(...args),
  onEvent: () => () => {},
  initialSong: () => null,
}));

import { actions, openSong } from "./stores";

beforeEach(() => {
  cmdMock.mockReset();
  cmdMock.mockResolvedValue(null);
  openSong.set(null);
});

describe("deleteSong", () => {
  it("sends song.delete, clears the open song, and refreshes the list", async () => {
    openSong.set({ song: { id: 5 } } as never);
    cmdMock.mockImplementation((name: string) =>
      name === "song.list" ? Promise.resolve([]) : Promise.resolve(null),
    );

    await actions.deleteSong(5);

    expect(cmdMock).toHaveBeenCalledWith("song.delete", { song_id: 5 });
    expect(get(openSong)).toBeNull();
    expect(cmdMock).toHaveBeenCalledWith("song.list");
  });

  it("leaves a different open song in place", async () => {
    openSong.set({ song: { id: 9 } } as never);
    cmdMock.mockImplementation((name: string) =>
      name === "song.list" ? Promise.resolve([]) : Promise.resolve(null),
    );

    await actions.deleteSong(5);

    expect(get(openSong)).not.toBeNull();
  });
});

describe("updateSong", () => {
  it("sends song.update and patches the open song's metadata", async () => {
    openSong.set({ song: { id: 5, title: "old", artist: null } } as never);
    cmdMock.mockImplementation((name: string) => {
      if (name === "song.update")
        return Promise.resolve({ id: 5, title: "new", artist: "B" });
      if (name === "song.list") return Promise.resolve([]);
      return Promise.resolve(null);
    });

    await actions.updateSong(5, "new", "B");

    expect(cmdMock).toHaveBeenCalledWith("song.update", {
      song_id: 5,
      title: "new",
      artist: "B",
    });
    expect(get(openSong)?.song.title).toBe("new");
  });
});

describe("reanalyze", () => {
  it("sends analysis.run with force for the open song", async () => {
    openSong.set({ song: { id: 7 } } as never);

    await actions.reanalyze();

    expect(cmdMock).toHaveBeenCalledWith("analysis.run", {
      song_id: 7,
      force: true,
    });
  });

  it("no-ops when nothing is open", async () => {
    await actions.reanalyze();
    expect(cmdMock).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/desktop && pnpm vitest run lib/library.test.ts`
Expected: FAIL — `actions.deleteSong is not a function`.

- [ ] **Step 3: Add the actions**

In `apps/desktop/src/lib/stores.ts`, inside the `export const actions = {` object (e.g. just after `importSong`), add:

```ts
  async deleteSong(id: number): Promise<void> {
    await cmd("song.delete", { song_id: id });
    if (get(openSong)?.song.id === id) openSong.set(null);
    await this.refreshSongs();
  },

  async updateSong(id: number, title: string, artist: string | null): Promise<void> {
    const song = await cmd<Song>("song.update", { song_id: id, title, artist });
    openSong.update((o) => (o && o.song.id === id ? { ...o, song } : o));
    await this.refreshSongs();
  },

  async reanalyze(): Promise<void> {
    const open = get(openSong);
    if (!open) return;
    // the analysis_progress event handler reloads the open song's analysis
    await cmd("analysis.run", { song_id: open.song.id, force: true });
  },
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd apps/desktop && pnpm vitest run lib/library.test.ts`
Expected: PASS (all six cases).

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/stores.ts apps/desktop/src/lib/library.test.ts
git commit -m "feat(desktop): deleteSong/updateSong/reanalyze store actions"
```

---

## Task 8: Library row affordances + re-analyze button

No vitest layer exists for `.svelte` components in this repo (tests are `lib/*.ts` only), so this task is verified by `svelte-check` + a manual run.

**Files:**
- Modify: `apps/desktop/src/components/Library.svelte`
- Modify: `apps/desktop/src/components/Sections.svelte`

- [ ] **Step 1: Add rename + delete affordances to Library rows**

Replace the `<script>` body additions and the `<li>` markup in `apps/desktop/src/components/Library.svelte`.

In the `<script lang="ts">` block, after the existing `openIt` function, add state + handlers:

```ts
  import Modal from "../lib/ui/Modal.svelte";

  let confirmDelete = $state<number | null>(null);
  let renaming = $state<number | null>(null);
  let renameTitle = $state("");
  let renameArtist = $state("");

  function startRename(id: number, title: string, artist: string | null) {
    renaming = id;
    renameTitle = title;
    renameArtist = artist ?? "";
  }

  async function saveRename() {
    if (renaming === null) return;
    error = "";
    try {
      await actions.updateSong(renaming, renameTitle.trim(), renameArtist.trim() || null);
      renaming = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function doDelete(id: number) {
    error = "";
    try {
      await actions.deleteSong(id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    confirmDelete = null;
  }
```

Replace the `<li>` block (lines ~46–62) with a row that keeps open-on-click and adds hover actions:

```svelte
    <li class="row">
      <button
        class="song"
        class:open={$openSong?.song.id === song.id}
        disabled={$openingSong !== null}
        onclick={() => openIt(song.id)}
      >
        <span class="title">
          {song.title}
          {#if $openingSong === song.id}<span class="opening mono">◌</span>{/if}
        </span>
        <span class="meta">
          {song.artist ?? ""}
          <span class="mono">{fmtDur(song.duration_secs)}</span>
        </span>
      </button>
      <span class="actions">
        <button class="act" title="rename" onclick={() => startRename(song.id, song.title, song.artist)}>✎</button>
        <button class="act" title="delete" onclick={() => (confirmDelete = song.id)}>✕</button>
      </span>
    </li>
```

After the `</ul>` (and before the import `<Button>`), add the two modals:

```svelte
<Modal open={confirmDelete !== null} title="delete track" closable onclose={() => (confirmDelete = null)}>
  <p>Remove this track and its loops, plans, ratings, and analysis? The source audio file is kept.</p>
  <div class="modal-actions">
    <Button onclick={() => (confirmDelete = null)}>cancel</Button>
    <Button accent onclick={() => confirmDelete !== null && doDelete(confirmDelete)}>delete</Button>
  </div>
</Modal>

<Modal open={renaming !== null} title="rename track" closable onclose={() => (renaming = null)}>
  <label class="field">title <input bind:value={renameTitle} /></label>
  <label class="field">artist <input bind:value={renameArtist} /></label>
  <div class="modal-actions">
    <Button onclick={() => (renaming = null)}>cancel</Button>
    <Button accent onclick={saveRename}>save</Button>
  </div>
</Modal>
```

Add styles inside the `<style>` block:

```css
  .row {
    display: flex;
    align-items: stretch;
  }
  .row .song {
    flex: 1;
    min-width: 0;
  }
  .actions {
    display: none;
    align-items: center;
    gap: 2px;
    padding-right: calc(var(--space) / 2);
  }
  .row:hover .actions {
    display: flex;
  }
  .act {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 2px 4px;
  }
  .act:hover {
    color: var(--accent);
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space);
    margin-top: var(--space);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: var(--space);
    font-size: 12px;
    color: var(--muted);
  }
```

- [ ] **Step 2: Add the re-analyze button to Sections**

In `apps/desktop/src/components/Sections.svelte`, add a handler in the `<script>` (after the existing imports/state):

```ts
  let confirmReanalyze = $state(false);

  async function reanalyze() {
    confirmReanalyze = false;
    await actions.reanalyze();
  }
```

In the controls row (near line 133–137, the `+ add` / `save` buttons), add a re-analyze trigger:

```svelte
    <Button onclick={() => (confirmReanalyze = true)}>re-analyze</Button>
```

After the `{#if $analysisError}` block, add the confirm modal (import `Modal` at the top if not already imported):

```svelte
<Modal open={confirmReanalyze} title="re-analyze" closable onclose={() => (confirmReanalyze = false)}>
  <p>Discard the cached beat grid and section suggestions and run analysis again?</p>
  <div class="modal-actions">
    <Button onclick={() => (confirmReanalyze = false)}>cancel</Button>
    <Button accent onclick={reanalyze}>re-analyze</Button>
  </div>
</Modal>
```

Add a `.modal-actions` style block to `Sections.svelte`'s `<style>` if not present:

```css
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space);
    margin-top: var(--space);
  }
```

- [ ] **Step 3: Typecheck the frontend**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: 0 errors (warnings about existing code are fine; no new ones in `Library.svelte` / `Sections.svelte`).

- [ ] **Step 4: Manual verification**

Run: `just dev`
Confirm:
- Hovering a library row reveals ✎ and ✕; clicking the row still opens the song.
- ✕ → confirm → the track disappears from the list; if it was open, the editor clears.
- ✎ → edit title/artist → save → the row updates; if open, the header updates.
- With a song open, **re-analyze** in the sections panel → confirm → analysis re-runs and suggestions refresh.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/components/Library.svelte apps/desktop/src/components/Sections.svelte
git commit -m "feat(desktop): library row rename/delete + re-analyze button"
```

---

## Task 9: Full gate

- [ ] **Step 1: Run the full suite + lint**

Run: `just check`
Expected: `cargo test --workspace` PASS, `pnpm vitest run` PASS, clippy clean, fmt clean, svelte-check clean.

- [ ] **Step 2: Fix anything the gate surfaces, then re-run**

Run: `just check`
Expected: all green.

- [ ] **Step 3: Final commit (only if fixes were needed)**

```bash
git add -A
git commit -m "chore: library management — pass full check gate"
```

---

## Notes carried from the design

- **Source audio is never deleted.** `song.delete` removes regenerable caches (peaks, stems) and the sidecar, but leaves the user's audio file in place — Task 5's test asserts `wav.exists()` after delete.
- **Re-analyze does not touch loops.** A forced re-analyze can change downbeats, leaving junction loops snapped to the old grid until the user re-derives them. This is intentional — re-deriving stays the existing manual action; we do not mutate the user's loops behind their back.
- **Persistence was already correct.** Analysis (SQLite `analysis` table), peaks (`~/.cache/dredge/peaks`), and stems (`<stems_dir>/<hash>`) already persist and are reused on open. This plan only adds their *removal* and a *forced regeneration*; it does not change how they are produced or cached.
