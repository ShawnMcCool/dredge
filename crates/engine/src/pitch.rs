//! Monophonic pitch detection for the tuner: McLeod pitch detection over a
//! window of mono samples, plus an interleaved-stereo entry point that downmixes
//! using the engine's capture format.

use crate::buffer::{CHANNELS, SAMPLE_RATE};
use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;

/// Detection window in samples. ~85 ms at 48 kHz — enough periods to resolve the
/// guitar low E (82 Hz) reliably while staying responsive.
pub const WINDOW: usize = 4096;

/// Energy gate: minimum window sum-of-squares to attempt detection. Set low so
/// weak / decaying higher strings still register — characterization showed clean
/// tones detect with clarity 0.86–0.96 right down to the gate, and detection cut
/// off *exactly* at the old 5.0 (≈amp 0.05), which is why softly-plucked high
/// strings vanished. Noise rejection is the *clarity* gate's job, not this one
/// (verified: white noise at power 13.5 — far above a quiet tone — is rejected),
/// so we can drop this to ~0.5 (≈amp 0.016) to track quieter strings. Low enough
/// to follow a decaying note, high enough to ignore near-silence / faint hum.
const POWER_THRESHOLD: f32 = 0.5;
/// How periodic the signal must be to count as a pitch (McLeod NSDF peak). This
/// is the real noise gate: non-periodic signal scores low here regardless of how
/// loud it is. 0.5 admits real (harmonic-rich) strings while rejecting noise.
const CLARITY_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchReading {
    pub hz: f32,
    pub clarity: f32,
}

/// Average channels down to mono. `channels <= 1` returns a copy.
pub fn downmix_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Detect the fundamental over the last `WINDOW` mono samples. None when the
/// signal is too short, too quiet, or not pitched enough.
pub fn detect(mono: &[f32], sample_rate: u32) -> Option<PitchReading> {
    if mono.len() < WINDOW {
        return None;
    }
    let window = &mono[mono.len() - WINDOW..];
    // padding = WINDOW/2: internal FFT buffer headroom for the autocorrelation.
    let mut detector = McLeodDetector::new(WINDOW, WINDOW / 2);
    detector
        .get_pitch(
            window,
            sample_rate as usize,
            POWER_THRESHOLD,
            CLARITY_THRESHOLD,
        )
        .map(|p| PitchReading {
            hz: p.frequency,
            clarity: p.clarity,
        })
}

/// Detect from an interleaved capture snapshot, downmixing with the engine's
/// channel count and detecting at the engine sample rate.
pub fn detect_interleaved(interleaved: &[f32]) -> Option<PitchReading> {
    let mono = downmix_mono(interleaved, CHANNELS);
    detect(&mono, SAMPLE_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(hz: f32, n: usize, rate: u32) -> Vec<f32> {
        (0..n)
            .map(|i| (TAU * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn detects_a440_within_a_hertz() {
        let signal = sine(440.0, WINDOW, SAMPLE_RATE);
        let r = detect(&signal, SAMPLE_RATE).expect("should detect a clean sine");
        assert!((r.hz - 440.0).abs() < 1.0, "got {}", r.hz);
        assert!(r.clarity > 0.9, "clarity {}", r.clarity);
    }

    #[test]
    fn detects_low_e_82hz() {
        let signal = sine(82.41, WINDOW, SAMPLE_RATE);
        let r = detect(&signal, SAMPLE_RATE).expect("should detect low E");
        assert!((r.hz - 82.41).abs() < 1.5, "got {}", r.hz);
    }

    #[test]
    fn silence_returns_none() {
        let signal = vec![0.0_f32; WINDOW];
        assert!(detect(&signal, SAMPLE_RATE).is_none());
    }

    #[test]
    fn detects_weak_high_string() {
        // A softly-plucked / decaying high string (amp 0.02, window power ~0.8).
        // The old power gate (5.0) rejected this — the bug. It must detect now.
        let signal: Vec<f32> = sine(329.6, WINDOW, SAMPLE_RATE)
            .iter()
            .map(|s| s * 0.02)
            .collect();
        let r = detect(&signal, SAMPLE_RATE).expect("weak high E should detect");
        assert!((r.hz - 329.6).abs() < 2.0, "got {}", r.hz);
    }

    #[test]
    fn rejects_loud_noise() {
        // White noise louder than a detectable quiet tone must still be rejected
        // — the clarity gate, not the power gate, is what guards against noise.
        let mut rng: u32 = 12345;
        let signal: Vec<f32> = (0..WINDOW)
            .map(|_| {
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                ((rng >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * 0.1
            })
            .collect();
        assert!(detect(&signal, SAMPLE_RATE).is_none());
    }

    #[test]
    fn downmix_averages_stereo() {
        // frames: (1.0, 3.0) -> 2.0 ; (0.0, 0.0) -> 0.0
        assert_eq!(downmix_mono(&[1.0, 3.0, 0.0, 0.0], 2), vec![2.0, 0.0]);
    }

    #[test]
    fn detect_interleaved_matches_mono_path() {
        // Duplicate a mono A440 into both stereo channels: downmix is identity,
        // so detect_interleaved must agree with detect on the mono signal.
        let mono = sine(440.0, WINDOW, SAMPLE_RATE);
        let stereo: Vec<f32> = mono.iter().flat_map(|&s| [s, s]).collect();
        let r = detect_interleaved(&stereo).expect("should detect A440 from stereo");
        assert!((r.hz - 440.0).abs() < 1.0, "got {}", r.hz);
    }
}
