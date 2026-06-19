# Portable Song Bundles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make each song a self-contained directory bundle (audio + stems + manifest) that is the canonical storage, so a song computed on one PC can be copied to a weaker PC and loaded with no recomputation.

**Architecture:** A song bundle is a directory `<library>/<slug>/` holding `dredge.json` (manifest: song, sections, loops, notes, analysis), `audio.<ext>`, and `stems/{vocals,drums,bass,other}.wav`. Bundles are the source of truth. SQLite keeps only the `settings` table. On startup dredge scans the library into an in-memory `Library` index; every edit rewrites the affected bundle's manifest atomically. Stems and analysis write into the bundle instead of a content-addressed cache and the DB.

**Tech Stack:** Rust (rusqlite, serde_json, symphonia), Svelte 5 + Tauri frontend, `dirs` crate for the music dir, `cargo test` + `pnpm vitest`.

**Reference spec:** `docs/superpowers/specs/2026-06-19-portable-song-bundles-design.md`

---

## File structure

New / changed files and their responsibilities:

- `crates/practice/src/bundle.rs` (**new**) — the `BundleManifest` type, slug,
  atomic manifest read/write, bundle creation (copy audio), and library scan.
  Replaces `sidecar.rs`.
- `crates/practice/src/library.rs` (**new**) — the in-memory `Library` index:
  loads all bundles, owns id generation, and is the mutation surface (each
  mutation rewrites a manifest). Replaces the song-data half of `Store`.
- `crates/practice/src/sidecar.rs` (**deleted**) — folded into `bundle.rs`.
- `crates/practice/src/store.rs` (**modified**) — reduced to the `settings`
  table (and `profiles`, kept settings-side). Song/section/loop/notes/analysis
  accessors and tables removed.
- `crates/practice/src/lib.rs` (**modified**) — module wiring.
- `crates/practice/src/model.rs` (**modified**) — `SectionNote` type.
- `crates/server/src/app.rs` (**modified**) — every `self.store.<song-data>`
  call becomes a `self.library.<...>` call; import copies audio + creates a
  bundle; open/export decode from the bundle; stems/analysis write into the
  bundle. `stems_dir`/`stems_cache_dir` removed.
- `crates/server/src/stems.rs`, `analysis.rs` (**modified**) — output paths
  point at the bundle.
- `apps/desktop/src/components/Settings.svelte` + `lib/stores.ts` (**modified**)
  — library-root field.

---

## Phase 0 — Reset and the library-root setting

### Task 0.1: Wipe current local state

Bundles are a clean break; existing DB + stems are discarded (per spec).

- [ ] **Step 1: Remove the current data**

```bash
rm -rf ~/.local/share/dredge/dredge.db ~/.local/share/dredge/dredge.db-shm \
       ~/.local/share/dredge/dredge.db-wal ~/.local/share/dredge/stems
```

Expected: those paths are gone; `analyze-venv` / `songformer-venv` remain.

- [ ] **Step 2: Confirm**

Run: `ls ~/.local/share/dredge`
Expected: no `dredge.db*`, no `stems/`.

(No commit — this is local environment only.)

### Task 0.2: Library-root resolution helper

**Files:**
- Create: `crates/practice/src/bundle.rs`
- Modify: `crates/practice/src/lib.rs`

- [ ] **Step 1: Register the module**

In `crates/practice/src/lib.rs` add (keep alphabetical with the existing `mod` lines):

```rust
pub mod bundle;
```

- [ ] **Step 2: Write the failing test**

Create `crates/practice/src/bundle.rs`:

```rust
use std::path::PathBuf;

/// Default library root: the OS music dir + `/dredge`, falling back to
/// `$HOME/Music/dredge` when no music dir is configured.
pub fn default_library_root() -> Option<PathBuf> {
    if let Some(music) = dirs::audio_dir() {
        return Some(music.join("dredge"));
    }
    dirs::home_dir().map(|h| h.join("Music").join("dredge"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_root_ends_with_dredge() {
        let root = default_library_root().expect("a home dir exists in CI");
        assert_eq!(root.file_name().unwrap(), "dredge");
    }
}
```

- [ ] **Step 3: Confirm `dirs` is a dependency of `practice`**

Run: `grep -n '^dirs' crates/practice/Cargo.toml`
If absent, add `dirs = "5"` under `[dependencies]` (the workspace already uses
`dirs` in `server`; match its version).

- [ ] **Step 4: Run the test**

