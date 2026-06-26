use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SongId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectionId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LoopId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordingId(pub i64);

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
    /// When true, the section gets a per-beat click guide during playback.
    #[serde(default)]
    pub click_guide: bool,
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
    /// Manual name pinned by the user; when `Some`, the dynamic namer leaves
    /// this loop alone. `None` means `name` is algorithm-derived.
    #[serde(default)]
    pub name_override: Option<String>,
    pub start: f64,
    pub end: f64,
    pub kind: LoopKind,
}

/// What you hear: the stem balance plus the bass-focus listening aid. The live
/// isolation state is an instance of this; a routine block stores a snapshot of
/// it. `stems` are the resolved "what you hear" gains (mute/solo already folded
/// in), in the fixed order vocals/drums/bass/other — matching the engine's
/// per-stem gain index and the export contract. Speed/pitch are not part of the
/// mix; they live on the transport.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Mix {
    /// Bass-focus listening aid (low-pass / octave): on or off.
    pub bass_focus: bool,
    /// Per-stem gains, 0.0..=1.0, order vocals/drums/bass/other.
    pub stems: [f32; 4],
}

impl Default for Mix {
    /// Full band, no listening aid — the state a freshly opened song carries.
    fn default() -> Self {
        Self {
            bass_focus: false,
            stems: [1.0; 4],
        }
    }
}

/// An overdub take: your own input recorded over one pass of a span, held as an
/// additive layer. Audio lives at `<bundle>/recordings/<file>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recording {
    pub id: RecordingId,
    pub name: String,
    /// Path relative to the bundle dir, e.g. "recordings/1.wav".
    pub file: String,
    /// Source frame where capture began (the span start).
    pub anchor_frame: i64,
    /// Recorded length in frames.
    pub len_frames: i64,
    /// Per-layer manual alignment offset in frames (added to global latency).
    #[serde(default)]
    pub nudge_frames: i64,
    /// Playback gain, 0.0..=1.5.
    pub gain: f32,
    /// Muted in the layer mix.
    #[serde(default)]
    pub muted: bool,
    /// ISO-8601 creation time (set by the server when written).
    pub created_at: String,
}

/// One section's notes, keyed by occurrence label (e.g. "verse 2"). Mirrors a
/// row of the old `section_notes` table; lives in the bundle manifest now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionNote {
    pub label: String,
    pub doc: crate::notes::NotesDoc,
}

/// One suggested section from the analysis pipeline (not user truth — the
/// user edits suggestions into real `Section`s).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisSection {
    pub label: String,
    pub start: f64,
    pub end: f64,
}

/// Cached output of `scripts/analyze` for one song — mirrors the wrapper's
/// JSON contract exactly (times in seconds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub bpm: Option<f64>,
    pub beats: Vec<f64>,
    pub downbeats: Vec<f64>,
    pub sections: Vec<AnalysisSection>,
    /// What produced the sections (`beat_this`, `beat_this+novelty`, `songformer`).
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileStage {
    pub name: String,
    pub ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One timed run of a heavy operation. `started_at` is assigned by the store
/// on save (SQLite `datetime('now')`); the in-flight value is ignored on insert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileRun {
    pub op: String, // "analysis" | "stems" | "open" | "import" | "grab"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song_id: Option<SongId>,
    #[serde(default)]
    pub started_at: String,
    pub total_ms: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>, // "gpu" | "cpu" | "auto" | null
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>, // analysis only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cpu_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_gpu_util: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_vram_used_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_total_mb: Option<u32>,
    pub stages: Vec<ProfileStage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoutineId(pub i64);

/// A span of the song, beat-snapped at author time. `{start, end}` matches the
/// frontend `Span` wire shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: f64,
    pub end: f64,
}

/// When a block's count-in fires: only on block entry (`First`) or before every
/// pass of the block (`Every`). Mirrors the existing count-in `loop_mode` wire
/// values ("first" / "every").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CountInMode {
    #[default]
    First,
    Every,
}

/// Per-block count-in: `beats` clicks before the block (0 = none), firing per
/// `loop_mode`. Reuses the shipped count-in engine; a block carries its own
/// instance rather than the global setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CountIn {
    pub beats: u32,
    #[serde(default)]
    pub loop_mode: CountInMode,
}

/// One practice block: a snapshot of *how to practice a span* — where, what you
/// hear, how fast, how long, and how you come in. The routine scheduler applies
/// these on each loop pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub span: Span,
    pub mix: Mix,
    /// Playback rate (0.25..=2.0), 1.0 = original tempo.
    #[serde(default = "one")]
    pub speed: f64,
    /// Loop passes to hold this block before advancing (>= 1).
    #[serde(default = "one_pass")]
    pub passes: u32,
    /// Audio pre-roll: beats added before the span start, beat-snapped. The
    /// persisted form of the Drill box's run-up. 0 = none.
    #[serde(default)]
    pub lead_in_beats: u32,
    #[serde(default)]
    pub count_in: CountIn,
    /// Optional label; the UI falls back to a mix-derived name when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

fn one() -> f64 {
    1.0
}
fn one_pass() -> u32 {
    1
}

/// A named, ordered list of blocks, looped through on playback. Saved in the
/// song bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    pub id: RoutineId,
    pub name: String,
    #[serde(default)]
    pub blocks: Vec<Block>,
}

#[cfg(test)]
mod click_guide_tests {
    use super::*;

    #[test]
    fn click_guide_defaults_false_when_absent() {
        // Old bundles have no `click_guide` key — it must deserialize to false.
        let json = r#"{"id":1,"song_id":2,"name":"verse","start":0.0,"end":4.0,"position":0}"#;
        let s: Section = serde_json::from_str(json).unwrap();
        assert!(!s.click_guide);
    }

    #[test]
    fn click_guide_round_trips_when_true() {
        let json = r#"{"id":1,"song_id":2,"name":"verse","start":0.0,"end":4.0,"position":0,"click_guide":true}"#;
        let s: Section = serde_json::from_str(json).unwrap();
        assert!(s.click_guide);
    }
}

#[cfg(test)]
mod mix_tests {
    use super::*;

    #[test]
    fn default_is_full_band_no_focus() {
        let m = Mix::default();
        assert!(!m.bass_focus);
        assert_eq!(m.stems, [1.0; 4]);
    }

    #[test]
    fn round_trips_through_json() {
        let m = Mix {
            bass_focus: true,
            stems: [0.0, 1.0, 0.5, 0.25],
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Mix = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
