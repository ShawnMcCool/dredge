//! Synthesized metronome sound kits.

use crate::buffer::SAMPLE_RATE;
use crate::pipeline::click_wave;

/// Which beats of the bar actually click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    EveryBeat,
    HalfBar,
    EveryBar,
}

/// A sound kit: a downbeat sound paired with an other-beats sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kit {
    Click,
    KickSnare,
    Cowbell,
}

/// One concrete voice the metronome can sound on a beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sound {
    ClickHi,
    ClickLo,
    Kick,
    Snare,
    CowbellHi,
    CowbellLo,
}

/// The voice a kit uses for a beat: accent (downbeat) vs the rest.
fn sound_for(kit: Kit, accent: bool) -> Sound {
    match (kit, accent) {
        (Kit::Click, true) => Sound::ClickHi,
        (Kit::Click, false) => Sound::ClickLo,
        (Kit::KickSnare, true) => Sound::Kick,
        (Kit::KickSnare, false) => Sound::Snare,
        (Kit::Cowbell, true) => Sound::CowbellHi,
        (Kit::Cowbell, false) => Sound::CowbellLo,
    }
}

/// Bipolar white noise in [-1, 1) from a tiny xorshift RNG (no allocation).
fn noise(rng: &mut u32) -> f32 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *rng = x;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

const SR: f64 = SAMPLE_RATE as f64;

/// Sample for a synthesized drum sound at envelope age `age` (frames since
/// trigger), or 0.0 once it has decayed.
fn synth(sound: Sound, age: usize, rng: &mut u32, volume: f32) -> f32 {
    let t = age as f64 / SR;
    let (len_s, raw) = match sound {
        Sound::ClickHi => return click_wave(age, true, volume),
        Sound::ClickLo => return click_wave(age, false, volume),
        Sound::Kick => {
            let f = 55.0 + 90.0 * (-t * 50.0).exp();
            let env = (-t * 22.0).exp();
            (0.15, (2.0 * std::f64::consts::PI * f * t).sin() * env * 0.9)
        }
        Sound::Snare => {
            let tone = (2.0 * std::f64::consts::PI * 180.0 * t).sin();
            let env = (-t * 30.0).exp();
            let n = noise(rng) as f64;
            (0.12, (0.7 * n + 0.3 * tone) * env * 0.6)
        }
        Sound::CowbellHi => (0.30, cowbell(t, 540.0, 800.0)),
        Sound::CowbellLo => (0.30, cowbell(t, 430.0, 640.0)),
    };
    if t >= len_s {
        return 0.0;
    }
    raw as f32 * volume
}

fn cowbell(t: f64, f1: f64, f2: f64) -> f64 {
    let env = (-t * 12.0).exp();
    let tau = 2.0 * std::f64::consts::PI;
    0.5 * ((tau * f1 * t).sin() + (tau * f2 * t).sin()) * env * 0.45
}

/// A one-shot retriggerable voice (the metronome holds one; beats never overlap
/// audibly at ≤300 BPM).
struct Voice {
    age: usize,
    sound: Sound,
    silent: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self { age: 0, sound: Sound::ClickLo, silent: true }
    }
}

impl Voice {
    fn trigger(&mut self, sound: Sound) {
        self.sound = sound;
        self.age = 0;
        self.silent = false;
    }
    /// Current sample then advance one frame. 0.0 while silent/decayed.
    fn sample(&mut self, volume: f32, rng: &mut u32) -> f32 {
        if self.silent {
            return 0.0;
        }
        let s = synth(self.sound, self.age, rng, volume);
        self.age = self.age.saturating_add(1);
        if s == 0.0 && self.age > 1 {
            self.silent = true;
        }
        s
    }
}

use crate::buffer::CHANNELS;

/// Emitted once per beat while running, for the UI bar indicator. 1-based beat
/// within the bar; `of` is beats-per-bar; `sounded` reflects the cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetronomeBeat {
    pub beat: u32,
    pub of: u32,
    pub sounded: bool,
}

