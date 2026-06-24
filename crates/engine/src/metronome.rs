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
#[allow(dead_code)] // consumed in Task M2
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
#[allow(dead_code)] // consumed in Task M2
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
#[allow(dead_code)] // consumed in Task M2
fn noise(rng: &mut u32) -> f32 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *rng = x;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[allow(dead_code)] // consumed in Task M2
const SR: f64 = SAMPLE_RATE as f64;

/// Sample for a synthesized drum sound at envelope age `age` (frames since
/// trigger), or 0.0 once it has decayed.
#[allow(dead_code)] // consumed in Task M2
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

#[allow(dead_code)] // consumed in Task M2
fn cowbell(t: f64, f1: f64, f2: f64) -> f64 {
    let env = (-t * 12.0).exp();
    let tau = 2.0 * std::f64::consts::PI;
    0.5 * ((tau * f1 * t).sin() + (tau * f2 * t).sin()) * env * 0.45
}

/// A one-shot retriggerable voice (the metronome holds one; beats never overlap
/// audibly at ≤300 BPM).
#[allow(dead_code)] // consumed in Task M2
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

#[allow(dead_code)] // consumed in Task M2
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

#[cfg(test)]
mod tests {
    use super::*;

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
