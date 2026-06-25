//! Overdub layers: recorded performances placed on the song timeline by
//! absolute source frame and summed into the *pre-stretch* mix buffer, so they
//! ride the song's single stretcher (aligned + pitch-preserved at any tempo).

use crate::buffer::{SongBuffer, CHANNELS};
use std::sync::Arc;

/// One recorded take placed on the song timeline.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Interleaved stereo f32 at 48 kHz (the recorded audio).
    pub samples: Arc<SongBuffer>,
    /// Absolute source frame where `samples` frame 0 sits. May be negative
    /// after latency/nudge compensation; the leading portion is then clipped.
    pub start_frame: i64,
    /// Playback gain (0.0..=1.5).
    pub gain: f32,
    /// When true the layer contributes nothing.
    pub muted: bool,
}

/// Add every active layer's audio for the contiguous source-frame window
/// `[src_start, src_start + out.len()/CHANNELS)` into `out` (which already holds
/// the looper's mixed track frames for the same window). Frames outside a
/// layer's extent contribute nothing.
pub fn mix_layers(layers: &[Layer], src_start: usize, out: &mut [f32]) {
    let frames = out.len() / CHANNELS;
    for layer in layers {
        if layer.muted || layer.gain == 0.0 {
            continue;
        }
        let len = layer.samples.frames() as i64;
        for i in 0..frames {
            let abs = src_start as i64 + i as i64;
            let local = abs - layer.start_frame;
            if local < 0 || local >= len {
                continue;
            }
            let li = (local as usize) * CHANNELS;
            let oi = i * CHANNELS;
            out[oi] += layer.samples.data[li] * layer.gain;
            out[oi + 1] += layer.samples.data[li + 1] * layer.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(frames: usize, v: f32) -> Arc<SongBuffer> {
        Arc::new(SongBuffer {
            data: vec![v; frames * CHANNELS],
        })
    }

    fn layer(start_frame: i64, frames: usize, v: f32) -> Layer {
        Layer {
            samples: constant(frames, v),
            start_frame,
            gain: 1.0,
            muted: false,
        }
    }

    #[test]
    fn places_layer_at_its_offset_and_leaves_the_rest_untouched() {
        let layers = vec![layer(100, 10, 0.5)];
        let mut out = vec![0.0f32; 6 * CHANNELS]; // window [98, 104)
        mix_layers(&layers, 98, &mut out);
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1 * CHANNELS], 0.0);
        for f in 2..6 {
            assert!((out[f * CHANNELS] - 0.5).abs() < 1e-6, "frame {f}");
            assert!((out[f * CHANNELS + 1] - 0.5).abs() < 1e-6, "frame {f}");
        }
    }

    #[test]
    fn adds_onto_existing_track_audio() {
        let layers = vec![layer(0, 4, 0.25)];
        let mut out = vec![0.1f32; 4 * CHANNELS];
        mix_layers(&layers, 0, &mut out);
        for s in &out {
            assert!((s - 0.35).abs() < 1e-6, "got {s}");
        }
    }

    #[test]
    fn muted_layer_contributes_nothing() {
        let mut l = layer(0, 4, 0.5);
        l.muted = true;
        let mut out = vec![0.0f32; 4 * CHANNELS];
        mix_layers(&[l], 0, &mut out);
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn gain_scales_contribution() {
        let mut l = layer(0, 4, 0.4);
        l.gain = 0.5;
        let mut out = vec![0.0f32; 4 * CHANNELS];
        mix_layers(&[l], 0, &mut out);
        for s in &out {
            assert!((s - 0.2).abs() < 1e-6, "got {s}");
        }
    }

    #[test]
    fn negative_start_frame_clips_the_leading_portion() {
        let layers = vec![layer(-2, 8, 0.5)];
        let mut out = vec![0.0f32; 4 * CHANNELS];
        mix_layers(&layers, 0, &mut out);
        for s in &out {
            assert!((s - 0.5).abs() < 1e-6, "got {s}");
        }
    }
}
