use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::model::{Analysis, LoopRegion, Recording, Section, SectionNote, Song};
use serde::{Deserialize, Serialize};

// ── Constants ──────────────────────────────────────────────────────────────────

pub const MANIFEST_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "dredge.json";

// ── Library-root resolution ────────────────────────────────────────────────────

/// Default library root: the OS music dir + `/dredge`, falling back to
/// `$HOME/Music/dredge` when no music dir is configured.
pub fn default_library_root() -> Option<PathBuf> {
    if let Some(music) = dirs::audio_dir() {
        return Some(music.join("dredge"));
    }
    dirs::home_dir().map(|h| h.join("Music").join("dredge"))
}

// ── BundleManifest ─────────────────────────────────────────────────────────────

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
    #[serde(default)]
    pub recordings: Vec<Recording>,
}

// ── slug ───────────────────────────────────────────────────────────────────────

/// A human-readable, filesystem-safe folder name: `Title — Artist` (or just
/// `Title`). Replaces path-hostile characters with `_`; never empty.
pub fn slug(title: &str, artist: Option<&str>) -> String {
    let base = match artist {
        Some(a) if !a.trim().is_empty() => format!("{} \u{2014} {}", title.trim(), a.trim()),
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

// ── Atomic manifest read/write ─────────────────────────────────────────────────

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

// ── unique_bundle_dir ──────────────────────────────────────────────────────────

/// `root/<slug>`, or `root/<slug>-2`, `-3`… if earlier names already exist on
/// disk. Does not create the directory.
pub fn unique_bundle_dir(root: &Path, slug: &str) -> PathBuf {
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

// ── scan_library ───────────────────────────────────────────────────────────────

/// Load every bundle under `root`. Returns `(bundle_dir, manifest)` pairs.
/// A directory without a readable `dredge.json` is skipped (not an error). A
/// missing root yields an empty list.
pub fn scan_library(root: &Path) -> Result<Vec<(PathBuf, BundleManifest)>> {
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
            Err(_) => continue,
        }
    }
    Ok(out)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LoopId, LoopKind, SongId};

    // ── helpers ──

    fn sample_song() -> Song {
        Song {
            id: SongId(1),
            title: "Weird Fishes".into(),
            artist: Some("Radiohead".into()),
            path: "/tmp/song.flac".into(),
            file_hash: "abc123".into(),
            duration_secs: 300.0,
        }
    }

    fn sample_manifest() -> BundleManifest {
        BundleManifest {
            version: MANIFEST_VERSION,
            song: sample_song(),
            sections: vec![],
            loops: vec![LoopRegion {
                id: LoopId(1),
                song_id: SongId(1),
                name: "intro".into(),
                name_override: None,
                start: 0.0,
                end: 10.0,
                kind: LoopKind::Manual,
            }],
            notes: vec![],
            analysis: None,
            recordings: vec![],
        }
    }

    // ── Task 0.2: default_library_root ──

    #[test]
    fn default_root_ends_with_dredge() {
        let root = default_library_root().expect("should return Some on any sane system");
        assert_eq!(root.file_name().unwrap(), "dredge");
    }

    // ── Task 1.1: BundleManifest roundtrip ──

    #[test]
    fn manifest_json_roundtrips() {
        let m = sample_manifest();
        let bytes = serde_json::to_vec(&m).unwrap();
        let back: BundleManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn manifest_without_recordings_field_still_loads() {
        // Older bundles have no `recordings` key; #[serde(default)] must apply.
        let json = r#"{"version":1,"song":{"id":1,"title":"T","artist":null,
            "path":"/tmp/a.flac","file_hash":"h","duration_secs":1.0}}"#;
        let m: BundleManifest = serde_json::from_str(json).unwrap();
        assert!(m.recordings.is_empty());
    }

    #[test]
    fn recordings_roundtrip_in_manifest() {
        let mut m = sample_manifest();
        m.recordings.push(crate::model::Recording {
            id: crate::model::RecordingId(1),
            name: "take 1".into(),
            file: "recordings/1.wav".into(),
            anchor_frame: 48_000,
            len_frames: 240_000,
            nudge_frames: -120,
            gain: 1.0,
            muted: false,
            created_at: "2026-06-25T12:00:00Z".into(),
        });
        let bytes = serde_json::to_vec(&m).unwrap();
        let back: BundleManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(m, back);
    }

    // ── Task 1.2: slug ──

    #[test]
    fn slug_title_and_artist() {
        assert_eq!(
            slug("Weird Fishes", Some("Radiohead")),
            "Weird Fishes \u{2014} Radiohead"
        );
    }

    #[test]
    fn slug_no_artist() {
        assert_eq!(slug("untitled", None), "untitled");
    }

    #[test]
    fn slug_replaces_slash_in_artist() {
        assert_eq!(slug("T.N.T.", Some("AC/DC")), "T.N.T. \u{2014} AC_DC");
    }

    #[test]
    fn slug_whitespace_only_title_becomes_untitled() {
        assert_eq!(slug("   ", None), "untitled");
    }

    // ── Task 1.3: write_manifest / read_manifest ──

    #[test]
    fn write_then_read_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let m = sample_manifest();
        write_manifest(dir.path(), &m).unwrap();
        let back = read_manifest(dir.path()).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn write_leaves_no_tmp_file() {
        let dir = tempfile::tempdir().unwrap();
        let m = sample_manifest();
        write_manifest(dir.path(), &m).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["dredge.json".to_string()]);
    }

    // ── Task 1.4: unique_bundle_dir ──

    #[test]
    fn unique_dir_suffixes_on_collision() {
        let root = tempfile::tempdir().unwrap();
        let first = unique_bundle_dir(root.path(), "Song");
        assert_eq!(first.file_name().unwrap(), "Song");
        std::fs::create_dir(&first).unwrap();
        let second = unique_bundle_dir(root.path(), "Song");
        assert_eq!(second.file_name().unwrap(), "Song-2");
    }

    // ── Task 1.5: scan_library ──

    #[test]
    fn scan_loads_every_manifest() {
        let root = tempfile::tempdir().unwrap();

        // two proper bundles
        for name in &["bundle-a", "bundle-b"] {
            let bdir = root.path().join(name);
            write_manifest(&bdir, &sample_manifest()).unwrap();
        }

        // a stray non-bundle directory (no dredge.json)
        std::fs::create_dir(root.path().join("not-a-bundle")).unwrap();

        let found = scan_library(root.path()).unwrap();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn scan_missing_root_is_empty() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("nonexistent");
        let found = scan_library(&missing).unwrap();
        assert!(found.is_empty());
    }
}
