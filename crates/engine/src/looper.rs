use crate::buffer::{StemSet, CHANNELS};

pub const XFADE_FRAMES: usize = 480; // 10 ms

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReadInfo {
    pub frames: usize,
    pub wrapped: bool,
    pub finished: bool,
}

/// Reads source frames; when a loop is set, wraps end→start with a
/// linear (equal-gain) crossfade. The crossfade blends the loop tail
/// [end-x, end) with the head [start, start+x); after the blend the
/// position continues from start+x (the head's first x frames were
/// already heard inside the blend), so the first pass is exactly
/// end-start frames and steady-state passes are end-start-x frames.
pub struct Looper {
    set: StemSet,
    pos: usize, // current source frame
    region: Option<(usize, usize)>,
}

impl Looper {
    pub fn new(set: StemSet) -> Self {
        Self {
            set,
            pos: 0,
            region: None,
        }
    }

    pub fn pos_frames(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, frame: usize) {
        self.pos = frame.min(self.set.frames());
    }

    /// Per-stem gain (clamped to 0.0..=1.5); out-of-range stems are ignored.
    pub fn set_gain(&mut self, idx: usize, gain: f32) {
        if let Some(g) = self.set.gains.get_mut(idx) {
            *g = gain.clamp(0.0, 1.5);
        }
    }

    /// Set loop [start, end) in frames; jumps into the region if outside.
    pub fn set_region(&mut self, start: usize, end: usize) {
        let end = end.min(self.set.frames());
        let start = start.min(end);
        self.region = Some((start, end));
        if self.pos < start || self.pos >= end {
            self.pos = start;
        }
    }

    pub fn clear_region(&mut self) {
        self.region = None;
    }

    /// Fill `out` (len = frames*CHANNELS). Returns ReadInfo.
    pub fn read(&mut self, out: &mut [f32]) -> ReadInfo {
        let total = self.set.frames();
        let frames_req = out.len() / CHANNELS;
        let mut info = ReadInfo::default();
        while info.frames < frames_req {
            let remaining = frames_req - info.frames;
            let base = info.frames * CHANNELS;
            match self.region {
                None => {
                    if self.pos >= total {
                        info.finished = true;
                        break;
                    }
                    // contiguous run up to end-of-song
                    let chunk = remaining.min(total - self.pos);
                    self.set
                        .mix_into(self.pos, &mut out[base..base + chunk * CHANNELS]);
                    self.pos += chunk;
                    info.frames += chunk;
                }
                Some((start, end)) => {
                    let len = end - start;
                    if len == 0 {
                        info.finished = true;
                        break;
                    }
                    let xfade = XFADE_FRAMES.min(len / 4);
                    let fade_start = end - xfade;
                    if self.pos < fade_start {
                        // contiguous non-crossfade run up to the fade boundary
                        let chunk = remaining.min(fade_start - self.pos);
                        self.set
                            .mix_into(self.pos, &mut out[base..base + chunk * CHANNELS]);
                        self.pos += chunk;
                        info.frames += chunk;
                    } else {
                        // blend tail with head (linear / equal-gain: the two
                        // sides are correlated material from the same song,
                        // so sum-to-one gains avoid a mid-fade level bulge
                        // and keep the seam continuous). One frame at a time —
                        // the blend reads two source positions.
                        let k = self.pos - fade_start;
                        let t = (k as f32 + 0.5) / xfade.max(1) as f32;
                        let (g_out, g_in) = (1.0 - t, t);
                        let (tl, tr) = self.set.frame(self.pos);
                        let (hl, hr) = self.set.frame(start + k);
                        out[base] = tl * g_out + hl * g_in;
                        out[base + 1] = tr * g_out + hr * g_in;
                        self.pos += 1;
                        info.frames += 1;
                        if self.pos >= end {
                            self.pos = start + xfade;
                            info.wrapped = true;
                            // return at the wrap boundary so the caller
                            // observes every wrap, even for tiny regions
                            return info;
                        }
                    }
                }
            }
        }
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::buffer::SongBuffer;

    /// Buffer where frame i has value i (both channels) — positions are
    /// directly observable in the output.
    fn ramp(frames: usize) -> StemSet {
        let mut data = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            data.push(i as f32);
            data.push(i as f32);
        }
        StemSet::single(SongBuffer { data })
    }

