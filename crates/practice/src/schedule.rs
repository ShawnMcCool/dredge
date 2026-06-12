use crate::model::{LoopId, Rating};
use serde::{Deserialize, Serialize};
use time::Date;

/// Interval ladder in days. Solid advances a rung, Shaky repeats the rung,
/// Miss resets to the first rung.
pub const INTERVALS: [i64; 5] = [1, 2, 4, 7, 14];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Resurfacing {
    pub loop_id: LoopId,
    pub interval_idx: usize,
    pub due_on: Date,
}

/// Compute the next schedule state after a rated practice on `today`.
pub fn next_state(
    prev: Option<Resurfacing>,
    loop_id: LoopId,
    rating: Rating,
    today: Date,
) -> Resurfacing {
    let interval_idx = match (prev, rating) {
        (None, _) | (_, Rating::Miss) => 0,
        (Some(p), Rating::Shaky) => p.interval_idx,
        (Some(p), Rating::Solid) => (p.interval_idx + 1).min(INTERVALS.len() - 1),
    };
    Resurfacing {
        loop_id,
        interval_idx,
        due_on: today + time::Duration::days(INTERVALS[interval_idx]),
    }
}

/// Loops due on or before `today`, soonest-due first.
pub fn due(items: &[Resurfacing], today: Date) -> Vec<LoopId> {
    let mut hits: Vec<&Resurfacing> = items.iter().filter(|r| r.due_on <= today).collect();
    hits.sort_by_key(|r| r.due_on);
    hits.into_iter().map(|r| r.loop_id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn first_rating_schedules_first_interval() {
        let s = next_state(None, LoopId(1), Rating::Solid, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 0);
        assert_eq!(s.due_on, date!(2026 - 06 - 13));
    }

    #[test]
    fn solid_advances_the_ladder() {
        let prev = Resurfacing {
            loop_id: LoopId(1),
            interval_idx: 1,
            due_on: date!(2026 - 06 - 12),
        };
        let s = next_state(Some(prev), LoopId(1), Rating::Solid, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 2);
        assert_eq!(s.due_on, date!(2026 - 06 - 16)); // +4 days
    }

    #[test]
    fn solid_clamps_at_top_rung() {
        let prev = Resurfacing {
            loop_id: LoopId(1),
            interval_idx: 4,
            due_on: date!(2026 - 06 - 12),
        };
        let s = next_state(Some(prev), LoopId(1), Rating::Solid, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 4);
        assert_eq!(s.due_on, date!(2026 - 06 - 26)); // +14 days
    }

    #[test]
    fn shaky_repeats_the_rung() {
        let prev = Resurfacing {
            loop_id: LoopId(1),
            interval_idx: 2,
            due_on: date!(2026 - 06 - 12),
        };
        let s = next_state(Some(prev), LoopId(1), Rating::Shaky, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 2);
        assert_eq!(s.due_on, date!(2026 - 06 - 16));
    }

    #[test]
    fn miss_resets_to_first_rung() {
        let prev = Resurfacing {
            loop_id: LoopId(1),
            interval_idx: 3,
            due_on: date!(2026 - 06 - 12),
        };
        let s = next_state(Some(prev), LoopId(1), Rating::Miss, date!(2026 - 06 - 12));
        assert_eq!(s.interval_idx, 0);
        assert_eq!(s.due_on, date!(2026 - 06 - 13));
    }

    #[test]
    fn due_returns_overdue_and_today_sorted() {
        let items = vec![
            Resurfacing {
                loop_id: LoopId(1),
                interval_idx: 0,
                due_on: date!(2026 - 06 - 14),
            },
            Resurfacing {
                loop_id: LoopId(2),
                interval_idx: 0,
                due_on: date!(2026 - 06 - 10),
            },
            Resurfacing {
                loop_id: LoopId(3),
                interval_idx: 0,
                due_on: date!(2026 - 06 - 12),
            },
        ];
        assert_eq!(
            due(&items, date!(2026 - 06 - 12)),
            vec![LoopId(2), LoopId(3)]
        );
    }
}
