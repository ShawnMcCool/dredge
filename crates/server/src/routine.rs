//! The practice-routine scheduler: a pure state machine that advances through a
//! routine's blocks on loop-pass signals. It owns *only* the position in the
//! routine (which block, how many passes remain); the `App` executes the audio
//! side — loop region, mix, rate, count-in — from `current_block`. Keeping the
//! scheduling logic here, behind the dispatch surface, makes routine-run state
//! authoritative and testable rather than smeared into the UI.

use practice::model::{Block, Routine, RoutineId, SongId};
use serde::Serialize;

/// Live state of a running routine.
pub struct RoutineRunner {
    pub song_id: SongId,
    routine: Routine,
    block_idx: usize,
    /// Loop passes still to play on the current block before advancing.
    passes_remaining: u32,
}

impl RoutineRunner {
    /// Start a routine at its first block. `None` if the routine has no blocks.
    pub fn new(song_id: SongId, routine: Routine) -> Option<Self> {
        Self::new_from(song_id, routine, 0)
    }

    /// Start a routine at block `start_idx` (clamped to the last block). `None`
    /// if the routine has no blocks. Used to jump straight into a chosen block.
    pub fn new_from(song_id: SongId, routine: Routine, start_idx: usize) -> Option<Self> {
        if routine.blocks.is_empty() {
            return None;
        }
        let block_idx = start_idx.min(routine.blocks.len() - 1);
        let passes_remaining = routine.blocks[block_idx].passes.max(1);
        Some(Self {
            song_id,
            routine,
            block_idx,
            passes_remaining,
        })
    }

    pub fn current_block(&self) -> &Block {
        &self.routine.blocks[self.block_idx]
    }

    pub fn routine_id(&self) -> RoutineId {
        self.routine.id
    }

    /// Register one elapsed loop pass. Returns `true` when the active block
    /// changed and the caller must re-apply it. Routines loop: after the last
    /// block it wraps to the first.
    pub fn on_wrap(&mut self) -> bool {
        self.passes_remaining = self.passes_remaining.saturating_sub(1);
        if self.passes_remaining > 0 {
            return false;
        }
        self.block_idx = (self.block_idx + 1) % self.routine.blocks.len();
        self.passes_remaining = self.current_block().passes.max(1);
        true
    }

    pub fn status(&self) -> RoutineStatus {
        RoutineStatus {
            running: true,
            routine_id: self.routine.id,
            block_index: self.block_idx,
            block_count: self.routine.blocks.len(),
            passes_remaining: self.passes_remaining,
            block: self.current_block().clone(),
        }
    }
}

/// What the UI needs to reflect a running routine: which block is active (so the
/// indicator and the isolation/transport controls can track it) and the block
/// itself (mix + speed the faders animate toward).
#[derive(Debug, Clone, Serialize)]
pub struct RoutineStatus {
    pub running: bool,
    pub routine_id: RoutineId,
    pub block_index: usize,
    pub block_count: usize,
    pub passes_remaining: u32,
    pub block: Block,
}

#[cfg(test)]
mod tests {
    use super::*;
    use practice::model::{CountIn, Mix, Span};

    fn block(passes: u32, name: &str) -> Block {
        Block {
            span: Span {
                start: 0.0,
                end: 8.0,
            },
            mix: Mix::default(),
            speed: 1.0,
            passes,
            lead_in_beats: 0,
            count_in: CountIn::default(),
            name: Some(name.into()),
        }
    }

    fn routine(blocks: Vec<Block>) -> Routine {
        Routine {
            id: RoutineId(7),
            name: "r".into(),
            blocks,
        }
    }

    #[test]
    fn empty_routine_does_not_run() {
        assert!(RoutineRunner::new(SongId(1), routine(vec![])).is_none());
    }

    #[test]
    fn single_pass_blocks_advance_each_wrap_and_loop() {
        let mut r =
            RoutineRunner::new(SongId(1), routine(vec![block(1, "a"), block(1, "b")])).unwrap();
        assert_eq!(r.status().block_index, 0);
        assert!(r.on_wrap()); // a done → b
        assert_eq!(r.status().block_index, 1);
        assert!(r.on_wrap()); // b done → wrap to a
        assert_eq!(r.status().block_index, 0);
    }

    #[test]
    fn multi_pass_block_holds_then_advances() {
        let mut r =
            RoutineRunner::new(SongId(1), routine(vec![block(3, "a"), block(1, "b")])).unwrap();
        assert!(!r.on_wrap(), "pass 1 of 3 — hold");
        assert!(!r.on_wrap(), "pass 2 of 3 — hold");
        assert!(r.on_wrap(), "pass 3 of 3 — advance");
        assert_eq!(r.status().block_index, 1);
        assert_eq!(r.status().passes_remaining, 1);
    }

    #[test]
    fn new_from_starts_at_chosen_block_and_clamps() {
        let r = RoutineRunner::new_from(SongId(1), routine(vec![block(1, "a"), block(2, "b")]), 1)
            .unwrap();
        assert_eq!(r.status().block_index, 1);
        assert_eq!(r.status().passes_remaining, 2);
        // Out-of-range start clamps to the last block.
        let r2 =
            RoutineRunner::new_from(SongId(1), routine(vec![block(1, "a")]), 9).expect("clamped");
        assert_eq!(r2.status().block_index, 0);
    }

    #[test]
    fn zero_passes_is_treated_as_one() {
        let mut r = RoutineRunner::new(SongId(1), routine(vec![block(0, "a"), block(0, "b")]))
            .expect("blocks present");
        assert_eq!(r.status().passes_remaining, 1);
        assert!(r.on_wrap());
        assert_eq!(r.status().block_index, 1);
    }
}
