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

/// The stem vocabulary — the demucs 6-stem model's channels, in the one fixed
/// wire order shared by the engine's gain indices, `Mix.stems`, the stem cache
/// file names, and the UI's fader row.
pub const STEM_NAMES: [&str; 6] = ["vocals", "drums", "bass", "guitar", "piano", "other"];
pub const STEM_COUNT: usize = STEM_NAMES.len();

/// What you hear: the stem balance plus the bass-focus listening aid. The live
/// isolation state is an instance of this; a routine block stores a snapshot of
/// it. `stems` are the resolved "what you hear" gains (mute/solo already folded
/// in), in `STEM_NAMES` order — matching the engine's per-stem gain index and
/// the export contract. Speed/pitch are not part of the mix; they live on the
/// transport.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Mix {
    /// Bass-focus listening aid (low-pass / octave): on or off.
    pub bass_focus: bool,
    /// Per-stem gains, 0.0..=1.0, in `STEM_NAMES` order.
    #[serde(deserialize_with = "stems_compat")]
    pub stems: [f32; STEM_COUNT],
}

/// Accept stem-gain vectors of any stored length: mixes saved before the
/// 6-stem vocabulary carry four gains, and the new channels default to full.
/// Extra entries (a future shrink) are dropped rather than erroring.
fn stems_compat<'de, D>(d: D) -> Result<[f32; STEM_COUNT], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Vec<f32> = Vec::deserialize(d)?;
    let mut out = [1.0; STEM_COUNT];
    for (slot, g) in out.iter_mut().zip(v) {
        *slot = g;
    }
    Ok(out)
}

impl Default for Mix {
    /// Full band, no listening aid — the state a freshly opened song carries.
    fn default() -> Self {
        Self {
            bass_focus: false,
            stems: [1.0; STEM_COUNT],
        }
    }
}

/// Saved per-song isolation-box state: the bass-focus toggle plus each stem's
/// fader level, mute, and solo. Restored verbatim on `song.open`. Distinct from
/// `Mix` (resolved gains) because it preserves the mute/solo toggles, not just
/// the resulting sound. Stored as `Vec`s and normalized to `STEM_COUNT` on read
/// so a state saved under an older stem vocabulary still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Isolation {
    #[serde(default)]
    pub bass_focus: bool,
    #[serde(default)]
    pub levels: Vec<u8>,
    #[serde(default)]
    pub mutes: Vec<bool>,
    #[serde(default)]
    pub solos: Vec<bool>,
}

impl Default for Isolation {
    /// Full band, no listening aid — a freshly opened song's isolation state.
    fn default() -> Self {
        Self {
            bass_focus: false,
            levels: vec![100; STEM_COUNT],
            mutes: vec![false; STEM_COUNT],
            solos: vec![false; STEM_COUNT],
        }
    }
}

impl Isolation {
    /// Pad/truncate every vector to exactly `STEM_COUNT`: missing stems default
    /// to full level, unmuted, unsoloed; extras are dropped.
    pub fn normalized(&self) -> Isolation {
        fn fit<T: Clone>(v: &[T], fill: T) -> Vec<T> {
            let mut out = v.to_vec();
            out.truncate(STEM_COUNT);
            while out.len() < STEM_COUNT {
                out.push(fill.clone());
            }
            out
        }
        Isolation {
            bass_focus: self.bass_focus,
            levels: fit(&self.levels, 100),
            mutes: fit(&self.mutes, false),
            solos: fit(&self.solos, false),
        }
    }
}

/// A numbered per-song position marker (seconds). Slots are stable handles the
/// global pedal mapping points at ("play from marker 2"); the number is the
/// identity, not an ordering of creation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub slot: u32,
    pub pos: f64,
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
        assert_eq!(m.stems, [1.0; STEM_COUNT]);
    }

    #[test]
    fn round_trips_through_json() {
        let m = Mix {
            bass_focus: true,
            stems: [0.0, 1.0, 0.5, 0.25, 0.75, 0.1],
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Mix = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn four_gain_mixes_from_before_six_stems_load_padded() {
        let back: Mix =
            serde_json::from_str(r#"{"bass_focus":false,"stems":[0.0,1.0,0.5,0.25]}"#).unwrap();
        assert_eq!(back.stems, [0.0, 1.0, 0.5, 0.25, 1.0, 1.0]);
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;

    #[test]
    fn default_is_full_band_no_focus() {
        let i = Isolation::default();
        assert!(!i.bass_focus);
        assert_eq!(i.normalized().levels, vec![100; STEM_COUNT]);
        assert_eq!(i.normalized().mutes, vec![false; STEM_COUNT]);
        assert_eq!(i.normalized().solos, vec![false; STEM_COUNT]);
    }

    #[test]
    fn normalize_pads_short_to_stem_count() {
        let i = Isolation {
            bass_focus: true,
            levels: vec![10, 20, 30, 40],
            mutes: vec![true],
            solos: vec![],
        };
        let n = i.normalized();
        assert_eq!(n.levels, vec![10, 20, 30, 40, 100, 100]);
        assert_eq!(n.mutes, vec![true, false, false, false, false, false]);
        assert_eq!(n.solos, vec![false; STEM_COUNT]);
        assert!(n.bass_focus);
    }

    #[test]
    fn normalize_truncates_long_to_stem_count() {
        let i = Isolation {
            bass_focus: false,
            levels: vec![1; STEM_COUNT + 3],
            mutes: vec![true; STEM_COUNT + 2],
            solos: vec![true; STEM_COUNT + 1],
        };
        let n = i.normalized();
        assert_eq!(n.levels.len(), STEM_COUNT);
        assert_eq!(n.mutes.len(), STEM_COUNT);
        assert_eq!(n.solos.len(), STEM_COUNT);
    }

    #[test]
    fn serde_round_trip() {
        let i = Isolation {
            bass_focus: true,
            levels: vec![50; STEM_COUNT],
            mutes: vec![false; STEM_COUNT],
            solos: vec![true; STEM_COUNT],
        };
        let s = serde_json::to_string(&i).unwrap();
        let back: Isolation = serde_json::from_str(&s).unwrap();
        assert_eq!(i, back);
    }
}

#[cfg(test)]
mod marker_tests {
    use super::*;

    #[test]
    fn marker_round_trips() {
        let m = Marker { slot: 2, pos: 92.5 };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<Marker>(&s).unwrap(), m);
    }
}