    fn read_frames(l: &mut Looper, n: usize) -> (Vec<f32>, usize) {
        let mut out = vec![0.0f32; n * CHANNELS];
        let mut wraps = 0;
        let mut filled = 0;
        while filled < n {
            let chunk = (n - filled).min(256);
            let info = l.read(&mut out[filled * CHANNELS..(filled + chunk) * CHANNELS]);
            if info.wrapped {
                wraps += 1;
            }
            if info.finished {
                break;
            }
            filled += info.frames;
        }
        (out.iter().step_by(CHANNELS).copied().collect(), wraps)
    }

    #[test]
    fn no_region_plays_through_and_finishes() {
        let mut l = Looper::new(ramp(1000));
        let mut out = vec![0.0f32; 600 * CHANNELS];
        let a = l.read(&mut out);
        assert_eq!(
            a,
            ReadInfo {
                frames: 600,
                wrapped: false,
                finished: false
            }
        );
        assert_eq!(out[599 * 2], 599.0);
        let b = l.read(&mut out);
        assert_eq!(b.frames, 400);
        assert!(b.finished);
    }

    #[test]
    fn loop_period_is_exact() {
        let mut l = Looper::new(ramp(50_000));
        l.set_region(10_000, 20_000); // 10k period
        let (_, wraps) = read_frames(&mut l, 40_000);
        assert_eq!(wraps, 4);
    }

    #[test]
    fn crossfade_is_continuous_and_lands_at_head_plus_xfade() {
        let mut l = Looper::new(ramp(50_000));
        l.set_region(10_000, 20_000);
        // read up to 2 frames past the wrap point: period = 10_000
        let (vals, _) = read_frames(&mut l, 10_001);
        // frame 0 of output = source 10_000
        assert_eq!(vals[0], 10_000.0);
        // last blended frame ends at head+xfade: source 10_000 + XFADE
        let landing = vals[10_000];
        assert!(
            (landing - (10_000 + XFADE_FRAMES) as f32).abs() < 1.5,
            "landing = {landing}"
        );
        // continuity: no sample-to-sample jump bigger than the blend slope bound.
        // ramp slope is 1/frame; blend moves value from ~19,520 to ~10,480
        // over 480 frames → max step ≈ (19520-10480)/480 + 1 ≈ 20.
        for w in vals.windows(2) {
            assert!(
                (w[1] - w[0]).abs() <= 25.0,
                "discontinuity {} -> {}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn outside_region_jumps_to_start() {
        let mut l = Looper::new(ramp(50_000));
        l.seek(40_000);
        l.set_region(10_000, 20_000);
        let mut out = vec![0.0f32; 2];
        l.read(&mut out);
        assert_eq!(out[0], 10_000.0);
    }

    #[test]
    fn setting_stem_gain_zero_mid_read_silences_its_contribution() {
        let constant = |v: f32| SongBuffer {
            data: vec![v; 1000 * CHANNELS],
        };
        let mut l = Looper::new(StemSet::new(vec![constant(0.25), constant(0.5)]));
        let mut out = vec![0.0f32; 10 * CHANNELS];
        l.read(&mut out);
        assert!((out[0] - 0.75).abs() < 1e-6, "mix = {}", out[0]);
        l.set_gain(1, 0.0);
        l.read(&mut out);
        assert!((out[0] - 0.25).abs() < 1e-6, "mix = {}", out[0]);
        // out-of-range index is ignored, not a panic
        l.set_gain(7, 0.0);
    }

    #[test]
    fn tiny_region_shrinks_crossfade_instead_of_breaking() {
        let mut l = Looper::new(ramp(50_000));
        l.set_region(100, 300); // 200-frame loop < 2*XFADE → xfade shrinks to 50
        let (_, wraps) = read_frames(&mut l, 2_000);
        // first cycle is 200 frames (start..end); after each wrap the
        // position resumes at start+xfade, so steady-state cycles are
        // len - xfade = 150 frames: wraps at 200, 350, ..., 2000 = 13
        assert_eq!(wraps, 13);
    }
}
