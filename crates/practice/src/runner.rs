use crate::model::{LoopId, Plan, PlanStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepMode {
    /// Playback only — instrument down, ears on.
    Listen,
    /// Play along with the recording.
    Play,
    /// Recording muted — play the passage from memory.
    RecallSilent,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct RepSpec {
    pub loop_id: LoopId,
    pub rate: f64,
    pub mode: RepMode,
    pub step_idx: usize,
    pub rep_idx: u32,
}

pub struct PlanRunner {
    plan: Plan,
    step_idx: usize,
    rep_idx: u32,
}

impl PlanRunner {
    pub fn new(plan: Plan) -> Self {
        Self { plan, step_idx: 0, rep_idx: 0 }
    }

    fn step_total_reps(step: &PlanStep) -> u32 {
        match step {
            PlanStep::ListenFirst { reps, .. } => *reps,
            PlanStep::PlayReps { reps, .. } => *reps,
            PlanStep::Rotation { loop_ids, rounds, reps_per_visit, .. } => {
                loop_ids.len() as u32 * rounds * reps_per_visit
            }
            PlanStep::RecallTest { alternations, .. } => alternations * 2,
        }
    }

    pub fn current(&self) -> Option<RepSpec> {
        let mut step_idx = self.step_idx;
        let mut rep_idx = self.rep_idx;
        // skip exhausted/empty steps
        loop {
            let step = self.plan.steps.get(step_idx)?;
            if rep_idx < Self::step_total_reps(step) {
                return Some(Self::spec(step, step_idx, rep_idx));
            }
            step_idx += 1;
            rep_idx = 0;
        }
    }

    fn spec(step: &PlanStep, step_idx: usize, rep_idx: u32) -> RepSpec {
        match step {
            PlanStep::ListenFirst { loop_id, .. } => RepSpec {
                loop_id: *loop_id,
                rate: 1.0,
                mode: RepMode::Listen,
                step_idx,
                rep_idx,
            },
            PlanStep::PlayReps { loop_id, curve, .. } => RepSpec {
                loop_id: *loop_id,
                rate: curve.rate_for_rep(rep_idx),
                mode: RepMode::Play,
                step_idx,
                rep_idx,
            },
            PlanStep::Rotation { loop_ids, reps_per_visit, curve, .. } => {
                let visit = rep_idx / (*reps_per_visit).max(1);
                let slot = (visit as usize) % loop_ids.len();
                let visits_to_this_loop = visit / loop_ids.len() as u32;
                RepSpec {
                    loop_id: loop_ids[slot],
                    rate: curve.rate_for_rep(visits_to_this_loop),
                    mode: RepMode::Play,
                    step_idx,
                    rep_idx,
                }
            }
            PlanStep::RecallTest { loop_id, rate, .. } => RepSpec {
                loop_id: *loop_id,
                rate: *rate,
                mode: if rep_idx % 2 == 0 { RepMode::Play } else { RepMode::RecallSilent },
                step_idx,
                rep_idx,
            },
        }
    }

    pub fn advance(&mut self) {
        // normalize to the position current() resolved to, then step once
        if let Some(spec) = self.current() {
            self.step_idx = spec.step_idx;
            self.rep_idx = spec.rep_idx + 1;
        }
    }

    /// Skip the rest of the current step.
    pub fn skip_step(&mut self) {
        if let Some(spec) = self.current() {
            self.step_idx = spec.step_idx + 1;
            self.rep_idx = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlanId, SongId, TempoCurve};

    fn plan(steps: Vec<PlanStep>) -> Plan {
        Plan { id: PlanId(1), song_id: SongId(1), name: "p".into(), steps }
    }

    fn drain(mut r: PlanRunner) -> Vec<RepSpec> {
        let mut out = Vec::new();
        while let Some(spec) = r.current() {
            out.push(spec);
            r.advance();
        }
        out
    }

    #[test]
    fn listen_first_yields_listen_reps() {
        let r = PlanRunner::new(plan(vec![PlanStep::ListenFirst {
            loop_id: LoopId(7),
            reps: 2,
        }]));
        let reps = drain(r);
        assert_eq!(reps.len(), 2);
        assert!(reps.iter().all(|s| s.mode == RepMode::Listen && s.loop_id == LoopId(7)));
        assert!(reps.iter().all(|s| s.rate == 1.0));
        assert_eq!(reps[1].rep_idx, 1);
    }

    #[test]
    fn play_reps_apply_curve_per_rep() {
        let r = PlanRunner::new(plan(vec![PlanStep::PlayReps {
            loop_id: LoopId(1),
            reps: 3,
            curve: TempoCurve::Ladder { start: 0.6, step: 0.1, target: 1.0 },
        }]));
        let rates: Vec<f64> = drain(r).iter().map(|s| s.rate).collect();
        assert_eq!(rates, vec![0.6, 0.7, 0.8]);
    }

    #[test]
    fn steps_run_in_sequence() {
        let r = PlanRunner::new(plan(vec![
            PlanStep::ListenFirst { loop_id: LoopId(1), reps: 1 },
            PlanStep::PlayReps {
                loop_id: LoopId(1),
                reps: 1,
                curve: TempoCurve::Dwell { rate: 0.9 },
            },
        ]));
        let reps = drain(r);
        assert_eq!(reps.len(), 2);
        assert_eq!(reps[0].mode, RepMode::Listen);
        assert_eq!(reps[1].mode, RepMode::Play);
        assert_eq!(reps[1].step_idx, 1);
    }

    #[test]
    fn rotation_interleaves_loops_round_robin() {
        let r = PlanRunner::new(plan(vec![PlanStep::Rotation {
            loop_ids: vec![LoopId(1), LoopId(2), LoopId(3)],
            rounds: 2,
            reps_per_visit: 1,
            curve: TempoCurve::Dwell { rate: 0.8 },
        }]));
        let ids: Vec<i64> = drain(r).iter().map(|s| s.loop_id.0).collect();
        // interleaved: 1,2,3,1,2,3 — NOT 1,1,2,2,3,3
        assert_eq!(ids, vec![1, 2, 3, 1, 2, 3]);
    }

    #[test]
    fn rotation_curve_advances_per_visit_to_same_loop() {
        let r = PlanRunner::new(plan(vec![PlanStep::Rotation {
            loop_ids: vec![LoopId(1), LoopId(2)],
            rounds: 2,
            reps_per_visit: 2,
            curve: TempoCurve::Ladder { start: 0.6, step: 0.1, target: 1.0 },
        }]));
        let reps = drain(r);
        // loop 1 visits: round0 (rate for visit 0), round1 (rate for visit 1)
        let loop1_rates: Vec<f64> = reps
            .iter()
            .filter(|s| s.loop_id == LoopId(1))
            .map(|s| s.rate)
            .collect();
        assert_eq!(loop1_rates, vec![0.6, 0.6, 0.7, 0.7]);
    }

    #[test]
    fn recall_test_alternates_play_and_silent() {
        let r = PlanRunner::new(plan(vec![PlanStep::RecallTest {
            loop_id: LoopId(5),
            alternations: 2,
            rate: 1.0,
        }]));
        let modes: Vec<RepMode> = drain(r).iter().map(|s| s.mode).collect();
        assert_eq!(
            modes,
            vec![RepMode::Play, RepMode::RecallSilent, RepMode::Play, RepMode::RecallSilent]
        );
    }

    #[test]
    fn skip_step_jumps_to_next_step() {
        let mut r = PlanRunner::new(plan(vec![
            PlanStep::ListenFirst { loop_id: LoopId(1), reps: 10 },
            PlanStep::ListenFirst { loop_id: LoopId(2), reps: 1 },
        ]));
        r.advance();
        r.skip_step();
        assert_eq!(r.current().unwrap().loop_id, LoopId(2));
        r.advance();
        assert!(r.current().is_none());
    }

    #[test]
    fn empty_plan_is_immediately_finished() {
        let r = PlanRunner::new(plan(vec![]));
        assert!(r.current().is_none());
    }

    #[test]
    fn empty_rotation_step_is_skipped() {
        let r = PlanRunner::new(plan(vec![PlanStep::Rotation {
            loop_ids: vec![],
            rounds: 3,
            reps_per_visit: 1,
            curve: TempoCurve::Dwell { rate: 0.8 },
        }]));
        assert!(drain(r).is_empty());
    }
}
