pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// Whole song in memory: interleaved stereo f32 at 48 kHz.
#[derive(Debug, Clone, PartialEq)]
pub struct SongBuffer {
    pub data: Vec<f32>,
}

impl SongBuffer {
    pub fn frames(&self) -> usize {
        self.data.len() / CHANNELS
    }
    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / SAMPLE_RATE as f64
    }
}

/// One or more equal-length stems mixed with per-stem gains.
#[derive(Debug, Clone)]
pub struct StemSet {
    pub stems: Vec<std::sync::Arc<SongBuffer>>, // len >= 1; equal frames (pad on construction)
    pub gains: Vec<f32>,                        // same len, 0.0..=1.5
}

impl StemSet {
    pub fn single(buf: SongBuffer) -> Self {
        Self {
            stems: vec![std::sync::Arc::new(buf)],
            gains: vec![1.0],
        }
    }

    /// Pads shorter stems with silence to the longest.
    pub fn new(stems: Vec<SongBuffer>) -> Self {
        let max = stems.iter().map(SongBuffer::frames).max().unwrap_or(0);
        let stems: Vec<_> = stems
            .into_iter()
            .map(|mut s| {
                s.data.resize(max * CHANNELS, 0.0);
                std::sync::Arc::new(s)
            })
            .collect();
        let gains = vec![1.0; stems.len()];
        Self { stems, gains }
    }

    pub fn frames(&self) -> usize {
        self.stems.first().map(|s| s.frames()).unwrap_or(0)
    }

    /// Mixed (left, right) at frame `pos`: sum of stems[i][pos] * gains[i].
    #[inline]
    pub fn frame(&self, pos: usize) -> (f32, f32) {
        let i = pos * CHANNELS;
        let (mut l, mut r) = (0.0, 0.0);
        for (stem, gain) in self.stems.iter().zip(&self.gains) {
            l += stem.data[i] * gain;
            r += stem.data[i + 1] * gain;
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(frames: usize, v: f32) -> SongBuffer {
        SongBuffer {
            data: vec![v; frames * CHANNELS],
        }
    }

    #[test]
    fn new_pads_shorter_stems_to_longest() {
        let set = StemSet::new(vec![constant(100, 0.25), constant(40, 0.5)]);
        assert_eq!(set.frames(), 100);
        // beyond the short stem's original end only the long stem contributes
        let (l, r) = set.frame(50);
        assert_eq!((l, r), (0.25, 0.25));
        // padded region of the short stem reads 0.0 (in bounds, silent)
        let short = &set.stems[1];
        assert_eq!(short.frames(), 100);
        assert_eq!(short.data[50 * CHANNELS], 0.0);
    }

    #[test]
    fn frame_sums_stems_with_gains() {
        let mut set = StemSet::new(vec![constant(10, 0.25), constant(10, 0.5)]);
        set.gains = vec![1.0, 0.5];
        let (l, r) = set.frame(3);
        assert!((l - 0.5).abs() < 1e-6, "l = {l}");
        assert!((r - 0.5).abs() < 1e-6, "r = {r}");
    }

    #[test]
    fn single_wraps_with_unity_gain() {
        let set = StemSet::single(constant(8, 0.3));
        assert_eq!(set.stems.len(), 1);
        assert_eq!(set.gains, vec![1.0]);
        assert_eq!(set.frames(), 8);
        assert_eq!(set.frame(0), (0.3, 0.3));
    }
}
