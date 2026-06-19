use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::bundle::{self, BundleManifest};
use crate::error::Result;
use crate::model::*;

pub struct NewSection<'a> {
    pub name: &'a str,
    pub start: f64,
    pub end: f64,
    pub position: i32,
}

pub struct NewLoop<'a> {
    pub name: &'a str,
    pub name_override: Option<&'a str>,
    pub start: f64,
    pub end: f64,
    pub kind: LoopKind,
}

/// A recomputed dynamic-loop name (override is reset to NULL). Used by the
/// batched `rename_loops`.
pub struct LoopRename {
    pub id: LoopId,
    pub name: String,
    pub start: f64,
    pub end: f64,
}

struct Entry {
    dir: PathBuf,
    manifest: BundleManifest,
}

/// In-memory index over the bundle library. Source of truth is the manifests
/// on disk; this caches them and writes through on every mutation.
pub struct Library {
    root: PathBuf,
    entries: HashMap<i64, Entry>, // keyed by SongId.0
}

impl Library {
    /// Load every bundle under `root`. Ids are read from the manifests (assigned
    /// at creation, so they travel with the bundle). On the vanishingly rare id
    /// clash between independently authored bundles, the newcomer is reassigned
    /// a fresh id and its manifest rewritten, so neither song is silently lost.
    pub fn load(root: PathBuf) -> Result<Self> {
        let mut entries = HashMap::new();
        let mut used: HashSet<i64> = HashSet::new();
        for (dir, mut m) in bundle::scan_library(&root)? {
            // The manifest stores an absolute audio path from whatever machine
            // wrote it. A bundle copied to another PC (or a different library
            // root / home dir) keeps that stale prefix, so rebase the audio
            // path onto THIS bundle dir — the file name is all that's portable.
            if let Some(fname) = Path::new(&m.song.path).file_name() {
                m.song.path = dir.join(fname).to_string_lossy().into_owned();
            }
            if entries.contains_key(&m.song.id.0) {
                m.song.id = SongId(Self::fresh_id(&mut used));
                bundle::write_manifest(&dir, &m)?;
            }
            used.insert(m.song.id.0);
            for sec in &m.sections {
                used.insert(sec.id.0);
            }
            for lp in &m.loops {
                used.insert(lp.id.0);
            }
            entries.insert(m.song.id.0, Entry { dir, manifest: m });
        }
        Ok(Self { root, entries })
    }

    /// An empty library rooted at `root` (used as a fallback when a scan fails).
    pub fn empty(root: PathBuf) -> Self {
        Self {
            root,
            entries: HashMap::new(),
        }
    }

    /// All bundle directories currently indexed.
    pub fn bundle_dirs(&self) -> Vec<PathBuf> {
        self.entries.values().map(|e| e.dir.clone()).collect()
    }

    /// Every id currently in use across songs, sections, and loops — the set a
    /// freshly minted id must avoid.
    fn used_ids(&self) -> HashSet<i64> {
        let mut s = HashSet::new();
        for e in self.entries.values() {
            s.insert(e.manifest.song.id.0);
            for sec in &e.manifest.sections {
                s.insert(sec.id.0);
            }
            for lp in &e.manifest.loops {
                s.insert(lp.id.0);
            }
        }
        s
    }

    /// A random id in `[1, 2^53)` not already in `used` (and recorded into it).
    /// Bounded below 2^53 so it round-trips losslessly through JSON/JS float64;
    /// 53 bits of entropy makes collisions negligible for a personal library,
    /// and `used` guarantees uniqueness regardless.
    fn fresh_id(used: &mut HashSet<i64>) -> i64 {
        loop {
            let mut b = [0u8; 8];
            getrandom::getrandom(&mut b).expect("system RNG unavailable");
            let id = (1 + (u64::from_le_bytes(b) % ((1u64 << 53) - 1))) as i64;
            if used.insert(id) {
                return id;
            }
        }
    }

