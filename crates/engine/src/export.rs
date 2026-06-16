//! Offline render for export: take a song's `StemSet` and a `RenderConfig`
//! (the current practice mix — stem gains, speed, pitch, bass focus, and a
//! span) and produce an interleaved-stereo f32 buffer at 48 kHz, baking the
//! whole DSP chain in. Master playback volume is intentionally *not* a field:
//! export reflects the mix, not the monitoring level.
//!
//! The render is pure (no PipeWire, no IO): it slices the set to the requested
//! span and drives a fresh `Pipeline` to its natural end, exactly as live
//! playback would, so what lands on disk matches what you hear.

use crate::buffer::{SongBuffer, StemSet, CHANNELS, SAMPLE_RATE};
use crate::pipeline::{EngineCmd, Pipeline};

/// The mix to bake into an exported file. No `volume` field by design.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Span start in seconds (clamped to the song).
    pub start_secs: f64,
    /// Span end in seconds; `None` renders to the end of the song.
    pub end_secs: Option<f64>,
    /// Playback rate (1.0 = original tempo; <1 slower → longer output).
    pub rate: f64,
    /// Pitch scale (frequency multiplier; 1.0 = original pitch).
    pub pitch_scale: f64,
    /// Bass-focus filter on/off.
    pub bass_focus: bool,
    /// Per-stem gains; empty (or shorter than the stem count) leaves the
    /// untouched stems at unity. Ignored beyond the available stems.
    pub gains: Vec<f32>,
}

/// Output frames pulled from the pipeline per `render()` call.
const RENDER_BLOCK: usize = 1024;

/// Render `set` under `cfg` to an interleaved-stereo f32 buffer at 48 kHz.
pub fn render(set: &StemSet, cfg: &RenderConfig) -> Vec<f32> {
    let total = set.frames();
    let start = secs_to_frames(cfg.start_secs).min(total);
    let end = cfg
        .end_secs
        .map(|e| secs_to_frames(e).min(total))
        .unwrap_or(total)
        .max(start);

    let in_frames = end - start;
    let rate = if cfg.rate > 0.0 { cfg.rate } else { 1.0 };
    // time-stretch maps input→output by 1/rate; this is the deterministic
    // output length, which also bounds the render loop.
    let target = ((in_frames as f64) / rate).round() as usize;
    if target == 0 {
        return Vec::new();
    }

    let mut pipeline = Pipeline::new(slice(set, start, end));
    for (idx, &gain) in cfg.gains.iter().enumerate() {
        pipeline.apply(EngineCmd::SetStemGain { idx, gain });
    }
    pipeline.apply(EngineCmd::SetRate(rate));
    pipeline.apply(EngineCmd::SetPitchScale(cfg.pitch_scale));
    pipeline.apply(EngineCmd::BassFocus(cfg.bass_focus));
    pipeline.apply(EngineCmd::Play);

    let mut out = vec![0.0f32; target * CHANNELS];
    let mut events = Vec::new();
    let mut filled = 0;
    while filled < target {
        let n = RENDER_BLOCK.min(target - filled);
        events.clear();
        pipeline.render(
            &mut out[filled * CHANNELS..(filled + n) * CHANNELS],
            &mut events,
        );
        filled += n;
    }
    out
}

fn secs_to_frames(secs: f64) -> usize {
    (secs.max(0.0) * SAMPLE_RATE as f64).round() as usize
}

/// A new `StemSet` holding only frames `[start, end)` of each stem (unity
/// gains — the caller applies the export gains).
fn slice(set: &StemSet, start: usize, end: usize) -> StemSet {
    let (a, b) = (start * CHANNELS, end * CHANNELS);
    let stems = set
        .stems
        .iter()
        .map(|s| SongBuffer {
            data: s.data[a..b].to_vec(),
        })
        .collect();
    StemSet::new(stems)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `secs` of a stereo sine at `hz`, amplitude `amp`.
    fn sine(secs: f64, hz: f32, amp: f32) -> SongBuffer {
        let frames = (secs * SAMPLE_RATE as f64) as usize;
        let mut data = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            let s = (i as f32 / SAMPLE_RATE as f32 * hz * std::f32::consts::TAU).sin() * amp;
            data.push(s);
            data.push(s);
        }
        SongBuffer { data }
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
    }

    fn cfg() -> RenderConfig {
        RenderConfig {
            start_secs: 0.0,
            end_secs: None,
            rate: 1.0,
            pitch_scale: 1.0,
            bass_focus: false,
            gains: vec![],
        }
    }

    #[test]
    fn unity_render_passes_audio_through() {
        let set = StemSet::single(sine(1.0, 220.0, 0.5));
        let out = render(&set, &cfg());
        let frames = (out.len() / CHANNELS) as i64;
        // ~1 s at 48 kHz, allowing for stretcher edge latency.
        assert!((frames - 48_000).abs() < 2_400, "frames = {frames}");
        assert!(
            peak(&out) > 0.2,
            "expected audible passthrough, peak = {}",
            peak(&out)
        );
    }

    #[test]
    fn renders_only_the_requested_span() {
        let set = StemSet::single(sine(4.0, 220.0, 0.5));
        let out = render(
            &set,
            &RenderConfig {
                start_secs: 1.0,
                end_secs: Some(2.0),
                ..cfg()
            },
        );
        let frames = (out.len() / CHANNELS) as i64;
        // a 1-second span at unity rate ≈ 48 kHz of output.
        assert!((frames - 48_000).abs() < 2_400, "frames = {frames}");
    }

    #[test]
    fn slowing_the_rate_lengthens_the_output() {
        let set = StemSet::single(sine(1.0, 220.0, 0.5));
        let out = render(&set, &RenderConfig { rate: 0.5, ..cfg() });
        let frames = (out.len() / CHANNELS) as i64;
        // half speed on 1 s of input ≈ 2 s of output.
        assert!((frames - 96_000).abs() < 2_400, "frames = {frames}");
    }

    #[test]
    fn muting_every_stem_renders_silence() {
        let set = StemSet::new(vec![sine(1.0, 220.0, 0.5), sine(1.0, 330.0, 0.5)]);
        let out = render(
            &set,
            &RenderConfig {
                gains: vec![0.0, 0.0],
                ..cfg()
            },
        );
        assert!(!out.is_empty());
        assert!(peak(&out) < 1e-3, "expected silence, peak = {}", peak(&out));
    }

    #[test]
    fn bass_focus_changes_the_output() {
        let set = StemSet::single(sine(1.0, 220.0, 0.5));
        let dry = render(&set, &cfg());
        let wet = render(
            &set,
            &RenderConfig {
                bass_focus: true,
                ..cfg()
            },
        );
        assert_eq!(dry.len(), wet.len());
        let diff: f32 = dry.iter().zip(&wet).map(|(a, b)| (a - b).abs()).sum();
        assert!(
            diff > 1.0,
            "bass focus left the signal unchanged, diff = {diff}"
        );
    }
}
