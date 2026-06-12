//! Rolling capture buffer: the last N seconds of interleaved stereo audio.

use crate::buffer::{CHANNELS, SAMPLE_RATE};

fn secs_to_frames(secs: f64) -> usize {
    // round, not truncate: secs values derived from frame counts must map back
    // to the same frame count despite f64 representation error
    (secs * SAMPLE_RATE as f64).round() as usize
}

/// Rolling buffer of the last `capacity_frames` of interleaved stereo audio.
pub struct RollingRing {
    data: Vec<f32>, // capacity_frames * CHANNELS, allocated once
    capacity_frames: usize,
    write_frame: usize, // next write position (frame index, wraps)
    filled_frames: usize,
}

impl RollingRing {
    pub fn with_secs(secs: f64) -> Self {
        let capacity_frames = secs_to_frames(secs);
        Self {
            data: vec![0.0; capacity_frames * CHANNELS],
            capacity_frames,
            write_frame: 0,
            filled_frames: 0,
        }
    }

    pub fn filled_secs(&self) -> f64 {
        self.filled_frames as f64 / SAMPLE_RATE as f64
    }

    /// Push interleaved stereo samples (any length; may exceed capacity).
    pub fn push(&mut self, interleaved: &[f32]) {
        if self.capacity_frames == 0 {
            return;
        }
        let frames = interleaved.len() / CHANNELS;
        // a push larger than capacity: only the tail can survive
        let skip = frames.saturating_sub(self.capacity_frames);
        let src = &interleaved[skip * CHANNELS..frames * CHANNELS];
        for frame in src.chunks_exact(CHANNELS) {
            let dst = self.write_frame * CHANNELS;
            self.data[dst..dst + CHANNELS].copy_from_slice(frame);
            self.write_frame = (self.write_frame + 1) % self.capacity_frames;
        }
        self.filled_frames = (self.filled_frames + frames).min(self.capacity_frames);
    }

    /// Last `secs` (clamped to what's filled), chronological, interleaved.
    pub fn snapshot_last(&self, secs: f64) -> Vec<f32> {
        let want = secs_to_frames(secs).min(self.filled_frames);
        if want == 0 {
            return Vec::new();
        }
        // oldest wanted frame sits `want` frames behind the write head
        let start = (self.write_frame + self.capacity_frames - want) % self.capacity_frames;
        let mut out = Vec::with_capacity(want * CHANNELS);
        let first = (self.capacity_frames - start).min(want);
        out.extend_from_slice(&self.data[start * CHANNELS..(start + first) * CHANNELS]);
        out.extend_from_slice(&self.data[..(want - first) * CHANNELS]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(vals: &[f32]) -> Vec<f32> {
        vals.iter().flat_map(|v| [*v, *v]).collect() // mono value -> stereo frame
    }

    fn firsts(inter: &[f32]) -> Vec<f32> {
        inter.iter().step_by(CHANNELS).copied().collect()
    }

    #[test]
    fn fills_and_snapshots_chronologically() {
        let mut r = RollingRing::with_secs(1.0); // 48_000 frames
        r.push(&frames(&[1.0, 2.0, 3.0]));
        assert_eq!(firsts(&r.snapshot_last(1.0)), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn overwrites_oldest_when_full() {
        let mut r = RollingRing::with_secs(4.0 / SAMPLE_RATE as f64); // 4 frames
        r.push(&frames(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
        assert_eq!(firsts(&r.snapshot_last(10.0)), vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn snapshot_subset_returns_most_recent() {
        let mut r = RollingRing::with_secs(1.0);
        let vals: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        r.push(&frames(&vals));
        let last = r.snapshot_last(10.0 / SAMPLE_RATE as f64); // last 10 frames
        assert_eq!(
            firsts(&last),
            (990..1000).map(|i| i as f32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn push_larger_than_capacity_keeps_tail() {
        let mut r = RollingRing::with_secs(3.0 / SAMPLE_RATE as f64); // 3 frames
        r.push(&frames(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]));
        assert_eq!(firsts(&r.snapshot_last(1.0)), vec![5.0, 6.0, 7.0]);
    }

    #[test]
    fn filled_secs_caps_at_capacity() {
        let mut r = RollingRing::with_secs(2.0 / SAMPLE_RATE as f64);
        r.push(&frames(&[1.0]));
        assert!(r.filled_secs() < 2.0 / SAMPLE_RATE as f64 + f64::EPSILON);
        r.push(&frames(&[2.0, 3.0, 4.0]));
        assert_eq!(r.filled_frames, 2);
    }
}
