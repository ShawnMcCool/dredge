# Bundle folder tracks song name — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On rename, the bundle directory on disk tracks the displayed `Title — Artist`, and the open song is reloaded from its new location.

**Architecture:** `Library::update_song` renames the bundle dir (atomic `fs::rename`, collision-disambiguated, no-op when the slug is unchanged) then writes the manifest through. `song.update` becomes a phased server command: under the lock it guards against in-flight stems/analysis jobs and does the fast disk work, then the open song's audio re-decode runs off-lock and finishes via the same `finish_open` path `song.open` uses.

**Tech Stack:** Rust (practice + server crates), `cargo test`.

Spec: `docs/superpowers/specs/2026-06-20-bundle-folder-tracks-name-design.md`

---

## File structure

- **`crates/practice/src/library.rs`** — `update_song` gains the dir rename + guards; new `rebase_audio_path` helper shared with `load`.
- **`crates/server/src/app.rs`** — new `update_apply` (guard + library call + reopen intent); `song_update` becomes the inline reopen fallback; new `update_phased` in `dispatch_shared`.

---

### Task 1: Library renames the bundle dir on update

**Files:**
- Modify: `crates/practice/src/library.rs` (`update_song` at ~389; `load` rebase at ~57-59)
- Test: `crates/practice/src/library.rs` (test module)

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn update_renames_bundle_dir_and_rebases_path() {
    let src_dir = tempfile::tempdir().unwrap();
    let lib_dir = tempfile::tempdir().unwrap();
    let audio_src = src_dir.path().join("orig.flac");
    std::fs::write(&audio_src, b"FAKEAUDIO").unwrap();

    let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
    let song = lib
        .create_song(&audio_src, "Old Title", Some("Old Artist"), "h1", 30.0)
        .unwrap();
    let old_dir = lib_dir.path().join("Old Title \u{2014} Old Artist");
    assert!(old_dir.is_dir());

    let updated = lib
        .update_song(song.id, "New Title", Some("New Artist"))
        .unwrap();

    let new_dir = lib_dir.path().join("New Title \u{2014} New Artist");
    assert!(new_dir.is_dir(), "renamed bundle dir should exist");
    assert!(!old_dir.exists(), "old bundle dir should be gone");
    assert!(new_dir.join("audio.flac").exists(), "audio moved with the dir");
    // path rebased onto the new dir
    assert_eq!(updated.path, new_dir.join("audio.flac").to_string_lossy());
    assert_eq!(lib.bundle_dir(song.id).unwrap(), new_dir);
    // manifest on disk reflects the rename
    let m = bundle::read_manifest(&new_dir).unwrap();
    assert_eq!(m.song.title, "New Title");
    assert_eq!(m.song.artist.as_deref(), Some("New Artist"));
}

#[test]
fn update_disambiguates_on_name_collision() {
    let src_dir = tempfile::tempdir().unwrap();
    let lib_dir = tempfile::tempdir().unwrap();
    let a = src_dir.path().join("a.flac");
    let b = src_dir.path().join("b.flac");
    std::fs::write(&a, b"A").unwrap();
    std::fs::write(&b, b"B").unwrap();

    let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
    lib.create_song(&a, "Taken", None, "ha", 1.0).unwrap();
    let song2 = lib.create_song(&b, "Other", None, "hb", 1.0).unwrap();

    // rename song2 onto the name song1 already holds
    let updated = lib.update_song(song2.id, "Taken", None).unwrap();

    let disambiguated = lib_dir.path().join("Taken-2");
    assert!(disambiguated.is_dir(), "collision disambiguates to -2");
    assert_eq!(lib.bundle_dir(song2.id).unwrap(), disambiguated);
    assert_eq!(updated.path, disambiguated.join("audio.flac").to_string_lossy());
}