/// Free-running click generator, owned by the render core so it runs with or
/// without a song. Mixes its voice over the output buffer (add) and pushes a
/// `MetronomeBeat` per beat.
pub struct Metronome {
    running: bool,
    interval: usize,
    beats_per_bar: u32,
    cadence: Cadence,
    kit: Kit,
    to_next: usize,
    beat_idx: u32,
    voice: Voice,
    rng: u32,
}

impl Default for Metronome {
    fn default() -> Self {
        Self {
            running: false,
            interval: (0.5 * SR) as usize,
            beats_per_bar: 4,
            cadence: Cadence::EveryBeat,
            kit: Kit::Click,
            to_next: 0,
            beat_idx: 0,
            voice: Voice::default(),
            rng: 0x2545_f491,
        }
    }
}

/// Does beat `beat` (0-based) of an `n`-beat bar sound under `cadence`?
fn cadence_sounds(cadence: Cadence, beat: u32, n: u32) -> bool {
    match cadence {
        Cadence::EveryBeat => true,
        Cadence::EveryBar => beat == 0,
        Cadence::HalfBar => beat == 0 || beat == n / 2,
    }
}

impl Metronome {
    /// Apply config. Re-derives the interval from `beat_secs`; a stopped→running
    /// edge resets the bar to beat 1 immediately. A config change while already
    /// running keeps the running phase (no bar restart).
    pub fn configure(
        &mut self,
        running: bool,
        beat_secs: f64,
        beats_per_bar: u32,
        cadence: Cadence,
        kit: Kit,
    ) {
        let starting = running && !self.running;
        self.running = running;
        self.interval = ((beat_secs * SR).round() as usize).max(1);
        self.beats_per_bar = beats_per_bar.max(1);
        self.cadence = cadence;
        self.kit = kit;
        if starting {
            self.beat_idx = 0;
            self.to_next = 0;
        } else if self.running {
            self.to_next = self.to_next.min(self.interval);
        }
    }

