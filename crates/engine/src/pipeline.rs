use crate::buffer::{SongBuffer, CHANNELS, SAMPLE_RATE};
use crate::filter::BassFocus;
use crate::looper::Looper;
use crate::stretch::{Stretcher, BLOCK_FRAMES};
use std::sync::Arc;

pub const GAIN_RAMP_FRAMES: usize = 240; // 5 ms

/// Copy-only commands — safe to ship over an SPSC ring into the RT thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineCmd {
    Play,
    Pause,
    SeekSecs(f64),
    SetLoopSecs { start: f64, end: f64 },
    ClearLoop,
    SetRate(f64),
    /// semitones + cents, combined at the boundary into one scale factor
    SetPitchScale(f64),
    BassFocus(bool),
    /// RecallSilent: audio muted, position keeps advancing.
    Mute(bool),
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
    bass_focus: Option<BassFocus>,
    rate: f64,
    pitch_scale: f64,
    playing: bool,
    muted: bool,
    gain: f32,
    target_gain: f32,
    feed_buf: Vec<f32>,
}

impl Pipeline {
    pub fn new(buf: Arc<SongBuffer>) -> Self {
        Self {
            looper: Looper::new(buf),
            stretch: Stretcher::new(),
            bass_focus: None,
            rate: 1.0,
            pitch_scale: 1.0,
            playing: false,
            muted: false,
            gain: 0.0,
            target_gain: 0.0,
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
                self.bass_focus = if on { Some(BassFocus::new()) } else { None };
            }
            EngineCmd::Mute(on) => {
                self.muted = on;
                self.target_gain = if on || !self.playing { 0.0 } else { 1.0 };
            }
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

        let got = self.stretch.pull(out);
        out[got * CHANNELS..].fill(0.0);

        if let Some(bf) = &mut self.bass_focus {
            bf.process_interleaved(out);
        }

        // Linear gain ramp toward target, applied per frame.
        let step = 1.0 / GAIN_RAMP_FRAMES as f32;
        for fr in out.chunks_exact_mut(CHANNELS) {
            if self.gain < self.target_gain {
                self.gain = (self.gain + step).min(self.target_gain);
            } else if self.gain > self.target_gain {
                self.gain = (self.gain - step).max(self.target_gain);
            }
            fr[0] *= self.gain;
            fr[1] *= self.gain;
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

    fn sine_buf(secs: f64) -> Arc<SongBuffer> {
        let frames = (secs * SAMPLE_RATE as f64) as usize;
        let mut data = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            let s = (i as f32 / SAMPLE_RATE as f32 * 220.0 * std::f32::consts::TAU).sin() * 0.5;
            data.push(s);
            data.push(s);
        }
        Arc::new(SongBuffer { data })
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
        let mut p = Pipeline::new(Arc::new(SongBuffer { data }));
        p.apply(EngineCmd::Play);
        p.apply(EngineCmd::BassFocus(true));
        let (out, _) = render_secs(&mut p, 1.0);
        let tail = &out[out.len() / 2..];
        assert!(rms(tail) < 0.05, "rms = {}", rms(tail));
    }

    #[test]
    fn finished_event_after_song_end_without_loop() {
        let mut p = Pipeline::new(sine_buf(0.5));
        p.apply(EngineCmd::Play);
        let (_, events) = render_secs(&mut p, 1.5);
        assert!(events.contains(&EngineEvent::Finished));
    }
}
