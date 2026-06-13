use crate::buffer::{StemSet, CHANNELS, SAMPLE_RATE};
use crate::filter::Focus;
pub use crate::filter::FocusKind;
use crate::looper::Looper;
use crate::stretch::{Stretcher, BLOCK_FRAMES};

pub const GAIN_RAMP_FRAMES: usize = 240; // 5 ms

/// Copy-only commands — safe to ship over an SPSC ring into the RT thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineCmd {
    Play,
    Pause,
    SeekSecs(f64),
    SetLoopSecs {
        start: f64,
        end: f64,
    },
    ClearLoop,
    SetRate(f64),
    /// semitones + cents, combined at the boundary into one scale factor
    SetPitchScale(f64),
    BassFocus(bool),
    SetFocus(Option<FocusKind>),
    /// RecallSilent: audio muted, position keeps advancing.
    Mute(bool),
    /// Per-stem mix gain (0.0..=1.5); out-of-range stems ignored.
    SetStemGain {
        idx: usize,
        gain: f32,
    },
    /// User playback volume (clamped 0.0..=1.5) — a multiplier separate
    /// from the play/pause/mute gain ramp, with its own ramp to target.
    SetVolume(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineEvent {
    Position { secs: f64, rate: f64, playing: bool },
    LoopWrapped,
    Finished,
}

pub struct Pipeline {
    looper: Looper,
    stretch: Stretcher,
    focus: Option<Focus>,
    rate: f64,
    pitch_scale: f64,
    playing: bool,
    muted: bool,
    gain: f32,
    target_gain: f32,
    volume: f32,
    target_volume: f32,
    feed_buf: Vec<f32>,
}

impl Pipeline {
    pub fn new(set: StemSet) -> Self {
        Self {
            looper: Looper::new(set),
            stretch: Stretcher::new(),
            focus: None,
            rate: 1.0,
            pitch_scale: 1.0,
            playing: false,
            muted: false,
            gain: 0.0,
            target_gain: 0.0,
            volume: 1.0,
            target_volume: 1.0,
            feed_buf: vec![0.0; BLOCK_FRAMES * CHANNELS],
        }
    }

    pub fn apply(&mut self, cmd: EngineCmd) {
        match cmd {
            EngineCmd::Play => {
                self.playing = true;
                if !self.muted {
                    self.target_gain = 1.0;
                }
            }
            EngineCmd::Pause => {
                self.target_gain = 0.0;
            }
            EngineCmd::SeekSecs(secs) => {
                self.looper.seek(secs_to_frames(secs));
                self.stretch.reset();
            }
            EngineCmd::SetLoopSecs { start, end } => {
                self.looper
                    .set_region(secs_to_frames(start), secs_to_frames(end));
                self.stretch.reset();
            }
            EngineCmd::ClearLoop => self.looper.clear_region(),
            EngineCmd::SetRate(rate) => {
                self.rate = rate.clamp(0.25, 2.0);
                self.stretch.set_rate(self.rate);
            }
            EngineCmd::SetPitchScale(scale) => {
                self.pitch_scale = scale;
                self.stretch.set_pitch_scale(self.pitch_scale);
            }
            EngineCmd::BassFocus(on) => {
                self.focus = if on { Some(Focus::new(FocusKind::Bass)) } else { None };
            }
            EngineCmd::SetFocus(kind) => {
                self.focus = kind.map(Focus::new);
            }
            EngineCmd::Mute(on) => {
                self.muted = on;
                self.target_gain = if on || !self.playing { 0.0 } else { 1.0 };
            }
            EngineCmd::SetStemGain { idx, gain } => self.looper.set_gain(idx, gain),
            EngineCmd::SetVolume(v) => self.target_volume = v.clamp(0.0, 1.5),
        }
    }

    /// Render interleaved stereo into `out`; push events into `events`.
    pub fn render(&mut self, out: &mut [f32], events: &mut Vec<EngineEvent>) {
        let frames_req = out.len() / CHANNELS;

        if !self.playing && self.gain == 0.0 {
            out.fill(0.0);
            self.push_position(events);
            return;
        }

        // Keep the stretcher fed until it can satisfy this block.
        while self.playing && self.stretch.available() < frames_req {
            let want = self.stretch.frames_wanted().max(1);
            let info = self.looper.read(&mut self.feed_buf[..want * CHANNELS]);
            if info.wrapped {
                events.push(EngineEvent::LoopWrapped);
            }
            if info.frames > 0 {
                self.stretch.feed(&self.feed_buf[..info.frames * CHANNELS]);
            }
            if info.finished {
                events.push(EngineEvent::Finished);
                self.playing = false;
                self.target_gain = 0.0;
                break;
            }
        }

        // Pull in BLOCK_FRAMES-sized chunks until the block is filled
        // (out may be larger than one stretcher block).
        let mut filled = 0;
        while filled < frames_req {
            let got = self.stretch.pull(&mut out[filled * CHANNELS..]);
            if got == 0 {
                break;
            }
            filled += got;
        }
        out[filled * CHANNELS..].fill(0.0);

        if let Some(f) = &mut self.focus {
            f.process_interleaved(out);
        }

        // Linear ramps toward targets, applied per frame: the play/pause/mute
        // gain and the user volume each move at full-scale-per-5-ms, then the
        // frame is scaled by their product (no zipper noise on either knob).
        let step = 1.0 / GAIN_RAMP_FRAMES as f32;
        for fr in out.chunks_exact_mut(CHANNELS) {
            if self.gain < self.target_gain {
                self.gain = (self.gain + step).min(self.target_gain);
            } else if self.gain > self.target_gain {
                self.gain = (self.gain - step).max(self.target_gain);
            }
            if self.volume < self.target_volume {
                self.volume = (self.volume + step).min(self.target_volume);
            } else if self.volume > self.target_volume {
                self.volume = (self.volume - step).max(self.target_volume);
            }
            let scale = self.gain * self.volume;
            fr[0] *= scale;
            fr[1] *= scale;
        }

        // A completed ramp-down means pause — unless we're muted (position
        // keeps advancing while muted: RecallSilent).
        if self.target_gain == 0.0 && self.gain == 0.0 && !self.muted {
            self.playing = false;
        }

        self.push_position(events);
    }

    fn push_position(&self, events: &mut Vec<EngineEvent>) {
        events.push(EngineEvent::Position {
            secs: self.looper.pos_frames() as f64 / SAMPLE_RATE as f64,
            rate: self.rate,
            playing: self.playing,
        });
    }
}

fn secs_to_frames(secs: f64) -> usize {
    (secs.max(0.0) * SAMPLE_RATE as f64).round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::buffer::SongBuffer;

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

    fn sine_buf(secs: f64) -> StemSet {
        StemSet::single(sine(secs, 220.0, 0.5))
    }

    fn render_secs(p: &mut Pipeline, secs: f64) -> (Vec<f32>, Vec<EngineEvent>) {
        let total = (secs * SAMPLE_RATE as f64) as usize;
        let mut out = Vec::new();
        let mut events = Vec::new();
        let mut block = vec![0.0f32; 256 * CHANNELS];
        let mut rendered = 0;
        while rendered < total {
            p.render(&mut block, &mut events);
            out.extend_from_slice(&block);
            rendered += 256;
        }
        (out, events)
    }

    fn rms(v: &[f32]) -> f64 {
        (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    }

    #[test]
    fn paused_pipeline_renders_silence() {
        let mut p = Pipeline::new(sine_buf(2.0));
        let (out, _) = render_secs(&mut p, 0.5);
        assert_eq!(rms(&out), 0.0);
    }

    #[test]
    fn playing_renders_audio_and_position_advances() {
        let mut p = Pipeline::new(sine_buf(4.0));
        p.apply(EngineCmd::Play);
        let (out, events) = render_secs(&mut p, 1.0);
        assert!(rms(&out) > 0.2);
        let last_pos = events.iter().rev().find_map(|e| match e {
            EngineEvent::Position { secs, .. } => Some(*secs),
            _ => None,
        });
        let secs = last_pos.unwrap();
        assert!((0.5..=1.5).contains(&secs), "pos = {secs}");
    }

    #[test]
    fn loop_wrap_events_fire_with_output_period_scaled_by_rate() {
        let mut p = Pipeline::new(sine_buf(10.0));
        p.apply(EngineCmd::SetLoopSecs {
            start: 1.0,
            end: 2.0,
        }); // 1 s loop
        p.apply(EngineCmd::SetRate(0.5));
        p.apply(EngineCmd::Play);
        let (_, events) = render_secs(&mut p, 7.0);
        let wraps = events
            .iter()
            .filter(|e| **e == EngineEvent::LoopWrapped)
            .count();
        // 7 s of output at half speed covers ~3.5 loop periods (minus RB latency)
        assert!((2..=4).contains(&wraps), "wraps = {wraps}");
    }

    #[test]
    fn mute_keeps_position_moving_but_output_silent() {
        let mut p = Pipeline::new(sine_buf(10.0));
        p.apply(EngineCmd::SetLoopSecs {
            start: 0.0,
            end: 1.0,
        });
        p.apply(EngineCmd::Play);
        p.apply(EngineCmd::Mute(true));
        let (out, events) = render_secs(&mut p, 2.5);
        // gain ramp at the start; steady state is silent
        let tail = &out[out.len() / 2..];
        assert!(rms(tail) < 1e-4, "tail rms = {}", rms(tail));
        let wraps = events
            .iter()
            .filter(|e| **e == EngineEvent::LoopWrapped)
            .count();
        assert!(wraps >= 1, "loop must keep wrapping while muted");
    }

    #[test]
    fn bass_focus_attenuates_a_high_sine() {
        // 2 kHz sine through bass focus should drop hard
        let frames = SAMPLE_RATE as usize * 4;
        let mut data = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            let s = (i as f32 / SAMPLE_RATE as f32 * 2_000.0 * std::f32::consts::TAU).sin() * 0.5;
            data.push(s);
            data.push(s);
        }
        let mut p = Pipeline::new(StemSet::single(SongBuffer { data }));
        p.apply(EngineCmd::Play);
        p.apply(EngineCmd::BassFocus(true));
        let (out, _) = render_secs(&mut p, 1.0);
        let tail = &out[out.len() / 2..];
        assert!(rms(tail) < 0.05, "rms = {}", rms(tail));
    }

    #[test]
    fn stem_gain_zero_drops_that_stem_from_the_mix() {
        // two equal-amplitude uncorrelated sines: muting one leaves the
        // other's RMS (a/√2) in the output
        let set = StemSet::new(vec![sine(4.0, 220.0, 0.4), sine(4.0, 333.0, 0.4)]);
        let mut p = Pipeline::new(set);
        p.apply(EngineCmd::Play);
        let (out, _) = render_secs(&mut p, 1.0);
        let before = rms(&out[out.len() / 2..]);
        p.apply(EngineCmd::SetStemGain { idx: 0, gain: 0.0 });
        let (out, _) = render_secs(&mut p, 1.0);
        let after = rms(&out[out.len() / 2..]);
        let one_stem = 0.4 / 2f64.sqrt();
        assert!(
            (after - one_stem).abs() / one_stem < 0.15,
            "after = {after}, expected ≈ {one_stem}"
        );
        assert!(after < before * 0.85, "before = {before}, after = {after}");
    }

    #[test]
    fn volume_half_halves_output_rms() {
        let run = |volume: Option<f32>| {
            let mut p = Pipeline::new(sine_buf(4.0));
            if let Some(v) = volume {
                p.apply(EngineCmd::SetVolume(v));
            }
            p.apply(EngineCmd::Play);
            let (out, _) = render_secs(&mut p, 1.0);
            rms(&out[out.len() / 2..]) // steady state, past both ramps
        };
        let full = run(None);
        let half = run(Some(0.5));
        assert!(full > 0.2, "full = {full}");
        assert!(
            (half - full / 2.0).abs() / (full / 2.0) < 0.05,
            "half = {half}, full = {full}"
        );
    }

    #[test]
    fn volume_clamps_to_one_point_five() {
        let run = |v: f32| {
            let mut p = Pipeline::new(sine_buf(4.0));
            p.apply(EngineCmd::SetVolume(v));
            p.apply(EngineCmd::Play);
            let (out, _) = render_secs(&mut p, 1.0);
            rms(&out[out.len() / 2..])
        };
        let max = run(1.5);
        let over = run(9.0);
        assert!(
            (over - max).abs() / max < 0.01,
            "over = {over}, max = {max}"
        );
    }

    #[test]
    fn mute_silences_regardless_of_volume() {
        let mut p = Pipeline::new(sine_buf(10.0));
        p.apply(EngineCmd::SetVolume(1.5));
        p.apply(EngineCmd::SetLoopSecs {
            start: 0.0,
            end: 1.0,
        });
        p.apply(EngineCmd::Play);
        p.apply(EngineCmd::Mute(true));
        let (out, _) = render_secs(&mut p, 2.5);
        let tail = &out[out.len() / 2..];
        assert!(rms(tail) < 1e-4, "tail rms = {}", rms(tail));
    }

    #[test]
    fn volume_does_not_change_loop_wrap_cadence() {
        let mut p = Pipeline::new(sine_buf(10.0));
        p.apply(EngineCmd::SetVolume(0.5));
        p.apply(EngineCmd::SetLoopSecs {
            start: 1.0,
            end: 2.0,
        }); // 1 s loop
        p.apply(EngineCmd::Play);
        let (_, events) = render_secs(&mut p, 3.5);
        let wraps = events
            .iter()
            .filter(|e| **e == EngineEvent::LoopWrapped)
            .count();
        // 3.5 s of output at 1× covers ~3.5 loop periods (minus RB latency)
        assert!((2..=4).contains(&wraps), "wraps = {wraps}");
    }

    #[test]
    fn finished_event_after_song_end_without_loop() {
        let mut p = Pipeline::new(sine_buf(0.5));
        p.apply(EngineCmd::Play);
        let (_, events) = render_secs(&mut p, 1.5);
        assert!(events.contains(&EngineEvent::Finished));
    }
}