    /// Mix the metronome over `out` (interleaved stereo, additive) and push beat
    /// events. `volume` is the user playback volume.
    pub fn render(&mut self, out: &mut [f32], volume: f32, events: &mut Vec<MetronomeBeat>) {
        let frames = out.len() / CHANNELS;
        for i in 0..frames {
            if self.running {
                if self.to_next == 0 {
                    let beat = self.beat_idx % self.beats_per_bar;
                    let sounded = cadence_sounds(self.cadence, beat, self.beats_per_bar);
                    if sounded {
                        self.voice.trigger(sound_for(self.kit, beat == 0));
                    }
                    events.push(MetronomeBeat {
                        beat: beat + 1,
                        of: self.beats_per_bar,
                        sounded,
                    });
                    self.beat_idx = self.beat_idx.wrapping_add(1);
                    self.to_next = self.interval;
                }
                self.to_next -= 1;
            }
            let s = self.voice.sample(volume, &mut self.rng);
            out[i * CHANNELS] += s;
            out[i * CHANNELS + 1] += s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::CHANNELS;

    fn render_secs(m: &mut Metronome, secs: f64) -> (Vec<f32>, Vec<MetronomeBeat>) {
        let frames = (secs * SR) as usize;
        let mut out = vec![0.0f32; frames * CHANNELS];
        let mut beats = Vec::new();
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(256);
            m.render(&mut out[done * CHANNELS..(done + n) * CHANNELS], 1.0, &mut beats);
            done += n;
        }
        (out, beats)
    }

    #[test]
    fn fires_beats_at_the_bpm_interval() {
        let mut m = Metronome::default();
        m.configure(true, 0.5, 4, Cadence::EveryBeat, Kit::Click);
        let (_out, beats) = render_secs(&mut m, 2.1);
        let sounded: Vec<_> = beats.iter().filter(|b| b.sounded).collect();
        assert!(sounded.len() >= 4 && sounded.len() <= 5, "got {} beats", sounded.len());
        assert_eq!(beats[0].beat, 1);
        assert_eq!(beats[0].of, 4);
    }

    #[test]
    fn downbeat_recurs_every_bar() {
        let mut m = Metronome::default();
        m.configure(true, 0.25, 4, Cadence::EveryBeat, Kit::Click);
        let (_o, beats) = render_secs(&mut m, 2.1);
        let labels: Vec<u32> = beats.iter().map(|b| b.beat).take(8).collect();
        assert_eq!(labels, vec![1, 2, 3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn every_bar_cadence_sounds_only_the_downbeat() {
        let mut m = Metronome::default();
        m.configure(true, 0.25, 4, Cadence::EveryBar, Kit::Click);
        let (_o, beats) = render_secs(&mut m, 2.1);
        for b in &beats {
            assert_eq!(b.sounded, b.beat == 1, "beat {} sounded={}", b.beat, b.sounded);
        }
    }

    #[test]
    fn half_bar_cadence_sounds_one_and_mid() {
        let mut m = Metronome::default();
        m.configure(true, 0.25, 4, Cadence::HalfBar, Kit::Click);
        let (_o, beats) = render_secs(&mut m, 2.1);
        for b in &beats {
            let want = b.beat == 1 || b.beat == 3;
            assert_eq!(b.sounded, want, "beat {} sounded={}", b.beat, b.sounded);
        }
    }

    #[test]
    fn half_bar_cadence_in_odd_meter_sounds_one_and_mid() {
        // 5/4: HalfBar sounds beat 1 and the integer mid-bar beat (1 + 5/2 = 3).
        let mut m = Metronome::default();
        m.configure(true, 0.2, 5, Cadence::HalfBar, Kit::Click);
        let (_o, beats) = render_secs(&mut m, 2.1);
        assert!(beats.iter().any(|b| b.beat == 3), "saw a beat 3 to check");
        for b in &beats {
            let want = b.beat == 1 || b.beat == 3;
            assert_eq!(b.sounded, want, "beat {} sounded={}", b.beat, b.sounded);
        }
    }

    #[test]
    fn stopped_metronome_is_silent_and_emits_no_beats() {
        let mut m = Metronome::default();
        m.configure(false, 0.5, 4, Cadence::EveryBeat, Kit::Click);
        let (out, beats) = render_secs(&mut m, 1.0);
        assert!(beats.is_empty());
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn audible_output_when_running() {
        let mut m = Metronome::default();
        m.configure(true, 0.5, 4, Cadence::EveryBeat, Kit::KickSnare);
        let (out, _b) = render_secs(&mut m, 1.0);
        assert!(out.iter().any(|s| s.abs() > 0.01), "metronome produced audio");
    }

    #[test]
    fn voice_is_silent_until_triggered_then_decays() {
        let mut v = Voice::default();
        let mut rng = 0x1234_5678u32;
        assert_eq!(v.sample(1.0, &mut rng), 0.0);
        v.trigger(Sound::Kick);
        let mut peak = 0.0f32;
        for _ in 0..200 {
            peak = peak.max(v.sample(1.0, &mut rng).abs());
        }
        assert!(peak > 0.0, "kick is audible after trigger");
        for _ in 0..7000 {
            v.sample(1.0, &mut rng);
        }
        assert_eq!(v.sample(1.0, &mut rng), 0.0, "decayed to silence");
    }

    #[test]
    fn every_kit_produces_sound_for_both_roles() {
        let mut rng = 0x9e37_79b9u32;
        for kit in [Kit::Click, Kit::KickSnare, Kit::Cowbell] {
            for accent in [true, false] {
                let mut v = Voice::default();
                v.trigger(sound_for(kit, accent));
                let mut peak = 0.0f32;
                for _ in 0..400 {
                    peak = peak.max(v.sample(1.0, &mut rng).abs());
                }
                assert!(peak > 0.0, "{kit:?} accent={accent} is audible");
            }
        }
    }

    #[test]
    fn click_kit_accent_is_louder_than_normal() {
        let mut rng = 1u32;
        let mut hi = Voice::default();
        hi.trigger(sound_for(Kit::Click, true));
        let mut lo = Voice::default();
        lo.trigger(sound_for(Kit::Click, false));
        let mut hp = 0.0f32;
        let mut lp = 0.0f32;
        for _ in 0..400 {
            hp = hp.max(hi.sample(1.0, &mut rng).abs());
            lp = lp.max(lo.sample(1.0, &mut rng).abs());
        }
        assert!(hp > lp, "click accent louder: {hp} > {lp}");
    }
}
