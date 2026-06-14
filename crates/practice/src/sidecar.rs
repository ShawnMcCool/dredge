use crate::error::Result;
use crate::model::{LoopRegion, Plan, Section, Song};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Everything a user could lose, mirrored as plain JSON next to the audio
/// file: `<audio path>.earworm.json`. Written atomically (tmp + rename).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub version: u32,
    pub song: Song,
    pub sections: Vec<Section>,
    pub loops: Vec<LoopRegion>,
    pub plans: Vec<Plan>,
}

pub fn sidecar_path(audio_path: &Path) -> PathBuf {
    let mut os = audio_path.as_os_str().to_owned();
    os.push(".earworm.json");
    PathBuf::from(os)
}

pub fn write_sidecar(s: &Sidecar) -> Result<PathBuf> {
    let path = sidecar_path(Path::new(&s.song.path));
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

pub fn read_sidecar(audio_path: &Path) -> Result<Option<Sidecar>> {
    let path = sidecar_path(audio_path);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn sample(dir: &Path) -> Sidecar {
        Sidecar {
            version: 1,
            song: Song {
                id: SongId(1),
                title: "T".into(),
                artist: None,
                path: dir.join("song.flac").to_string_lossy().into_owned(),
                file_hash: "h".into(),
                duration_secs: 10.0,
            },
            sections: vec![],
            loops: vec![LoopRegion {
                id: LoopId(1),
                song_id: SongId(1),
                name: "riff".into(),
                name_override: None,
                start: 1.0,
                end: 2.0,
                kind: LoopKind::Manual,
            }],
            plans: vec![],
        }
    }

    #[test]
    fn path_appends_earworm_json() {
        assert_eq!(
            sidecar_path(Path::new("/x/song.flac")),
            PathBuf::from("/x/song.flac.earworm.json")
        );
    }

    #[test]
    fn write_then_read_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let s = sample(dir.path());
        let written_to = write_sidecar(&s).unwrap();
        assert!(written_to.exists());
        let back = read_sidecar(Path::new(&s.song.path)).unwrap().unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn missing_sidecar_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_sidecar(&dir.path().join("nope.flac"))
            .unwrap()
            .is_none());
    }

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

    #[test]
    fn no_partial_file_left_on_write() {
        // atomicity contract: tmp file is renamed, never left behind
        let dir = tempfile::tempdir().unwrap();
        let s = sample(dir.path());
        write_sidecar(&s).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["song.flac.earworm.json".to_string()]);
    }
}
