use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SongId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectionId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LoopId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanId(pub i64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub id: SongId,
    pub title: String,
    pub artist: Option<String>,
    pub path: String,
    pub file_hash: String,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: SectionId,
    pub song_id: SongId,
    pub name: String,
    pub start: f64,
    pub end: f64,
    /// 0-based order within the song.
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoopKind {
    Manual,
    Junction {
        from_section: SectionId,
        to_section: SectionId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopRegion {
    pub id: LoopId,
    pub song_id: SongId,
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub kind: LoopKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "curve", rename_all = "snake_case")]
pub enum TempoCurve {
    /// Constant rate (the submaximal-dwell default, e.g. 0.9).
    Dwell { rate: f64 },
    /// start + step per rep, clamped at target.
    Ladder { start: f64, step: f64, target: f64 },
    /// Every `period`-th rep at `high`, others at `low` (touch target early).
    Oscillate { low: f64, high: f64, period: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum PlanStep {
    /// Playback-only passes before play-along (audiation gate).
    ListenFirst { loop_id: LoopId, reps: u32 },
    PlayReps {
        loop_id: LoopId,
        reps: u32,
        curve: TempoCurve,
    },
    /// Interleaved rotation over several loops.
    Rotation {
        loop_ids: Vec<LoopId>,
        rounds: u32,
        reps_per_visit: u32,
        curve: TempoCurve,
    },
    /// Alternate audible pass / silent pass (play from memory).
    RecallTest {
        loop_id: LoopId,
        alternations: u32,
        rate: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: PlanId,
    pub song_id: SongId,
    pub name: String,
    pub steps: Vec<PlanStep>,
}

/// Self-rating after reps — musician-friendly three-point scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Miss,
    Shaky,
    Solid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_steps_roundtrip_through_json() {
        let steps = vec![
            PlanStep::ListenFirst {
                loop_id: LoopId(1),
                reps: 3,
            },
            PlanStep::PlayReps {
                loop_id: LoopId(1),
                reps: 5,
                curve: TempoCurve::Oscillate {
                    low: 0.7,
                    high: 1.0,
                    period: 3,
                },
            },
            PlanStep::Rotation {
                loop_ids: vec![LoopId(1), LoopId(2)],
                rounds: 2,
                reps_per_visit: 2,
                curve: TempoCurve::Dwell { rate: 0.9 },
            },
            PlanStep::RecallTest {
                loop_id: LoopId(2),
                alternations: 4,
                rate: 1.0,
            },
        ];
        let json = serde_json::to_string(&steps).unwrap();
        let back: Vec<PlanStep> = serde_json::from_str(&json).unwrap();
        assert_eq!(steps, back);
        // tagged representation is the sidecar/socket contract — pin it
        assert!(json.contains("\"step\":\"listen_first\""));
        assert!(json.contains("\"curve\":\"oscillate\""));
    }
}