#[test]
fn update_with_unchanged_slug_leaves_dir_in_place() {
    let src_dir = tempfile::tempdir().unwrap();
    let lib_dir = tempfile::tempdir().unwrap();
    let audio_src = src_dir.path().join("orig.flac");
    std::fs::write(&audio_src, b"X").unwrap();

    let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
    let song = lib.create_song(&audio_src, "Same", Some("Band"), "h", 1.0).unwrap();
    let dir = lib_dir.path().join("Same \u{2014} Band");
    let path_before = lib.bundle_dir(song.id).unwrap();

    // re-applying the identical title/artist must not move the dir
    let updated = lib.update_song(song.id, "Same", Some("Band")).unwrap();

    assert_eq!(lib.bundle_dir(song.id).unwrap(), path_before);
    assert!(dir.is_dir());
    assert_eq!(updated.path, dir.join("orig.flac").to_string_lossy());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p practice library::tests::update_`
Expected: FAIL — current `update_song` doesn't rename the dir, so `new_dir` assertions fail.

- [ ] **Step 3: Add the `rebase_audio_path` helper and rewrite `update_song`**

Add this associated helper (near `persist`, ~line 194):

```rust
/// Point a song's audio path at `dir`, keeping only the file name. The stored
/// path may be stale — a bundle copied from another machine, or a dir just
/// renamed — and the file name is all that's portable.
fn rebase_audio_path(dir: &Path, song: &mut Song) {
    if let Some(fname) = Path::new(&song.path).file_name() {
        song.path = dir.join(fname).to_string_lossy().into_owned();
    }
}
```

Replace `update_song` (~389) with:

```rust
pub fn update_song(&mut self, id: SongId, title: &str, artist: Option<&str>) -> Result<Song> {
    let root = self.root.clone();
    let entry = self.entry_mut(id)?;

    // Rename the bundle dir first so the folder tracks the displayed name.
    // fs::rename is atomic within the library root; on failure nothing else
    // has changed, so we bail with disk and in-memory state untouched.
    let slug = bundle::slug(title, artist);
    let moved = entry.dir.file_name().and_then(|n| n.to_str()) != Some(slug.as_str());
    if moved {
        let dest = bundle::unique_bundle_dir(&root, &slug);
        std::fs::rename(&entry.dir, &dest)?;
        entry.dir = dest;
    }

    // Update metadata, rebase the audio path onto the (possibly new) dir, and
    // write the manifest through to disk.
    entry.manifest.song.title = title.to_owned();
    entry.manifest.song.artist = artist.map(str::to_owned);
    if moved {
        Self::rebase_audio_path(&entry.dir, &mut entry.manifest.song);
    }
    Self::persist(entry)?;
    Ok(entry.manifest.song.clone())
}
```

- [ ] **Step 4: DRY the `load` rebase through the new helper**

In `load` (~57-59), replace:

```rust
            if let Some(fname) = Path::new(&m.song.path).file_name() {
                m.song.path = dir.join(fname).to_string_lossy().into_owned();
            }
```

with:

```rust
            Self::rebase_audio_path(&dir, &mut m.song);
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p practice library::tests::update_`
Expected: PASS (all three).

- [ ] **Step 6: Commit**

```bash
git add crates/practice/src/library.rs
git commit -m "feat(library): rename bundle dir to track song name on update"
```

---

### Task 2: Guard the rename and reopen the open song (phased)

**Files:**
- Modify: `crates/server/src/app.rs` (`song_update` ~1100; `dispatch_shared` match ~51; add `update_phased` near `open_phased` ~62)
- Test: `crates/server/src/app.rs` (test module)

- [ ] **Step 1: Write the failing test**

Add to the `app.rs` test module (mirror existing `App` test setup — find how a test `App` is built nearby and reuse it). The test asserts the rename is rejected while a song is "analyzing":

```rust
#[test]
fn rename_rejected_while_analysis_running() {
    let mut app = test_app();              // existing helper in this test module
    let song = import_test_song(&mut app); // existing helper; returns the Song

    app.analyzing.insert(song.id.0);

    let err = app
        .update_apply(json!({ "song_id": song.id, "title": "New", "artist": "X" }))
        .unwrap_err();
    assert!(err.contains("running"), "got: {err}");

    // nothing changed: the bundle dir keeps its original name
    let dir = app.song_bundle_dir(song.id).unwrap();
    assert_eq!(dir.file_name().unwrap().to_str().unwrap(), song_slug(&song));
}
```

> If the test module has no `test_app`/`import_test_song`/`song_slug` helpers, adapt to whatever construction the existing `app.rs` tests use (e.g. build the `App` the same way the nearest existing test does, and assert on `app.song_bundle_dir` / `bundle::slug`). Keep the two assertions: error mentions "running", and the dir is unchanged.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p server app::tests::rename_rejected_while_analysis_running`
Expected: FAIL — `update_apply` does not exist yet.

- [ ] **Step 3: Add `update_apply` and rewrite the inline `song_update`**

Replace `song_update` (~1100-1125) with these two methods:

```rust
/// State phase of `song.update`: refuse while a job for this song is running,
/// rename the bundle (via the library) so the folder tracks the new name, and
/// report whether the renamed song is the open one (so the caller reopens it).
fn update_apply(&mut self, p: Value) -> Result<(Song, Option<SongId>), String> {
    #[derive(Deserialize)]
    struct P {
        song_id: SongId,
        title: String,
        // omitted artist clears it — socket/script clients can send {title} alone
        #[serde(default)]
        artist: Option<String>,
    }
    let p: P = from_params(p)?;

    // A rename moves the bundle dir; a stems/analysis job for this song
    // captured the old path up front and writes into it from another thread.
    // Moving the dir under it would silently lose its output, so refuse.
    if self.analyzing.contains(&p.song_id.0)
        || self.separating.lock().unwrap().contains(&p.song_id.0)
    {
        return Err("can't rename while stems or analysis are running for this song".into());
    }

    let song = self
        .library
        .update_song(p.song_id, &p.title, p.artist.as_deref())
        .err_str()?;
    let _ = self.job_tx.send(Event {
        event: "library_changed".into(),
        data: Value::Null,
    });

    let reopen =
        (self.open_song.as_ref().map(|o| o.song.id) == Some(p.song_id)).then_some(p.song_id);
    Ok((song, reopen))
}

fn song_update(&mut self, p: Value) -> Result<Value, String> {
    // Inline fallback (direct App::dispatch): decode under the lock, the same
    // accepted tradeoff as the inline `song_open`. The pump path uses the
    // phased `update_phased` instead.
    let (song, reopen) = self.update_apply(p)?;
    if let Some(song_id) = reopen {
        let (s, stems_cache) = self.open_lookup(song_id)?;
        let decoded = open_decode(&s, &stems_cache)?;
        self.finish_open(s, decoded)?;
    }
    serde_json::to_value(song).err_str()
}
```

- [ ] **Step 4: Add the phased `update_phased` and route to it**

Add next to `open_phased` (~62):

```rust
fn update_phased(app: &Arc<Mutex<App>>, p: Value) -> Result<Value, String> {
    let (song, reopen) = app.lock().unwrap().update_apply(p)?;
    if let Some(song_id) = reopen {
        // Reopen the renamed song with the heavy decode off-lock, exactly like
        // `open_phased`, so the pump never waits behind it.
        let (s, stems_cache) = app.lock().unwrap().open_lookup(song_id)?;
        let decoded = open_decode(&s, &stems_cache)?;
        app.lock().unwrap().finish_open(s, decoded)?;
    }
    serde_json::to_value(song).err_str()
}
```

In the `dispatch_shared` match (~51-54), add the route:

```rust
    let phased = match req.cmd.as_str() {
        "song.open" => open_phased(app, req.params),
        "song.import" => import_phased(app, req.params),
        "song.update" => update_phased(app, req.params),
        _ => return app.lock().unwrap().dispatch(req),
    };
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p server app::tests::rename_rejected_while_analysis_running`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/app.rs
git commit -m "feat(server): phase song.update, guard rename against in-flight jobs"
```

---

### Task 3: Full verification

- [ ] **Step 1: Run the whole gate**

Run: `just check`
Expected: `cargo test --workspace` + `pnpm vitest run` pass; clippy clean (`-D warnings`); fmt clean; svelte-check clean.

- [ ] **Step 2: Manual smoke (human checklist — webview isn't chrome-debuggable)**

Build and run, then in the UI: rename a song's title and artist; confirm the
folder under the library root is renamed to `New Title — New Artist` and still
plays; rename a song to a name another song holds and confirm a `-2` folder;
start stem separation and confirm a rename is refused while it runs.
