use crate::model::TempoCurve;

impl TempoCurve {
    /// Playback rate for 0-based rep index. Clamped to [0.25, 2.0].
    pub fn rate_for_rep(&self, rep: u32) -> f64 {
        let raw = match *self {
            TempoCurve::Dwell { rate } => rate,
            TempoCurve::Ladder { start, step, target } => {
                (start + step * rep as f64).min(target)
            }
            TempoCurve::Oscillate { low, high, period } => {
                let period = period.max(1);
                if (rep + 1) % period == 0 { high } else { low }
            }
        };
        raw.clamp(0.25, 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dwell_is_constant() {
        let c = TempoCurve::Dwell { rate: 0.9 };
        assert_eq!(c.rate_for_rep(0), 0.9);
        assert_eq!(c.rate_for_rep(99), 0.9);
    }

    #[test]
    fn ladder_climbs_and_clamps_at_target() {
        let c = TempoCurve::Ladder { start: 0.6, step: 0.1, target: 0.9 };
        assert_eq!(c.rate_for_rep(0), 0.6);
        assert_eq!(c.rate_for_rep(2), 0.8);
        assert_eq!(c.rate_for_rep(3), 0.9);
        assert_eq!(c.rate_for_rep(50), 0.9);
    }

    #[test]
    fn oscillate_touches_high_every_period() {
        let c = TempoCurve::Oscillate { low: 0.7, high: 1.0, period: 3 };
        // reps 0,1 low; rep 2 high; repeat
        assert_eq!(c.rate_for_rep(0), 0.7);
        assert_eq!(c.rate_for_rep(1), 0.7);
        assert_eq!(c.rate_for_rep(2), 1.0);
        assert_eq!(c.rate_for_rep(5), 1.0);
    }

    #[test]
    fn oscillate_period_zero_treated_as_one() {
        let c = TempoCurve::Oscillate { low: 0.7, high: 1.0, period: 0 };
        assert_eq!(c.rate_for_rep(0), 1.0);
    }

    #[test]
    fn rates_clamped_to_engine_range() {
        let c = TempoCurve::Dwell { rate: 5.0 };
        assert_eq!(c.rate_for_rep(0), 2.0);
        let c = TempoCurve::Ladder { start: 0.1, step: 0.0, target: 0.1 };
        assert_eq!(c.rate_for_rep(0), 0.25);
    }
}
