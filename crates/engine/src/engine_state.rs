//! Live engine state snapshot for output-device handoff.
//!
//! `Engine` observes every `EngineCmd` it sends (and every position event it
//! receives) into an `EngineState`. When the output thread is torn down and
//! respawned on a new device, the fresh pipeline starts from defaults; replaying
//! `replay_cmds()` into the new command ring restores playback to where it was.
//!
//! This type is pure: no audio, no threads, no I/O. It is unit-tested in
//! isolation.

use crate::pipeline::EngineCmd;

#[derive(Default, Clone)]
pub struct EngineState {
    pub loop_region: Option<(f64, f64)>,
    pub rate: Option<f64>,
    pub pitch_scale: Option<f64>,
    pub bass_focus: bool,
    pub muted: bool,
    pub stem_gains: std::collections::BTreeMap<usize, f32>, // only observed indices
    pub volume: Option<f32>,
    pub playing: bool,
    pub pos_secs: f64,
}

impl EngineState {
    /// Fold a command into the snapshot. Mirrors the meaning of each
    /// `EngineCmd` so the snapshot tracks what a fresh pipeline would need.
    pub fn observe(&mut self, cmd: &EngineCmd) {
        match cmd {
            EngineCmd::Play => self.playing = true,
            EngineCmd::Pause => self.playing = false,
            EngineCmd::SeekSecs(s) => self.pos_secs = *s,
            EngineCmd::SetLoopSecs { start, end } => self.loop_region = Some((*start, *end)),
            EngineCmd::ClearLoop => self.loop_region = None,
            EngineCmd::SetRate(r) => self.rate = Some(*r),
            EngineCmd::SetPitchScale(p) => self.pitch_scale = Some(*p),
            EngineCmd::BassFocus(b) => self.bass_focus = *b,
            EngineCmd::Mute(m) => self.muted = *m,
            EngineCmd::SetStemGain { idx, gain } => {
                self.stem_gains.insert(*idx, *gain);
            }
            EngineCmd::SetVolume(v) => self.volume = Some(*v),
        }
    }

    /// Update playhead/play state from a position event coming back from the
    /// render thread (the authoritative live position the pump observes).
    pub fn set_position(&mut self, secs: f64, playing: bool) {
        self.pos_secs = secs;
        self.playing = playing;
    }

    /// Emit the commands that rebuild this state on a fresh pipeline, in the
    /// order the new ring should receive them. `BassFocus` and `Mute` are
    /// always emitted (their "off" is meaningful and cheap); volume/rate/pitch
    /// only when observed; the loop only when one is set; trailing `Play` only
    /// when playing.
    pub fn replay_cmds(&self) -> Vec<EngineCmd> {
        let mut cmds = Vec::new();
        if let Some(v) = self.volume {
            cmds.push(EngineCmd::SetVolume(v));
        }
        if let Some(r) = self.rate {
            cmds.push(EngineCmd::SetRate(r));
        }
        if let Some(p) = self.pitch_scale {
            cmds.push(EngineCmd::SetPitchScale(p));
        }
        cmds.push(EngineCmd::BassFocus(self.bass_focus));
        cmds.push(EngineCmd::Mute(self.muted));
        for (&idx, &gain) in &self.stem_gains {
            cmds.push(EngineCmd::SetStemGain { idx, gain });
        }
        if let Some((start, end)) = self.loop_region {
            cmds.push(EngineCmd::SetLoopSecs { start, end });
        }
        cmds.push(EngineCmd::SeekSecs(self.pos_secs));
        if self.playing {
            cmds.push(EngineCmd::Play);
        }
        cmds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_sequence_builds_expected_snapshot() {
        let mut s = EngineState::default();
        s.observe(&EngineCmd::SetVolume(0.8));
        s.observe(&EngineCmd::SetRate(0.5));
        s.observe(&EngineCmd::Play);
        s.observe(&EngineCmd::SetLoopSecs {
            start: 1.0,
            end: 2.0,
        });
        s.observe(&EngineCmd::SeekSecs(1.25));

        assert_eq!(s.volume, Some(0.8));
        assert_eq!(s.rate, Some(0.5));
        assert!(s.playing);
        assert_eq!(s.loop_region, Some((1.0, 2.0)));
        assert_eq!(s.pos_secs, 1.25);
        // untouched fields keep defaults
        assert_eq!(s.pitch_scale, None);
        assert!(!s.bass_focus);
        assert!(!s.muted);
        assert!(s.stem_gains.is_empty());
    }

    #[test]
    fn replay_emits_expected_ordered_cmds_with_trailing_play() {
        let mut s = EngineState::default();
        s.observe(&EngineCmd::SetVolume(0.8));
        s.observe(&EngineCmd::SetRate(0.5));
        s.observe(&EngineCmd::SetPitchScale(1.2));
        s.observe(&EngineCmd::BassFocus(true));
        s.observe(&EngineCmd::SetStemGain { idx: 1, gain: 0.0 });
        s.observe(&EngineCmd::SetLoopSecs {
            start: 1.0,
            end: 2.0,
        });
        s.observe(&EngineCmd::SeekSecs(1.25));
        s.observe(&EngineCmd::Play);

        assert_eq!(
            s.replay_cmds(),
            vec![
                EngineCmd::SetVolume(0.8),
                EngineCmd::SetRate(0.5),
                EngineCmd::SetPitchScale(1.2),
                EngineCmd::BassFocus(true),
                EngineCmd::Mute(false),
                EngineCmd::SetStemGain { idx: 1, gain: 0.0 },
                EngineCmd::SetLoopSecs {
                    start: 1.0,
                    end: 2.0,
                },
                EngineCmd::SeekSecs(1.25),
                EngineCmd::Play,
            ]
        );
    }

    #[test]
    fn paused_snapshot_omits_trailing_play() {
        let mut s = EngineState::default();
        s.observe(&EngineCmd::Play);
        s.observe(&EngineCmd::SeekSecs(3.0));
        s.observe(&EngineCmd::Pause);

        let cmds = s.replay_cmds();
        assert!(!cmds.contains(&EngineCmd::Play));
        // still always emits the cheap toggles + a seek
        assert_eq!(
            cmds,
            vec![
                EngineCmd::BassFocus(false),
                EngineCmd::Mute(false),
                EngineCmd::SeekSecs(3.0),
            ]
        );
    }

    #[test]
    fn clear_loop_leaves_no_loop_in_replay() {
        let mut s = EngineState::default();
        s.observe(&EngineCmd::SetLoopSecs {
            start: 1.0,
            end: 2.0,
        });
        s.observe(&EngineCmd::ClearLoop);

        assert_eq!(s.loop_region, None);
        let cmds = s.replay_cmds();
        assert!(
            !cmds
                .iter()
                .any(|c| matches!(c, EngineCmd::SetLoopSecs { .. })),
            "no loop cmd should be replayed after ClearLoop: {cmds:?}"
        );
    }
}
