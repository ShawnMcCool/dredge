use crate::buffer::{SongBuffer, CHANNELS};
use std::sync::Arc;

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
/// position continues from start+x, so the loop period is exactly
/// end-start frames.
pub struct Looper {
    buf: Arc<SongBuffer>,
    pos: usize, // current source frame
    region: Option<(usize, usize)>,
}

impl Looper {
    pub fn new(buf: Arc<SongBuffer>) -> Self {
        Self {
            buf,
            pos: 0,
            region: None,
        }
    }

    pub fn pos_frames(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, frame: usize) {
        self.pos = frame.min(self.buf.frames());
    }

    /// Set loop [start, end) in frames; jumps into the region if outside.
    pub fn set_region(&mut self, start: usize, end: usize) {
        let end = end.min(self.buf.frames());
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
        let total = self.buf.frames();
        let frames_req = out.len() / CHANNELS;
        let mut info = ReadInfo::default();
        for f in 0..frames_req {
            match self.region {
                None => {
                    if self.pos >= total {
                        info.finished = true;
                        break;
                    }
                    let i = self.pos * CHANNELS;
                    out[f * CHANNELS] = self.buf.data[i];
                    out[f * CHANNELS + 1] = self.buf.data[i + 1];
                    self.pos += 1;
                }
                Some((start, end)) => {
                    let len = end - start;
                    if len == 0 {
                        info.finished = true;
                        break;
                    }
                    let xfade = XFADE_FRAMES.min(len / 4);
                    let fade_start = end - xfade;
                    let i = self.pos * CHANNELS;
                    if self.pos >= fade_start {
                        // blend tail with head (linear / equal-gain: the two
                        // sides are correlated material from the same song,
                        // so sum-to-one gains avoid a mid-fade level bulge
                        // and keep the seam continuous)
                        let k = self.pos - fade_start;
                        let t = (k as f32 + 0.5) / xfade.max(1) as f32;
                        let (g_out, g_in) = (1.0 - t, t);
                        let j = (start + k) * CHANNELS;
                        out[f * CHANNELS] = self.buf.data[i] * g_out + self.buf.data[j] * g_in;
                        out[f * CHANNELS + 1] =
                            self.buf.data[i + 1] * g_out + self.buf.data[j + 1] * g_in;
                        self.pos += 1;
                        info.frames += 1;
                        if self.pos >= end {
                            self.pos = start + xfade;
                            info.wrapped = true;
                            // return at the wrap boundary so the caller
                            // observes every wrap, even for tiny regions
                            return info;
                        }
                        continue;
                    } else {
                        out[f * CHANNELS] = self.buf.data[i];
                        out[f * CHANNELS + 1] = self.buf.data[i + 1];
                        self.pos += 1;
                    }
                }
            }
            info.frames += 1;
        }
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Buffer where frame i has value i (both channels) — positions are
    /// directly observable in the output.
    fn ramp(frames: usize) -> Arc<SongBuffer> {
        let mut data = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            data.push(i as f32);
            data.push(i as f32);
        }
        Arc::new(SongBuffer { data })
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
            assert!((w[1] - w[0]).abs() <= 25.0, "discontinuity {} -> {}", w[0], w[1]);
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
