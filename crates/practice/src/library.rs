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
    pub click_guide: bool,
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
            Self::rebase_audio_path(&dir, &mut m.song);
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
            for r in &e.manifest.routines {
                s.insert(r.id.0);
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
            recordings: vec![],
            routines: vec![],
            isolation: Isolation::default(),
            markers: vec![],
            snapshots: vec![],
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

    /// Point a song's audio path at `dir`, keeping only the file name. The
    /// stored path may be stale — a bundle copied from another machine, or a
    /// dir just renamed — and the file name is all that's portable.
    fn rebase_audio_path(dir: &Path, song: &mut Song) {
        if let Some(fname) = Path::new(&song.path).file_name() {
            song.path = dir.join(fname).to_string_lossy().into_owned();
        }
    }

    // ── sections ───────────────────────────────────────────────────────────────

    pub fn list_sections(&self, song_id: SongId) -> Vec<Section> {
        self.entries
            .get(&song_id.0)
            .map(|e| e.manifest.sections.clone())
            .unwrap_or_default()
    }

    /// Toggle a section's click guide and rewrite the bundle manifest.
    pub fn set_section_click_guide(
        &mut self,
        song_id: SongId,
        section_id: SectionId,
        on: bool,
    ) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        if let Some(s) = entry
            .manifest
            .sections
            .iter_mut()
            .find(|s| s.id == section_id)
        {
            s.click_guide = on;
        }
        Self::persist(entry)?;
        Ok(())
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
                click_guide: s.click_guide,
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

    // ── routines ─────────────────────────────────────────────────────────────────

    pub fn list_routines(&self, song_id: SongId) -> Vec<Routine> {
        self.entries
            .get(&song_id.0)
            .map(|e| e.manifest.routines.clone())
            .unwrap_or_default()
    }

    /// Upsert a routine. `id == RoutineId(0)` mints a fresh id and appends;
    /// otherwise the routine with that id is replaced in place (error if no such
    /// routine exists). Returns the stored routine, with its real id.
    pub fn save_routine(&mut self, song_id: SongId, mut routine: Routine) -> Result<Routine> {
        if routine.id.0 == 0 {
            routine.id = RoutineId(Self::fresh_id(&mut self.used_ids()));
            let entry = self.entry_mut(song_id)?;
            entry.manifest.routines.push(routine.clone());
            Self::persist(entry)?;
            return Ok(routine);
        }
        let entry = self.entry_mut(song_id)?;
        let slot = entry
            .manifest
            .routines
            .iter_mut()
            .find(|r| r.id == routine.id)
            .ok_or(crate::error::Error::NotFound)?;
        *slot = routine.clone();
        Self::persist(entry)?;
        Ok(routine)
    }

    pub fn delete_routine(&mut self, song_id: SongId, id: RoutineId) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.routines.retain(|r| r.id != id);
        Self::persist(entry)?;
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

    // ── recordings ───────────────────────────────────────────────────────────────

    pub fn recordings(&self, song_id: SongId) -> Vec<Recording> {
        self.entries
            .get(&song_id.0)
            .map(|e| e.manifest.recordings.clone())
            .unwrap_or_default()
    }

    /// Replace the song's recordings and write the manifest through to disk.
    pub fn set_recordings(&mut self, song_id: SongId, recordings: Vec<Recording>) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.recordings = recordings;
        Self::persist(entry)
    }

    // ── isolation ──────────────────────────────────────────────────────────────

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

    // ── markers ────────────────────────────────────────────────────────────────

    pub fn list_markers(&self, song_id: SongId) -> Vec<Marker> {
        self.entries
            .get(&song_id.0)
            .map(|e| e.manifest.markers.clone())
            .unwrap_or_default()
    }

    pub fn marker(&self, song_id: SongId, slot: u32) -> Option<Marker> {
        self.entries
            .get(&song_id.0)?
            .manifest
            .markers
            .iter()
            .copied()
            .find(|m| m.slot == slot)
    }

    /// Set (or overwrite) the marker in `slot`. Kept sorted by slot.
    pub fn set_marker(&mut self, song_id: SongId, slot: u32, pos: f64) -> Result<Marker> {
        let entry = self.entry_mut(song_id)?;
        let m = Marker { slot, pos };
        match entry.manifest.markers.iter_mut().find(|x| x.slot == slot) {
            Some(x) => *x = m,
            None => {
                entry.manifest.markers.push(m);
                entry.manifest.markers.sort_by_key(|x| x.slot);
            }
        }
        Self::persist(entry)?;
        Ok(m)
    }

    pub fn clear_marker(&mut self, song_id: SongId, slot: u32) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.markers.retain(|m| m.slot != slot);
        Self::persist(entry)
    }

    // ── isolation snapshots ───────────────────────────────────────────────────

    pub fn list_snapshots(&self, song_id: SongId) -> Vec<IsolationSnapshot> {
        self.entries
            .get(&song_id.0)
            .map(|e| e.manifest.snapshots.clone())
            .unwrap_or_default()
    }

    pub fn snapshot(&self, song_id: SongId, slot: u32) -> Option<IsolationSnapshot> {
        self.entries
            .get(&song_id.0)?
            .manifest
            .snapshots
            .iter()
            .find(|s| s.slot == slot)
            .cloned()
    }

    /// Save (or overwrite) the snapshot in `slot`. Kept sorted by slot.
    pub fn save_snapshot(
        &mut self,
        song_id: SongId,
        slot: u32,
        name: Option<String>,
        state: Isolation,
    ) -> Result<IsolationSnapshot> {
        let entry = self.entry_mut(song_id)?;
        let s = IsolationSnapshot { slot, name, state };
        match entry.manifest.snapshots.iter_mut().find(|x| x.slot == slot) {
            Some(x) => *x = s.clone(),
            None => {
                entry.manifest.snapshots.push(s.clone());
                entry.manifest.snapshots.sort_by_key(|x| x.slot);
            }
        }
        Self::persist(entry)?;
        Ok(s)
    }

    pub fn clear_snapshot(&mut self, song_id: SongId, slot: u32) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        entry.manifest.snapshots.retain(|s| s.slot != slot);
        Self::persist(entry)
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

        // Update metadata, rebase the audio path onto the (possibly new) dir,
        // and write the manifest through to disk.
        entry.manifest.song.title = title.to_owned();
        entry.manifest.song.artist = artist.map(str::to_owned);
        if moved {
            Self::rebase_audio_path(&entry.dir, &mut entry.manifest.song);
        }
        Self::persist(entry)?;
        Ok(entry.manifest.song.clone())
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
                recordings: vec![],
                routines: vec![],
                isolation: Isolation::default(),
                markers: vec![],
                snapshots: vec![],
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
            recordings: vec![],
            routines: vec![],
            isolation: Isolation::default(),
            markers: vec![],
            snapshots: vec![],
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
                    click_guide: false,
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

    #[test]
    fn routines_upsert_persist_and_reload() {
        let lib_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let audio_src = src_dir.path().join("track.flac");
        std::fs::write(&audio_src, b"AUDIO").unwrap();

        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "Test Song", None, "hash123", 60.0)
            .unwrap();

        // Save a new (id 0) routine with two blocks; a fresh id is minted.
        let routine = Routine {
            id: RoutineId(0),
            name: "verse drill".into(),
            blocks: vec![
                Block {
                    span: Span {
                        start: 0.0,
                        end: 8.0,
                    },
                    mix: Mix {
                        bass_focus: false,
                        stems: [0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
                    },
                    speed: 1.0,
                    passes: 1,
                    lead_in_beats: 0,
                    count_in: CountIn::default(),
                    name: Some("bass".into()),
                },
                Block {
                    span: Span {
                        start: 0.0,
                        end: 8.0,
                    },
                    mix: Mix::default(),
                    speed: 0.85,
                    passes: 2,
                    lead_in_beats: 4,
                    count_in: CountIn {
                        beats: 4,
                        loop_mode: CountInMode::Every,
                    },
                    name: None,
                },
            ],
        };
        let saved = lib.save_routine(song.id, routine).unwrap();
        assert_ne!(saved.id, RoutineId(0), "a fresh id must be minted");
        assert_eq!(saved.blocks.len(), 2);

        // Update in place (same id): rename + drop a block.
        let mut edited = saved.clone();
        edited.name = "verse drill v2".into();
        edited.blocks.truncate(1);
        lib.save_routine(song.id, edited).unwrap();
        assert_eq!(lib.list_routines(song.id).len(), 1, "upsert, not append");

        // Reload from disk: the surviving routine round-trips field-for-field.
        let lib2 = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let routines = lib2.list_routines(song.id);
        assert_eq!(routines.len(), 1);
        let r = &routines[0];
        assert_eq!(r.id, saved.id);
        assert_eq!(r.name, "verse drill v2");
        assert_eq!(r.blocks.len(), 1);
        assert_eq!(r.blocks[0].mix.stems, [0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);

        // Delete it.
        lib2_delete_check(lib_dir.path(), song.id, saved.id);
    }

    fn lib2_delete_check(lib_dir: &Path, song_id: SongId, id: RoutineId) {
        let mut lib = Library::load(lib_dir.to_path_buf()).unwrap();
        lib.delete_routine(song_id, id).unwrap();
        assert!(lib.list_routines(song_id).is_empty());
        let reloaded = Library::load(lib_dir.to_path_buf()).unwrap();
        assert!(reloaded.list_routines(song_id).is_empty());
    }

    #[test]
    fn set_section_click_guide_persists_and_reloads() {
        let lib_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let audio_src = src_dir.path().join("track.flac");
        std::fs::write(&audio_src, b"AUDIO").unwrap();

        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "Click Song", None, "clickhash", 60.0)
            .unwrap();
        let sections = lib
            .replace_sections(
                song.id,
                &[NewSection {
                    name: "intro",
                    start: 0.0,
                    end: 10.0,
                    position: 0,
                    click_guide: false,
                }],
            )
            .unwrap();
        let sec_id = sections[0].id;
        assert!(!sections[0].click_guide, "starts off");

        lib.set_section_click_guide(song.id, sec_id, true).unwrap();
        let live = lib.list_sections(song.id);
        assert!(live[0].click_guide, "flag is on in-memory");

        // reload from disk to prove it persisted to the manifest
        let lib2 = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let reloaded = lib2.list_sections(song.id);
        assert!(reloaded[0].click_guide, "flag survived reload");
        assert_eq!(reloaded[0].id, sec_id);
    }

    #[test]
    fn replace_sections_carries_click_guide() {
        let lib_dir = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();
        let audio_src = src_dir.path().join("track.flac");
        std::fs::write(&audio_src, b"AUDIO").unwrap();

        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "Replace Song", None, "replacehash", 60.0)
            .unwrap();

        // Replace with two sections; mark the second one's click guide on.
        let saved = lib
            .replace_sections(
                song.id,
                &[
                    NewSection {
                        name: "intro",
                        start: 0.0,
                        end: 10.0,
                        position: 0,
                        click_guide: false,
                    },
                    NewSection {
                        name: "verse",
                        start: 10.0,
                        end: 20.0,
                        position: 1,
                        click_guide: true,
                    },
                ],
            )
            .unwrap();
        assert_eq!(saved.len(), 2);
        assert!(!saved[0].click_guide, "intro stays unmarked");
        assert!(saved[1].click_guide, "verse keeps its mark through replace");

        // And it persists to the manifest.
        let lib2 = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let reloaded = lib2.list_sections(song.id);
        assert!(!reloaded[0].click_guide);
        assert!(
            reloaded[1].click_guide,
            "flag survived reload after replace"
        );
    }

    // ── rename tracks the bundle dir ──

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
        assert!(
            new_dir.join("audio.flac").exists(),
            "audio moved with the dir"
        );
        assert_eq!(updated.path, new_dir.join("audio.flac").to_string_lossy());
        assert_eq!(lib.bundle_dir(song.id).unwrap(), new_dir);
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

        let updated = lib.update_song(song2.id, "Taken", None).unwrap();

        let disambiguated = lib_dir.path().join("Taken-2");
        assert!(disambiguated.is_dir(), "collision disambiguates to -2");
        assert_eq!(lib.bundle_dir(song2.id).unwrap(), disambiguated);
        assert_eq!(
            updated.path,
            disambiguated.join("audio.flac").to_string_lossy()
        );
    }

    #[test]
    fn update_with_unchanged_slug_leaves_dir_in_place() {
        let src_dir = tempfile::tempdir().unwrap();
        let lib_dir = tempfile::tempdir().unwrap();
        let audio_src = src_dir.path().join("orig.flac");
        std::fs::write(&audio_src, b"X").unwrap();

        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "Same", Some("Band"), "h", 1.0)
            .unwrap();
        let dir = lib_dir.path().join("Same \u{2014} Band");
        let path_before = lib.bundle_dir(song.id).unwrap();

        let updated = lib.update_song(song.id, "Same", Some("Band")).unwrap();

        assert_eq!(lib.bundle_dir(song.id).unwrap(), path_before);
        assert!(dir.is_dir());
        assert_eq!(updated.path, dir.join("audio.flac").to_string_lossy());
    }

    #[test]
    fn isolation_persists_and_reloads() {
        let src_dir = tempfile::tempdir().unwrap();
        let lib_dir = tempfile::tempdir().unwrap();
        let audio_src = src_dir.path().join("orig.flac");
        std::fs::write(&audio_src, b"X").unwrap();

        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "Iso", Some("Band"), "h", 1.0)
            .unwrap();

        let iso = Isolation {
            bass_focus: true,
            levels: vec![80, 0, 100, 100, 100, 50],
            mutes: vec![false, true, false, false, false, false],
            solos: vec![false, false, true, false, false, false],
        };
        lib.set_isolation(song.id, iso.clone()).unwrap();

        // in-memory: normalized (already STEM_COUNT-long here, so identical)
        assert_eq!(lib.get_isolation(song.id), iso.normalized());

        // on disk: the manifest carries it verbatim
        let dir = lib.bundle_dir(song.id).unwrap();
        let m = bundle::read_manifest(&dir).unwrap();
        assert_eq!(m.isolation, iso);
    }

    #[test]
    fn isolation_defaults_when_absent() {
        let src_dir = tempfile::tempdir().unwrap();
        let lib_dir = tempfile::tempdir().unwrap();
        let audio_src = src_dir.path().join("orig.flac");
        std::fs::write(&audio_src, b"X").unwrap();

        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "Iso", Some("Band"), "h", 1.0)
            .unwrap();

        assert_eq!(lib.get_isolation(song.id), Isolation::default());
    }

    // ── Task 1: markers ──

    /// A fresh library with one indexed song, for tests that don't care about
    /// bundle placement. Returns the backing `TempDir` guards alongside —
    /// callers must hold them for the test's duration, or their on-disk dirs
    /// get removed out from under the library (mirrors `isolation_persists_and_reloads`).
    fn lib_with_song() -> (Library, SongId, tempfile::TempDir, tempfile::TempDir) {
        let src_dir = tempfile::tempdir().unwrap();
        let lib_dir = tempfile::tempdir().unwrap();
        let audio_src = src_dir.path().join("orig.flac");
        std::fs::write(&audio_src, b"X").unwrap();

        let mut lib = Library::load(lib_dir.path().to_path_buf()).unwrap();
        let song = lib
            .create_song(&audio_src, "Markers", Some("Band"), "h", 1.0)
            .unwrap();
        (lib, song.id, src_dir, lib_dir)
    }

    #[test]
    fn markers_set_overwrite_clear_and_sort() {
        let (mut lib, song_id, _src_dir, _lib_dir) = lib_with_song();
        lib.set_marker(song_id, 3, 30.0).unwrap();
        lib.set_marker(song_id, 1, 10.0).unwrap();
        assert_eq!(
            lib.list_markers(song_id)
                .iter()
                .map(|m| m.slot)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        lib.set_marker(song_id, 3, 33.0).unwrap(); // overwrite, not duplicate
        assert_eq!(lib.marker(song_id, 3).unwrap().pos, 33.0);
        assert_eq!(lib.list_markers(song_id).len(), 2);
        lib.clear_marker(song_id, 1).unwrap();
        assert!(lib.marker(song_id, 1).is_none());

        // on disk: the manifest carries the surviving marker
        let dir = lib.bundle_dir(song_id).unwrap();
        let m = bundle::read_manifest(&dir).unwrap();
        assert_eq!(m.markers, lib.list_markers(song_id));
    }

    // ── Task 3: isolation snapshots ──

    #[test]
    fn snapshots_save_overwrite_clear_and_sort() {
        let (mut lib, song_id, _src_dir, _lib_dir) = lib_with_song();
        lib.save_snapshot(song_id, 2, None, Isolation::default())
            .unwrap();
        lib.save_snapshot(song_id, 1, Some("full".into()), Isolation::default())
            .unwrap();
        assert_eq!(
            lib.list_snapshots(song_id)
                .iter()
                .map(|s| s.slot)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        let mut alt = Isolation::default();
        alt.mutes[0] = true;
        lib.save_snapshot(song_id, 2, None, alt.clone()).unwrap(); // overwrite
        assert_eq!(lib.snapshot(song_id, 2).unwrap().state, alt);
        assert_eq!(lib.list_snapshots(song_id).len(), 2);
        lib.clear_snapshot(song_id, 1).unwrap();
        assert!(lib.snapshot(song_id, 1).is_none());

        // on disk: the manifest carries the surviving snapshot
        let dir = lib.bundle_dir(song_id).unwrap();
        let m = bundle::read_manifest(&dir).unwrap();
        assert_eq!(m.snapshots, lib.list_snapshots(song_id));
    }
}