Run: `cargo test -p practice bundle::tests::default_root_ends_with_dredge`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/practice/src/bundle.rs crates/practice/src/lib.rs crates/practice/Cargo.toml
git commit -m "feat(practice): library-root resolution helper"
```

### Task 0.3: `library_root` setting + settings accessor

The `settings` table already stores arbitrary JSON per key. The app reads the
library root from the setting, falling back to the default.

**Files:**
- Modify: `crates/server/src/app.rs` (where the App resolves its data dirs)

- [ ] **Step 1: Add a resolver on `App`**

Add a private helper (near `default_stems_dir`, which you will delete in Phase 3):

```rust
/// Library root: the `library_root` setting if set, else the OS default.
fn library_root(store: &practice::store::Store) -> PathBuf {
    if let Ok(Some(v)) = store.get_setting("library_root") {
        if let Some(s) = v.as_str() {
            if !s.trim().is_empty() {
                return PathBuf::from(s);
            }
        }
    }
    practice::bundle::default_library_root()
        .unwrap_or_else(|| PathBuf::from("dredge-library"))
}
```

- [ ] **Step 2: Confirm `get_setting` exists**

Run: `grep -n "fn get_setting\|fn all_settings\|fn set_setting" crates/practice/src/store.rs`
Expected: `all_settings` and `set_setting` exist. If `get_setting` does not,
add it:

```rust
pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
    let mut stmt = self
        .conn
        .prepare_cached("SELECT value_json FROM settings WHERE key = ?1")?;
    let json: Option<String> = stmt
        .query_row(params![key], |r| r.get(0))
        .optional()?;
    Ok(match json {
        Some(j) => Some(serde_json::from_str(&j)?),
        None => None,
    })
}
```

- [ ] **Step 3: Build**

Run: `cargo build -p server`
Expected: compiles (the helper is unused for now — allow with `#[allow(dead_code)]` if clippy complains; it is wired in Phase 3).

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/store.rs crates/server/src/app.rs
git commit -m "feat(server): resolve library root from settings"
```

---

## Phase 1 — The bundle module (pure, TDD)

### Task 1.1: `BundleManifest` type and `SectionNote`

**Files:**
- Modify: `crates/practice/src/model.rs`
- Modify: `crates/practice/src/bundle.rs`

- [ ] **Step 1: Add `SectionNote` to `model.rs`**

```rust
/// One section's notes, keyed by occurrence label (e.g. "verse 2"). Mirrors a
/// row of the old `section_notes` table; lives in the bundle manifest now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionNote {
    pub label: String,
    pub doc: crate::notes::NotesDoc,
}
```

- [ ] **Step 2: Write the failing test in `bundle.rs`**

```rust
use crate::model::{Analysis, LoopRegion, Section, SectionNote, Song};
use serde::{Deserialize, Serialize};

/// The bundle's `dredge.json` — the source of truth for one song.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub version: u32,
    pub song: Song,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub loops: Vec<LoopRegion>,
    #[serde(default)]
    pub notes: Vec<SectionNote>,
    #[serde(default)]
    pub analysis: Option<Analysis>,
}

pub const MANIFEST_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "dredge.json";

#[cfg(test)]
mod manifest_tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn manifest_json_roundtrips() {
        let m = BundleManifest {
            version: MANIFEST_VERSION,
            song: Song {
                id: SongId(1),
                title: "T".into(),
                artist: None,
                path: "/x/audio.flac".into(),
                file_hash: "h".into(),
                duration_secs: 10.0,
            },
            sections: vec![],
            loops: vec![],
            notes: vec![],
            analysis: None,
        };
        let bytes = serde_json::to_vec(&m).unwrap();
        let back: BundleManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(m, back);
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p practice bundle::manifest_tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/model.rs crates/practice/src/bundle.rs
git commit -m "feat(practice): BundleManifest type"
```

### Task 1.2: Slugify bundle folder names

**Files:**
- Modify: `crates/practice/src/bundle.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod slug_tests {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(slug("Radiohead", Some("Weird Fishes")), "Weird Fishes — Radiohead");
    }
    #[test]
    fn slug_no_artist() {
        assert_eq!(slug("untitled", None), "untitled");
    }
    #[test]
    fn slug_strips_path_separators() {
        assert_eq!(slug("AC/DC", Some("T.N.T.")), "T.N.T. — AC_DC");
    }
    #[test]
    fn slug_trims_and_collapses_blank() {
        assert_eq!(slug("   ", None), "untitled");
    }
}
```

- [ ] **Step 2: Implement `slug`**

```rust
/// A human-readable, filesystem-safe folder name: `Title — Artist` (or just
/// `Title`). Replaces path-hostile characters with `_`; never empty.
pub fn slug(title: &str, artist: Option<&str>) -> String {
    let base = match artist {
        Some(a) if !a.trim().is_empty() => format!("{} — {}", title.trim(), a.trim()),
        _ => title.trim().to_string(),
    };
    let cleaned: String = base
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    if cleaned.is_empty() {
        "untitled".to_string()
    } else {
        cleaned
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p practice bundle::slug_tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/bundle.rs
git commit -m "feat(practice): bundle folder slug"
```

### Task 1.3: Atomic manifest read/write

**Files:**
- Modify: `crates/practice/src/bundle.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod io_tests {
    use super::*;
    use crate::model::*;

    fn sample(dir: &std::path::Path) -> BundleManifest {
        BundleManifest {
            version: MANIFEST_VERSION,
            song: Song {
                id: SongId(7),
                title: "T".into(),
                artist: Some("A".into()),
                path: dir.join("audio.flac").to_string_lossy().into_owned(),
                file_hash: "h".into(),
                duration_secs: 12.0,
            },
            sections: vec![],
            loops: vec![],
            notes: vec![],
            analysis: None,
        }
    }

    #[test]
    fn write_then_read_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let m = sample(dir.path());
        write_manifest(dir.path(), &m).unwrap();
        let back = read_manifest(dir.path()).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn write_leaves_no_tmp_file() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), &sample(dir.path())).unwrap();
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![MANIFEST_FILE.to_string()]);
    }
}
```

- [ ] **Step 2: Implement**

```rust
use crate::error::Result;
use std::path::Path;