    pub fn list_songs(&self) -> Vec<Song> {
        let mut v: Vec<Song> = self
            .entries
            .values()
            .map(|e| e.manifest.song.clone())
            .collect();
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

    // ── create_song ────────────────────────────────────────────────────────────

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
            id: SongId(Self::fresh_id(&mut self.used_ids())),
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

    // ── internal helpers ───────────────────────────────────────────────────────

    fn entry_mut(&mut self, id: SongId) -> Result<&mut Entry> {
        self.entries
            .get_mut(&id.0)
            .ok_or(crate::error::Error::NotFound)
    }

    fn persist(entry: &Entry) -> Result<()> {
        bundle::write_manifest(&entry.dir, &entry.manifest)
    }

    // ── sections ───────────────────────────────────────────────────────────────

    pub fn list_sections(&self, song_id: SongId) -> Vec<Section> {
        self.entries
            .get(&song_id.0)
            .map(|e| e.manifest.sections.clone())
            .unwrap_or_default()
    }

    pub fn replace_sections(
        &mut self,
        song_id: SongId,
        sections: &[NewSection],
    ) -> Result<Vec<Section>> {
        let mut used = self.used_ids();
        let mut ids = Vec::with_capacity(sections.len());
        for _ in sections {
            ids.push(Self::fresh_id(&mut used));
        }
        let mut out: Vec<Section> = sections
            .iter()
            .zip(ids)
            .map(|(s, id)| Section {
                id: SectionId(id),
                song_id,
                name: s.name.to_owned(),
                start: s.start,
                end: s.end,
                position: s.position,
            })
            .collect();
        out.sort_by_key(|s| s.position);
        let entry = self.entry_mut(song_id)?;
        entry.manifest.sections = out.clone();
        Self::persist(entry)?;
        Ok(out)
    }

    // ── loops ──────────────────────────────────────────────────────────────────

    pub fn list_loops(&self, song_id: SongId) -> Vec<LoopRegion> {
        self.entries
            .get(&song_id.0)
            .map(|e| e.manifest.loops.clone())
            .unwrap_or_default()
    }

    pub fn loop_by_id(&self, id: LoopId) -> Option<LoopRegion> {
        self.entries
            .values()
            .flat_map(|e| &e.manifest.loops)
            .find(|l| l.id == id)
            .cloned()
    }

    pub fn insert_loop(&mut self, song_id: SongId, l: NewLoop) -> Result<LoopRegion> {
        let region = LoopRegion {
            id: LoopId(Self::fresh_id(&mut self.used_ids())),
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

    /// Matches store::update_loop exactly: caller computes name + override.
    pub fn update_loop(
        &mut self,
        id: LoopId,
        name: &str,
        name_override: Option<&str>,
        start: f64,
        end: f64,
    ) -> Result<LoopRegion> {
        let song_id = self
            .loop_by_id(id)
            .ok_or(crate::error::Error::NotFound)?
            .song_id;
        let entry = self.entry_mut(song_id)?;
        let lp = entry
            .manifest
            .loops
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or(crate::error::Error::NotFound)?;
        lp.name = name.to_owned();
        lp.name_override = name_override.map(str::to_owned);
        lp.start = start;
        lp.end = end;
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
        for id in ids {
            self.delete_loop(*id)?;
        }
        Ok(())
    }

    pub fn rename_loops(&mut self, renames: &[LoopRename]) -> Result<()> {
        for r in renames {
            self.update_loop(r.id, &r.name, None, r.start, r.end)?;
        }
        Ok(())
    }

    // ── notes ──────────────────────────────────────────────────────────────────

    pub fn get_section_notes(
        &self,
        song_id: SongId,
        label: &str,
    ) -> Option<crate::notes::NotesDoc> {
        self.entries
            .get(&song_id.0)
            .and_then(|e| e.manifest.notes.iter().find(|n| n.label == label))
            .map(|n| n.doc.clone())
    }

    pub fn list_section_notes(&self, song_id: SongId) -> Vec<(String, crate::notes::NotesDoc)> {
        self.entries
            .get(&song_id.0)
            .map(|e| {
                e.manifest
                    .notes
                    .iter()
                    .map(|n| (n.label.clone(), n.doc.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_section_notes(
        &mut self,
        song_id: SongId,
        label: &str,
        doc: &crate::notes::NotesDoc,
    ) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.notes.retain(|n| n.label != label);
        if !doc.is_empty() {
            entry.manifest.notes.push(SectionNote {
                label: label.to_owned(),
                doc: doc.clone(),
            });
            entry.manifest.notes.sort_by(|a, b| a.label.cmp(&b.label));
        }
        Self::persist(entry)?;
        Ok(())
    }

    // ── analysis ───────────────────────────────────────────────────────────────

    pub fn has_analysis(&self, song_id: SongId) -> bool {
        self.entries
            .get(&song_id.0)
            .map(|e| e.manifest.analysis.is_some())
            .unwrap_or(false)
    }

    pub fn get_analysis(&self, song_id: SongId) -> Option<Analysis> {
        self.entries
            .get(&song_id.0)
            .and_then(|e| e.manifest.analysis.clone())
    }

    pub fn save_analysis(&mut self, song_id: SongId, a: &Analysis) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.analysis = Some(a.clone());
        Self::persist(entry)
    }

    // ── song mutations ─────────────────────────────────────────────────────────

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
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(e.into());
                }
            }
        }
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Task 2.1: load + listing + id generation ──

    #[test]
    fn empty_root_loads_clean() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::load(dir.path().to_path_buf()).unwrap();
        assert!(lib.list_songs().is_empty());
    }

    #[test]
    fn fresh_ids_are_unique_and_js_safe() {
        // 2^53 is the largest integer JS float64 represents exactly; ids must
        // stay below it to survive the JSON round-trip to the frontend.
        const JS_MAX: i64 = 1 << 53;
        let mut used = HashSet::new();
        for _ in 0..10_000 {
            let id = Library::fresh_id(&mut used);
            assert!((1..JS_MAX).contains(&id), "id {id} out of JS-safe range");
        }
        assert_eq!(used.len(), 10_000, "all 10k ids were distinct");
    }

    #[test]
    fn load_reassigns_colliding_song_ids() {
        // Two independently authored bundles that happen to share song id 1:
        // both must survive the load, with one reassigned.
        let lib_dir = tempfile::tempdir().unwrap();
        for name in ["First", "Second"] {
            let bundle_dir = lib_dir.path().join(name);
            std::fs::create_dir_all(&bundle_dir).unwrap();
            std::fs::write(bundle_dir.join("audio.flac"), b"A").unwrap();
            let manifest = BundleManifest {
                version: bundle::MANIFEST_VERSION,
                song: Song {
                    id: SongId(1),
                    title: name.into(),
                    artist: None,
                    path: bundle_dir.join("audio.flac").to_string_lossy().into_owned(),
                    file_hash: name.into(),
                    duration_secs: 1.0,
                },
                sections: vec![],
                loops: vec![],
                notes: vec![],
                analysis: None,
            };
            bundle::write_manifest(&bundle_dir, &manifest).unwrap();
        }

        let lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let songs = lib.list_songs();
        assert_eq!(songs.len(), 2, "both bundles survived the id clash");
        let ids: HashSet<i64> = songs.iter().map(|s| s.id.0).collect();
        assert_eq!(ids.len(), 2, "the colliding id was reassigned");
    }

    // ── Task 2.2: create_song ──

    #[test]
    fn create_copies_audio_and_indexes_song() {
        let src_dir = tempfile::tempdir().unwrap();
        let lib_dir = tempfile::tempdir().unwrap();

        let audio_src = src_dir.path().join("orig.flac");
        std::fs::write(&audio_src, b"FAKEAUDIO").unwrap();

        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "My Song", Some("Me"), "deadbeef", 30.0)
            .unwrap();

        // findable by hash
        let found = lib.song_by_hash("deadbeef").unwrap();
        assert_eq!(found.id, song.id);

        // bundle dir is <root>/My Song — Me/ (em-dash U+2014)
        let expected_slug = "My Song \u{2014} Me";
        let bundle_dir = lib_dir.path().join(expected_slug);
        assert!(
            bundle_dir.is_dir(),
            "bundle dir should exist: {bundle_dir:?}"
        );

        // audio.flac exists with correct content
        let audio_dest = bundle_dir.join("audio.flac");
        assert!(audio_dest.exists(), "audio.flac should exist");
        let bytes = std::fs::read(&audio_dest).unwrap();
        assert_eq!(bytes, b"FAKEAUDIO");

        // dredge.json exists
        let manifest_path = bundle_dir.join("dredge.json");
        assert!(manifest_path.exists(), "dredge.json should exist");
    }

    // ── portability: a copied bundle's audio path is rebased on load ──

    #[test]
    fn load_rebases_stale_audio_path_onto_actual_dir() {
        // Simulate a bundle authored on another machine: its manifest records
        // an absolute audio path under a foreign library root that does not
        // exist here. On load, song.path must point at the real audio file in
        // this bundle dir, not the stale prefix.
        let lib_dir = tempfile::tempdir().unwrap();
        let bundle_dir = lib_dir.path().join("Foreign Song");
        std::fs::create_dir_all(&bundle_dir).unwrap();
        std::fs::write(bundle_dir.join("audio.flac"), b"AUDIO").unwrap();

        let manifest = BundleManifest {
            version: bundle::MANIFEST_VERSION,
            song: Song {
                id: SongId(1),
                title: "Foreign Song".into(),
                artist: None,
                path: "/home/someone-else/Music/dredge/Foreign Song/audio.flac".into(),
                file_hash: "abc".into(),
                duration_secs: 10.0,
            },
            sections: vec![],
            loops: vec![],
            notes: vec![],
            analysis: None,
        };
        bundle::write_manifest(&bundle_dir, &manifest).unwrap();

        let lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib.song_by_id(SongId(1)).unwrap();
        assert_eq!(song.path, bundle_dir.join("audio.flac").to_string_lossy());
        assert!(
            Path::new(&song.path).exists(),
            "rebased audio path must exist"
        );
    }

    // ── Task 2.3: mutators + accessors + write-through ──

    #[test]
    fn sections_loops_notes_analysis_persist_and_reload() {
        let lib_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let audio_src = src_dir.path().join("track.flac");
        std::fs::write(&audio_src, b"AUDIO").unwrap();

        // seed library and create song
        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "Test Song", None, "hash123", 60.0)
            .unwrap();

        // replace_sections with one section
        let sections = lib
            .replace_sections(
                song.id,
                &[NewSection {
                    name: "intro",
                    start: 0.0,
                    end: 10.0,
                    position: 0,
                }],
            )
            .unwrap();
        assert_eq!(sections.len(), 1);

        // insert_loop
        lib.insert_loop(
            song.id,
            NewLoop {
                name: "loop 1",
                name_override: None,
                start: 0.0,
                end: 5.0,
                kind: LoopKind::Manual,
            },
        )
        .unwrap();

        // set_section_notes with a non-empty NotesDoc
        let doc = crate::notes::NotesDoc {
            blocks: vec![crate::notes::Block::Text { text: "hi".into() }],
        };
        lib.set_section_notes(song.id, "intro", &doc).unwrap();

        // save_analysis with bpm Some(120.0)
        let analysis = Analysis {
            bpm: Some(120.0),
            beats: vec![],
            downbeats: vec![],
            sections: vec![],
            engine: "test".into(),
        };
        lib.save_analysis(song.id, &analysis).unwrap();
        assert!(lib.has_analysis(song.id));

        // reload from disk and verify everything survived
        let lib2 = Library::load(lib_dir.path().to_path_buf()).unwrap();
        assert_eq!(lib2.list_sections(song.id).len(), 1);
        assert_eq!(lib2.list_loops(song.id).len(), 1);
        assert_eq!(lib2.list_section_notes(song.id).len(), 1);
        let a2 = lib2.get_analysis(song.id).unwrap();
        assert_eq!(a2.bpm, Some(120.0));
    }
}
