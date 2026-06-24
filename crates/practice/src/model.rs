use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SongId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectionId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LoopId(pub i64);

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