/// Write `dredge.json` into `bundle_dir` atomically (tmp + rename).
pub fn write_manifest(bundle_dir: &Path, m: &BundleManifest) -> Result<()> {
    std::fs::create_dir_all(bundle_dir)?;
    let path = bundle_dir.join(MANIFEST_FILE);
    let tmp = bundle_dir.join("dredge.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(m)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Read `dredge.json` from `bundle_dir`.
pub fn read_manifest(bundle_dir: &Path) -> Result<BundleManifest> {
    let bytes = std::fs::read(bundle_dir.join(MANIFEST_FILE))?;
    Ok(serde_json::from_slice(&bytes)?)
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p practice bundle::io_tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/bundle.rs
git commit -m "feat(practice): atomic manifest read/write"
```

### Task 1.4: Allocate a unique bundle directory

**Files:**
- Modify: `crates/practice/src/bundle.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod dir_tests {
    use super::*;

    #[test]
    fn unique_dir_suffixes_on_collision() {
        let root = tempfile::tempdir().unwrap();
        let a = unique_bundle_dir(root.path(), "Song");
        std::fs::create_dir_all(&a).unwrap();
        let b = unique_bundle_dir(root.path(), "Song");
        assert_eq!(a.file_name().unwrap(), "Song");
        assert_eq!(b.file_name().unwrap(), "Song-2");
    }
}
```

- [ ] **Step 2: Implement**

```rust
/// `root/<slug>`, or `root/<slug>-2`, `-3`… if earlier names already exist on
/// disk. Does not create the directory.
pub fn unique_bundle_dir(root: &Path, slug: &str) -> std::path::PathBuf {
    let base = root.join(slug);
    if !base.exists() {
        return base;
    }
    for n in 2..100_000 {
        let cand = root.join(format!("{slug}-{n}"));
        if !cand.exists() {
            return cand;
        }
    }
    base
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p practice bundle::dir_tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/bundle.rs
git commit -m "feat(practice): unique bundle directory allocation"
```

### Task 1.5: Scan the library into manifests

**Files:**
- Modify: `crates/practice/src/bundle.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod scan_tests {
    use super::*;
    use crate::model::*;

    fn write_one(root: &std::path::Path, id: i64, hash: &str) {
        let dir = root.join(format!("song{id}"));
        let m = BundleManifest {
            version: MANIFEST_VERSION,
            song: Song {
                id: SongId(id),
                title: format!("S{id}"),
                artist: None,
                path: dir.join("audio.flac").to_string_lossy().into_owned(),
                file_hash: hash.into(),
                duration_secs: 1.0,
            },
            sections: vec![],
            loops: vec![],
            notes: vec![],
            analysis: None,
        };
        write_manifest(&dir, &m).unwrap();
    }

    #[test]
    fn scan_loads_every_manifest() {
        let root = tempfile::tempdir().unwrap();
        write_one(root.path(), 1, "a");
        write_one(root.path(), 2, "b");
        // a stray non-bundle dir is ignored
        std::fs::create_dir_all(root.path().join("not-a-bundle")).unwrap();
        let mut found = scan_library(root.path()).unwrap();
        found.sort_by_key(|(_, m)| m.song.id.0);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].1.song.file_hash, "a");
        assert_eq!(found[1].1.song.file_hash, "b");
    }

    #[test]
    fn scan_missing_root_is_empty() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("nope");
        assert!(scan_library(&missing).unwrap().is_empty());
    }
}
```

- [ ] **Step 2: Implement**

```rust
/// Load every bundle under `root`. Returns `(bundle_dir, manifest)` pairs.
/// A directory without a readable `dredge.json` is skipped (not an error). A
/// missing root yields an empty list.
pub fn scan_library(root: &Path) -> Result<Vec<(std::path::PathBuf, BundleManifest)>> {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = entry.path();
        match read_manifest(&dir) {
            Ok(m) => out.push((dir, m)),
            Err(_) => continue, // not a bundle
        }
    }
    Ok(out)
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p practice bundle::scan_tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/bundle.rs
git commit -m "feat(practice): scan library into manifests"
```

---

## Phase 2 — The in-memory `Library` index

This replaces the song-data half of `Store`. It owns the manifests, generates
ids, and persists every mutation by rewriting the affected manifest.

### Task 2.1: `Library` load + listing + id generation

**Files:**
- Create: `crates/practice/src/library.rs`
- Modify: `crates/practice/src/lib.rs` (add `pub mod library;`)

- [ ] **Step 1: Write the failing test**

```rust
use crate::bundle::{self, BundleManifest};
use crate::error::Result;
use crate::model::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One loaded bundle: where it lives + its manifest.
struct Entry {
    dir: PathBuf,
    manifest: BundleManifest,
}

/// In-memory index over the bundle library. Source of truth is the manifests
/// on disk; this caches them and writes through on every mutation.
pub struct Library {
    root: PathBuf,
    /// keyed by SongId
    entries: HashMap<i64, Entry>,
    next_id: i64,
}

impl Library {
    /// Load every bundle under `root`. `next_id` continues past the largest id
    /// seen across songs, sections, and loops so generated ids never collide.
    pub fn load(root: PathBuf) -> Result<Self> {
        let mut entries = HashMap::new();
        let mut max_id = 0i64;
        for (dir, m) in bundle::scan_library(&root)? {
            max_id = max_id.max(m.song.id.0);
            max_id = m.sections.iter().fold(max_id, |a, s| a.max(s.id.0));
            max_id = m.loops.iter().fold(max_id, |a, l| a.max(l.id.0));
            entries.insert(m.song.id.0, Entry { dir, manifest: m });
        }
        Ok(Self { root, entries, next_id: max_id + 1 })
    }

    fn next_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn list_songs(&self) -> Vec<Song> {
        let mut v: Vec<Song> = self.entries.values().map(|e| e.manifest.song.clone()).collect();
        v.sort_by_key(|s| s.id.0);
        v
    }

    pub fn song_by_id(&self, id: SongId) -> Option<Song> {
        self.entries.get(&id.0).map(|e| e.manifest.song.clone())
    }

    pub fn song_by_hash(&self, hash: &str) -> Option<Song> {
        self.entries
            .values()
            .map(|e| &e.manifest.song)
            .find(|s| s.file_hash == hash)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_loads_clean() {
        let root = tempfile::tempdir().unwrap();
        let lib = Library::load(root.path().to_path_buf()).unwrap();
        assert!(lib.list_songs().is_empty());
        // first generated id starts at 1
        let mut lib = lib;
        assert_eq!(lib.next_id(), 1);
    }
}
```

Add to `crates/practice/src/lib.rs`:

```rust
pub mod library;
```

- [ ] **Step 2: Run**

Run: `cargo test -p practice library::tests::empty_root_loads_clean`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/practice/src/library.rs crates/practice/src/lib.rs
git commit -m "feat(practice): Library index load + listing"
```

### Task 2.2: Create a bundle (copy audio, write manifest)

**Files:**
- Modify: `crates/practice/src/library.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod create_tests {
    use super::*;

    #[test]
    fn create_copies_audio_and_indexes_song() {
        let root = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let audio = src.path().join("orig.flac");
        std::fs::write(&audio, b"FAKEAUDIO").unwrap();

        let mut lib = Library::load(root.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio, "My Song", Some("Me"), "deadbeef", 30.0)
            .unwrap();

        // song is indexed and dedup-findable
        assert_eq!(lib.song_by_hash("deadbeef").unwrap().id, song.id);
        // audio copied into the bundle as audio.flac
        let bundle_dir = root.path().join("My Song — Me");
        assert!(bundle_dir.join("audio.flac").exists());
        assert_eq!(std::fs::read(&song.path).unwrap(), b"FAKEAUDIO");
        // manifest written
        assert!(bundle_dir.join("dredge.json").exists());
    }
}
```

- [ ] **Step 2: Implement**

```rust
impl Library {
    /// Create a new bundle: allocate a dir, copy the source audio in as
    /// `audio.<ext>`, write the initial manifest, and index it. The source
    /// file is never touched again.
    pub fn create_song(
        &mut self,
        src_audio: &Path,
        title: &str,
        artist: Option<&str>,
        file_hash: &str,
        duration_secs: f64,
    ) -> Result<Song> {
        let slug = bundle::slug(title, artist);
        let dir = bundle::unique_bundle_dir(&self.root, &slug);
        std::fs::create_dir_all(&dir)?;
        let ext = src_audio
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("audio");
        let dest = dir.join(format!("audio.{ext}"));
        std::fs::copy(src_audio, &dest)?;

        let song = Song {
            id: SongId(self.next_id()),
            title: title.to_string(),
            artist: artist.map(str::to_string),
            path: dest.to_string_lossy().into_owned(),
            file_hash: file_hash.to_string(),
            duration_secs,
        };
        let manifest = BundleManifest {
            version: bundle::MANIFEST_VERSION,
            song: song.clone(),
            sections: vec![],
            loops: vec![],
            notes: vec![],
            analysis: None,
        };
        bundle::write_manifest(&dir, &manifest)?;
        self.entries.insert(song.id.0, Entry { dir, manifest });
        Ok(song)
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p practice library::create_tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/library.rs
git commit -m "feat(practice): create bundle copies audio + writes manifest"
```

### Task 2.3: Accessors + mutators mirroring the old Store API

Each mutator updates the in-memory manifest and rewrites `dredge.json`. These
mirror the `Store` methods `app.rs` calls today (see the call-site list in the
plan header) so Phase 3 is a near-mechanical swap.

**Files:**
- Modify: `crates/practice/src/library.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod mutate_tests {
    use super::*;

    fn seeded() -> (tempfile::TempDir, Library, SongId) {
        let root = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let audio = src.path().join("o.wav");
        std::fs::write(&audio, b"A").unwrap();
        let mut lib = Library::load(root.path().to_path_buf()).unwrap();
        let s = lib.create_song(&audio, "S", None, "h", 5.0).unwrap();
        (root, lib, s.id)
    }

    #[test]
    fn sections_loops_notes_analysis_persist_and_reload() {
        let (root, mut lib, song_id) = seeded();

        let secs = lib
            .replace_sections(song_id, &[NewSection { name: "verse".into(), start: 0.0, end: 2.0, position: 0 }])
            .unwrap();
        assert_eq!(secs.len(), 1);

        let lp = lib
            .insert_loop(song_id, NewLoop { name: "riff".into(), name_override: None, start: 0.0, end: 1.0, kind: LoopKind::Manual })
            .unwrap();
        assert_eq!(lib.list_loops(song_id).len(), 1);

        lib.set_section_notes(song_id, "verse", &crate::notes::NotesDoc::from_text("hi")).unwrap();

        let analysis = Analysis { bpm: Some(120.0), beats: vec![], downbeats: vec![], sections: vec![], engine: "test".into() };
        lib.save_analysis(song_id, &analysis).unwrap();
        assert!(lib.has_analysis(song_id));

        // reload from disk: everything survived
        let lib2 = Library::load(root.path().to_path_buf()).unwrap();
        assert_eq!(lib2.list_sections(song_id).len(), 1);
        assert_eq!(lib2.list_loops(song_id).len(), 1);
        assert_eq!(lib2.list_section_notes(song_id).len(), 1);
        assert_eq!(lib2.get_analysis(song_id).unwrap().bpm, Some(120.0));
        let _ = (root, lp);
    }
}
```

> Note: this test uses `crate::notes::NotesDoc::from_text`. If no such
> constructor exists, build a `NotesDoc` the way `notes.rs` tests do — check
> `crates/practice/src/notes.rs` for the public constructor and use it verbatim.

- [ ] **Step 2: Implement the mutators/accessors**

Add to `impl Library`. They reuse the existing `NewSection`/`NewLoop`/`LoopRename`
structs — move those from `store.rs` into `library.rs` (or re-export), and use
the shared `next_id` for section/loop ids.

```rust
use crate::store::{NewLoop, NewSection, LoopRename}; // or move the structs here

impl Library {
    fn entry_mut(&mut self, id: SongId) -> Result<&mut Entry> {
        self.entries.get_mut(&id.0).ok_or(crate::error::Error::NotFound)
    }

    fn persist(entry: &Entry) -> Result<()> {
        bundle::write_manifest(&entry.dir, &entry.manifest)
    }

    pub fn list_sections(&self, song_id: SongId) -> Vec<Section> {
        self.entries.get(&song_id.0).map(|e| e.manifest.sections.clone()).unwrap_or_default()
    }

    pub fn replace_sections(&mut self, song_id: SongId, sections: &[NewSection]) -> Result<Vec<Section>> {
        let mut out = Vec::with_capacity(sections.len());
        let mut ids = Vec::with_capacity(sections.len());
        for _ in sections { ids.push(self.next_id()); }
        for (s, id) in sections.iter().zip(ids) {
            out.push(Section { id: SectionId(id), song_id, name: s.name.to_owned(), start: s.start, end: s.end, position: s.position });
        }
        out.sort_by_key(|s| s.position);
        let entry = self.entry_mut(song_id)?;
        entry.manifest.sections = out.clone();
        Self::persist(entry)?;
        Ok(out)
    }

    pub fn list_loops(&self, song_id: SongId) -> Vec<LoopRegion> {
        self.entries.get(&song_id.0).map(|e| e.manifest.loops.clone()).unwrap_or_default()
    }

    pub fn loop_by_id(&self, id: LoopId) -> Option<LoopRegion> {
        self.entries.values().flat_map(|e| &e.manifest.loops).find(|l| l.id == id).cloned()
    }

    pub fn insert_loop(&mut self, song_id: SongId, l: NewLoop) -> Result<LoopRegion> {
        let region = LoopRegion {
            id: LoopId(self.next_id()),
            song_id,
            name: l.name.to_owned(),
            name_override: l.name_override.map(str::to_owned),
            start: l.start,
            end: l.end,
            kind: l.kind,
        };
        let entry = self.entry_mut(song_id)?;
        entry.manifest.loops.push(region.clone());
        Self::persist(entry)?;
        Ok(region)
    }

    pub fn update_loop(&mut self, id: LoopId, name: Option<&str>, name_override: Option<Option<&str>>, start: f64, end: f64) -> Result<LoopRegion> {
        let song_id = self.loop_by_id(id).ok_or(crate::error::Error::NotFound)?.song_id;
        let entry = self.entry_mut(song_id)?;
        let lp = entry.manifest.loops.iter_mut().find(|l| l.id == id).ok_or(crate::error::Error::NotFound)?;
        if let Some(n) = name { lp.name = n.to_owned(); }
        if let Some(ov) = name_override { lp.name_override = ov.map(str::to_owned); }
        lp.start = start; lp.end = end;
        let updated = lp.clone();
        Self::persist(entry)?;
        Ok(updated)
    }

    pub fn delete_loop(&mut self, id: LoopId) -> Result<()> {
        if let Some(song_id) = self.loop_by_id(id).map(|l| l.song_id) {
            let entry = self.entry_mut(song_id)?;
            entry.manifest.loops.retain(|l| l.id != id);
            Self::persist(entry)?;
        }
        Ok(())
    }

    pub fn delete_loops(&mut self, ids: &[LoopId]) -> Result<()> {
        for id in ids { self.delete_loop(*id)?; }
        Ok(())
    }

    pub fn rename_loops(&mut self, renames: &[LoopRename]) -> Result<()> {
        for r in renames {
            self.update_loop(r.id, Some(&r.name), Some(None), r.start, r.end)?;
        }
        Ok(())
    }

    pub fn get_section_notes(&self, song_id: SongId, label: &str) -> Option<crate::notes::NotesDoc> {
        self.entries.get(&song_id.0)
            .and_then(|e| e.manifest.notes.iter().find(|n| n.label == label))
            .map(|n| n.doc.clone())
    }

    pub fn list_section_notes(&self, song_id: SongId) -> Vec<(String, crate::notes::NotesDoc)> {
        self.entries.get(&song_id.0)
            .map(|e| e.manifest.notes.iter().map(|n| (n.label.clone(), n.doc.clone())).collect())
            .unwrap_or_default()
    }

    pub fn set_section_notes(&mut self, song_id: SongId, label: &str, doc: &crate::notes::NotesDoc) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.notes.retain(|n| n.label != label);
        if !doc.is_empty() {
            entry.manifest.notes.push(SectionNote { label: label.to_owned(), doc: doc.clone() });
            entry.manifest.notes.sort_by(|a, b| a.label.cmp(&b.label));
        }
        Self::persist(entry)?;
        Ok(())
    }

    pub fn has_analysis(&self, song_id: SongId) -> bool {
        self.entries.get(&song_id.0).map(|e| e.manifest.analysis.is_some()).unwrap_or(false)
    }

    pub fn get_analysis(&self, song_id: SongId) -> Option<Analysis> {
        self.entries.get(&song_id.0).and_then(|e| e.manifest.analysis.clone())
    }

    pub fn save_analysis(&mut self, song_id: SongId, a: &Analysis) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.analysis = Some(a.clone());
        Self::persist(entry)
    }

    pub fn update_song(&mut self, id: SongId, title: &str, artist: Option<&str>) -> Result<Song> {
        let entry = self.entry_mut(id)?;
        entry.manifest.song.title = title.to_owned();
        entry.manifest.song.artist = artist.map(str::to_owned);
        let song = entry.manifest.song.clone();
        Self::persist(entry)?;
        Ok(song)
    }

    /// Bundle directory for a song (used to locate audio + stems).
    pub fn bundle_dir(&self, id: SongId) -> Option<PathBuf> {
        self.entries.get(&id.0).map(|e| e.dir.clone())
    }

    /// Delete the whole bundle directory and drop it from the index.
    pub fn delete_song(&mut self, id: SongId) -> Result<()> {
        if let Some(entry) = self.entries.remove(&id.0) {
            if let Err(e) = std::fs::remove_dir_all(&entry.dir) {
                if e.kind() != std::io::ErrorKind::NotFound { return Err(e.into()); }
            }
        }
        Ok(())
    }
}
```

> `update_loop`'s `name_override: Option<Option<&str>>` mirrors today's
> semantics: outer `None` = leave override unchanged, `Some(None)` = clear it,
> `Some(Some(s))` = pin it. Confirm against the current `loop.update` handler in
> `app.rs` and match whatever it does; adjust the signature if the handler is
> simpler.

- [ ] **Step 3: Run**

Run: `cargo test -p practice library::mutate_tests`
Expected: PASS.

- [ ] **Step 4: Run the whole practice crate**

Run: `cargo test -p practice`
Expected: PASS (old store tests for removed methods are deleted in Phase 4; if
they still reference removed APIs at this point, that's fine — Phase 4 cleans
them, but `library` and `bundle` tests must pass now).

- [ ] **Step 5: Commit**

```bash
git add crates/practice/src/library.rs
git commit -m "feat(practice): Library mutators write through to manifests"
```

---

## Phase 3 — Wire `app.rs` to the `Library`

The App gains a `library: Library` field and loses `stems_dir`. Every
`self.store.<song-data>` call becomes `self.library.<...>`. Stems and analysis
write into the bundle dir.

### Task 3.1: Add `Library` to `App`, build it at construction

**Files:**
- Modify: `crates/server/src/app.rs`

- [ ] **Step 1: Add the field and remove `stems_dir`**

In the `App` struct (around line 262-267): add `library: practice::library::Library,`
and delete `stems_dir: PathBuf,`.

In the constructor (around line 320), after the `Store` is opened, build the
library from the resolved root and load it:

```rust
let root = Self::library_root(&store);
let library = practice::library::Library::load(root).map_err(|e| e.to_string())?;
```

Set `library` in the struct literal; delete `stems_dir: default_stems_dir(),`.
Delete the now-unused `default_stems_dir`, `stems_cache_dir`, `set_stems_dir`,
and the `set_stems_dir`-based test seams (replace test seams with a
`set_library`/`with_library_root` helper as needed — see Step 3).

- [ ] **Step 2: Replace stems-cache path usage**

Wherever the code computed `self.stems_cache_dir(&song.file_hash)`, use the
bundle's stems dir instead:

```rust
let stems_cache = self.library.bundle_dir(song.id)
    .ok_or("song not in library")?
    .join("stems");
```

`open_decode` / `export_decode` already take a `stems_cache: &Path` — pass this
path. `App::stems_cached` is unchanged (it just checks the four WAVs exist).

- [ ] **Step 3: Provide a test seam**

Tests today call `set_stems_dir`. Replace with a constructor/override that builds
the App against a tempdir library root. Add:

```rust
#[cfg(test)]
pub fn set_library_root(&mut self, root: std::path::PathBuf) {
    self.library = practice::library::Library::load(root).expect("load test library");
}
```

- [ ] **Step 4: Build**

Run: `cargo build -p server`
Expected: compile errors only at the not-yet-migrated `self.store.<...>` song
calls (fixed in 3.2). Path/stems wiring should compile.

(No commit yet — finish 3.2 first.)

### Task 3.2: Swap every song-data store call to the library

**Files:**
- Modify: `crates/server/src/app.rs`

For each call site (line numbers from the plan header; re-grep to be exact),
replace `self.store.X(...).err_str()?` with the `Library` equivalent. The
`Library` accessors return values directly (not `Result`) for reads, so drop
`.err_str()?` on reads and wrap them in `Ok(...)` where the handler expected a
`Result`:

- [ ] **Step 1: Replace the calls**

| Old (`self.store`) | New (`self.library`) |
|---|---|
| `list_songs()` (returns `Result<Vec<Song>>`) | `list_songs()` → `Vec<Song>` (no `?`) |
| `song_by_hash(h)` | `song_by_hash(h)` → `Option<Song>` |
| `song_by_id(id)` | `song_by_id(id)` → `Option<Song>` |
| `delete_song(id)` | `delete_song(id)` → `Result<()>` |
| `update_song(id, t, a)` | `update_song(id, t, a)` → `Result<Song>` |
| `replace_sections(id, &s)` | `replace_sections(id, &s)` → `Result<Vec<Section>>` |
| `list_sections(id)` | `list_sections(id)` → `Vec<Section>` |
| `insert_loop(id, l)` | `insert_loop(id, l)` → `Result<LoopRegion>` |
| `update_loop(...)` | `update_loop(...)` → `Result<LoopRegion>` |
| `delete_loop(id)` / `delete_loops(&ids)` | same names → `Result<()>` |
| `list_loops(id)` | `list_loops(id)` → `Vec<LoopRegion>` |
| `loop_by_id(id)` | `loop_by_id(id)` → `Option<LoopRegion>` |
| `rename_loops(&r)` | `rename_loops(&r)` → `Result<()>` |
| `set_section_notes(id, l, d)` | same → `Result<()>` |
| `get_section_notes(id, l)` | same → `Option<NotesDoc>` |
| `list_section_notes(id)` | same → `Vec<(String, NotesDoc)>` |
| `has_analysis(id)` | `has_analysis(id)` → `bool` |
| `get_analysis(id)` | `get_analysis(id)` → `Option<Analysis>` |
| `save_analysis(id, &a)` | `save_analysis(id, &a)` → `Result<()>` |

Settings/profiles calls (`all_settings`, `set_setting`, `get_setting`,
`list_profiles`, `save_profile`) **stay on `self.store`** — those tables remain.

For reads that previously did `serde_json::to_value(self.store.list_songs().err_str()?)`,
write `serde_json::to_value(self.library.list_songs())`.

- [ ] **Step 2: Fix `song_import`**

In `song_import` (around line 1147), the dedupe + insert path becomes:

```rust
if let Some(existing) = self.library.song_by_hash(&prep.hash) {
    return /* existing-song response, unchanged */;
}
let song = self
    .library
    .create_song(Path::new(&prep.path), &prep.title, None, &prep.hash, prep.duration_secs)
    .err_str()?;
```

`prep.path` is the *source* file the user picked; `create_song` copies it in.
Drop the old sidecar read (`prep.sidecar`) — bundles replace it (Phase 4 removes
the `sidecar` field from `ImportPrepared`).

- [ ] **Step 3: Build**

Run: `cargo build -p server`
Expected: compiles.

- [ ] **Step 4: Run server tests**

Run: `cargo test -p server`
Expected: failures only where tests used `set_stems_dir`/sidecar; update them to
`set_library_root` + a tempdir. Re-run until green.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/app.rs
git commit -m "feat(server): app reads/writes song data through the Library"
```

### Task 3.3: Stems and analysis write into the bundle

**Files:**
- Modify: `crates/server/src/app.rs`, `crates/server/src/stems.rs`

- [ ] **Step 1: Point stem separation output at the bundle**

In `stems_separate` (and the background job that runs the separator), compute the
output dir as `self.library.bundle_dir(song_id)?.join("stems")` and pass that to
the separator. Remove any `self.stems_dir`-relative path. Confirm
`stems::separate`/`rewrite_wav_48k` take a target dir/path argument and feed them
the bundle stems dir.

- [ ] **Step 2: Confirm analysis already routes through `save_analysis`**

The analysis completion handler (around line 590) already calls
`save_analysis(song_id, &a)` — now backed by the Library, so it writes the
manifest. No further change beyond Task 3.2.

- [ ] **Step 3: Build + test**

Run: `cargo build -p server && cargo test -p server`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/app.rs crates/server/src/stems.rs
git commit -m "feat(server): stems write into the song bundle"
```

---

## Phase 4 — Reduce the Store and delete the sidecar

### Task 4.1: Delete `sidecar.rs` and its references

**Files:**
- Delete: `crates/practice/src/sidecar.rs`
- Modify: `crates/practice/src/lib.rs`, `crates/server/src/app.rs`

- [ ] **Step 1: Remove the module and call sites**

```bash
rm crates/practice/src/sidecar.rs
```

Remove `pub mod sidecar;` from `lib.rs`. Remove `sidecar` from `ImportPrepared`
and the `read_sidecar` call in `import_decode`. Remove any `write_sidecar` /
`remove_sidecar` calls (e.g. in `song_update`, `section_replace`, `song_delete`
if present — grep `sidecar`).

- [ ] **Step 2: Confirm no references remain**

Run: `grep -rn "sidecar" crates/ apps/`
Expected: no matches.

- [ ] **Step 3: Build**

Run: `cargo build -p practice -p server`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: delete sidecar, bundles replace it"
```

### Task 4.2: Strip song-data tables and accessors from `store.rs`

**Files:**
- Modify: `crates/practice/src/store.rs`

- [ ] **Step 1: Remove accessors and tables**

Delete from `store.rs`: `insert_song`, `song_from_row`, `song_by_hash`,
`list_songs`, `song_by_id`, `delete_song`, `update_song`, `replace_sections`,
`list_sections`, `set_section_notes`, `get_section_notes`, `list_section_notes`,
`insert_loop`, `loop_*`, `delete_loop(s)`, `rename_loops`, `save_analysis`,
`get_analysis`, `has_analysis`, and the `NewSong`/`NewSection`/`NewLoop`/
`LoopRename` structs if you moved them into `library.rs`.

Keep: `Store::open`, `open_in_memory`, `init`, `migrate`, settings accessors
(`all_settings`, `set_setting`, `get_setting`), and profile accessors
(`save_profile`, `list_profiles`).

Replace the schema: keep `SCHEMA_V3` (settings) and `SCHEMA_V4/V5` (profiles).
Since existing DBs are wiped (Task 0.1), simplify `migrate` to create just the
settings + profiles tables at `user_version` 1. Delete `SCHEMA_V1`, `V2`,
`V6`–`V9` and the song-data parts. Update the doc comments.

```rust
const SCHEMA_V1: &str = "
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);
CREATE TABLE profiles (
    id INTEGER PRIMARY KEY,
    op TEXT NOT NULL,
    song_id INTEGER,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    total_ms INTEGER NOT NULL,
    ok INTEGER NOT NULL,
    error TEXT,
    device TEXT,
    engine TEXT,
    max_cpu_pct INTEGER,
    max_gpu_util INTEGER,
    max_vram_used_mb INTEGER,
    vram_total_mb INTEGER,
    stages_json TEXT NOT NULL
);
";
```

`migrate` becomes a single `if version < 1 { execute_batch(SCHEMA_V1); set user_version 1 }`.

> `profiles.song_id` is now just an integer with no FK to a songs table (the
> table is gone). That is fine — it was already nullable and FK-less in spirit.

- [ ] **Step 2: Delete obsolete store tests**

Remove the `store::tests` cases that exercise removed methods (songs/sections/
loops/notes/analysis). Keep settings/profiles tests.

- [ ] **Step 3: Build + test**

Run: `cargo test -p practice`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/practice/src/store.rs
git commit -m "refactor(practice): Store keeps only settings + profiles"
```

---

## Phase 5 — Frontend: library-root setting

### Task 5.1: Surface the library path in the settings tab

**Files:**
- Modify: `apps/desktop/src/components/Settings.svelte`
- Possibly: `apps/desktop/src/lib/stores.ts`

- [ ] **Step 1: Inspect how other settings are read/written**

Run: `grep -n "settings.set\|settings.get_all\|set_setting" apps/desktop/src/lib/stores.ts apps/desktop/src/components/Settings.svelte`
Follow the existing pattern for a string setting (the settings flow already
exists for other keys).

- [ ] **Step 2: Add a text field bound to `library_root`**

In `Settings.svelte`, add a labeled text input that reads the current
`library_root` setting and writes it via the same `cmd('settings.set', {...})`
path other settings use. Copy the existing setting-row markup so styling matches
(no new components). Per project convention, no explanatory hint text — just the
label and field.

- [ ] **Step 3: Note the restart requirement**

Changing `library_root` re-points the library; the app loads the library at
construction. Either reload the library on the `settings.set` of that key
(call a new `app.reload_library` path) or document that it takes effect on
restart. Simplest: take effect on restart (matches "personal tool", no
hand-holding copy). Confirm with a one-line label suffix only if natural.

- [ ] **Step 4: Frontend check**

Run: `cd apps/desktop && pnpm vitest run` then `pnpm svelte-check`
Expected: PASS / no new errors.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src
git commit -m "feat(ui): library path setting"
```

---

## Phase 6 — Full verification

### Task 6.1: Whole-suite gate

- [ ] **Step 1: Run the project gate**

Run: `just check`
Expected: `cargo test --workspace` + `pnpm vitest run` pass; clippy `-D warnings`
clean; `cargo fmt --check` clean; `svelte-check` clean.

- [ ] **Step 2: Manual end-to-end on the dev build**

Run: `just build` then `just run`. Verify by hand:
1. Import an audio file → a bundle dir appears under `<music dir>/dredge/<slug>/`
   with `audio.<ext>` and `dredge.json`.
2. Run analysis → `dredge.json` gains an `analysis` block.
3. Separate stems → `stems/{vocals,drums,bass,other}.wav` appear in the bundle.
4. Add a section + note + loop → all present in `dredge.json`.
5. Quit. Copy the bundle dir to a second machine's `<music dir>/dredge/`. Launch
   dredge there → the song loads with stems, analysis, sections, loops, notes,
   no recomputation.

> The webview can't be driven by chrome-devtools (WebKitGTK) — verify steps 1–4
> via the file system + `just cmd '{"id":1,"cmd":"song.list"}'`, and step 5 by
> the second-machine launch (or a second library root via `DREDGE_DB`/setting).

- [ ] **Step 3: Final commit (if any fixups)**

```bash
git add -A
git commit -m "chore: portable bundles verification fixups"
```

---

## Self-review notes

- **Spec coverage:** layout (Task 0.2, 1.x), manifest incl. notes+analysis
  (1.1, 2.3), DB→settings-only (4.2), bundles canonical + in-memory index (2.1),
  ids in manifest (2.1, 2.3), import copy-in + dedup (2.2, 3.2), edits write
  manifest (2.3), stems/analysis into bundle (3.3), sidecar removal (4.1), wipe
  (0.1), settings library_root (0.3, 5.1). All spec sections map to a task.
- **Open items resolved:** id scheme = monotonic-from-scan (2.1); profiles kept
  settings-side (4.2); notes = `Vec<SectionNote>` (1.1); frontend list now from
  scan, wire shape unchanged (3.2).
- **Type consistency:** `Library` method names match the swap table in 3.2;
  `NewSection`/`NewLoop`/`LoopRename` reused from `store.rs` (moved in 2.3);
  `bundle::{slug, unique_bundle_dir, write_manifest, read_manifest, scan_library,
  default_library_root, MANIFEST_VERSION, MANIFEST_FILE}` used consistently.
- **Verify before claiming done:** the `NotesDoc` constructor in 2.3 and the
  `loop.update` override semantics in 2.3 must be checked against current code,
  not assumed.
