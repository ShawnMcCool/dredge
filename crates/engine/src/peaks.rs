use crate::buffer::{SongBuffer, CHANNELS};
use serde::{Deserialize, Serialize};

pub const FRAMES_PER_BUCKET: usize = 1024;

/// Per-bucket (min, max) over both channels — what the waveform draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Peaks {
    pub frames_per_bucket: usize,
    pub buckets: Vec<(f32, f32)>,
}

pub fn compute_peaks(buf: &SongBuffer) -> Peaks {
    let buckets = buf
        .data
        .chunks(FRAMES_PER_BUCKET * CHANNELS)
        .map(|chunk| {
            chunk
                .iter()
                .fold((f32::MAX, f32::MIN), |(lo, hi), s| (lo.min(*s), hi.max(*s)))
        })
        .collect();
    Peaks {
        frames_per_bucket: FRAMES_PER_BUCKET,
        buckets,
    }
}

/// Delete the cached peaks for a song hash. A missing cache (or no cache dir)
/// is a no-op — deletion cleanup must not fail on an absent file.
pub fn remove_cache(file_hash: &str) -> std::io::Result<()> {
    let Some(base) = dirs::cache_dir() else {
        return Ok(());
    };
    let path = base.join("earworm/peaks").join(format!("{file_hash}.json"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Cache under ~/.cache/earworm/peaks/<file_hash>.json; load if present.
pub fn load_or_compute(buf: &SongBuffer, file_hash: &str) -> std::io::Result<Peaks> {
    let dir = dirs::cache_dir()
        .ok_or_else(|| std::io::Error::other("no cache dir"))?
        .join("earworm/peaks");
    let path = dir.join(format!("{file_hash}.json"));
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(peaks) = serde_json::from_str::<Peaks>(&text) {
            return Ok(peaks);
        }
        // parse failure → fall through and recompute
    }
    let peaks = compute_peaks(buf);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&path, serde_json::to_string(&peaks)?)?;
    Ok(peaks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_capture_min_and_max_per_bucket() {
        // bucket 0: constant 0.5; bucket 1: constant -0.25
        let mut data = vec![0.5f32; FRAMES_PER_BUCKET * CHANNELS];
        data.extend(vec![-0.25f32; FRAMES_PER_BUCKET * CHANNELS]);
        let p = compute_peaks(&SongBuffer { data });
        assert_eq!(p.buckets.len(), 2);
        assert_eq!(p.buckets[0], (0.5, 0.5));
        assert_eq!(p.buckets[1], (-0.25, -0.25));
    }

    #[test]
    fn partial_final_bucket_included() {
        let data = vec![0.1f32; (FRAMES_PER_BUCKET + 10) * CHANNELS];
        let p = compute_peaks(&SongBuffer { data });
        assert_eq!(p.buckets.len(), 2);
    }

    #[test]
    fn cache_roundtrip() {
        let data = vec![0.3f32; FRAMES_PER_BUCKET * CHANNELS];
        let buf = SongBuffer { data };
        let hash = format!("test-{}", std::process::id());
        let first = load_or_compute(&buf, &hash).unwrap();
        // second call must hit the cache file (delete buf data influence: pass empty buffer)
        let cached = load_or_compute(&SongBuffer { data: vec![] }, &hash).unwrap();
        assert_eq!(first, cached);
        // cleanup
        let dir = dirs::cache_dir().unwrap().join("earworm/peaks");
        let _ = std::fs::remove_file(dir.join(format!("{hash}.json")));
    }

    #[test]
    fn remove_cache_deletes_then_noops() {
        let buf = SongBuffer {
            data: vec![0.2f32; FRAMES_PER_BUCKET * CHANNELS],
        };
        let hash = format!("rm-{}", std::process::id());
        load_or_compute(&buf, &hash).unwrap();
        let path = dirs::cache_dir()
            .unwrap()
            .join("earworm/peaks")
            .join(format!("{hash}.json"));
        assert!(path.exists());

        remove_cache(&hash).unwrap();
        assert!(!path.exists());

        // second remove on the missing file is a clean no-op
        remove_cache(&hash).unwrap();
    }
}
