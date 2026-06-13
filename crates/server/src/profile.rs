//! Minimal stopwatch for heavy-op profiling. No framework — `Instant` + stages.

use practice::model::SongId;
use practice::model::{ProfileRun, ProfileStage};
use std::time::Instant;

pub struct Timer {
    op: String,
    song_id: Option<SongId>,
    start: Instant,
    stages: Vec<ProfileStage>,
}

impl Timer {
    pub fn new(op: &str, song_id: Option<SongId>) -> Self {
        Self {
            op: op.into(),
            song_id,
            start: Instant::now(),
            stages: Vec::new(),
        }
    }

    /// Time `f`, record a stage with `name`, return f's value.
    pub fn stage<T>(&mut self, name: &str, f: impl FnOnce() -> T) -> T {
        let t0 = Instant::now();
        let out = f();
        self.stages.push(ProfileStage {
            name: name.into(),
            ms: t0.elapsed().as_millis() as u64,
            note: None,
        });
        out
    }

    /// Attach a note to the most recently recorded stage.
    pub fn note_last(&mut self, note: &str) {
        if let Some(s) = self.stages.last_mut() {
            s.note = Some(note.into());
        }
    }

    pub fn finish(
        self,
        ok: bool,
        error: Option<String>,
        device: Option<String>,
        engine: Option<String>,
    ) -> ProfileRun {
        ProfileRun {
            op: self.op,
            song_id: self.song_id,
            started_at: String::new(),
            total_ms: self.start.elapsed().as_millis() as u64,
            ok,
            error,
            device,
            engine,
            max_cpu_pct: None,
            max_gpu_util: None,
            max_vram_used_mb: None,
            vram_total_mb: None,
            stages: self.stages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_records_stages_and_total() {
        let mut t = Timer::new("analysis", Some(SongId(3)));
        let v = t.stage("a", || 21 + 21);
        assert_eq!(v, 42);
        t.note_last("ok");
        let run = t.finish(true, None, Some("cpu".into()), Some("songformer".into()));
        assert_eq!(run.op, "analysis");
        assert_eq!(run.song_id, Some(SongId(3)));
        assert_eq!(run.stages.len(), 1);
        assert_eq!(run.stages[0].name, "a");
        assert_eq!(run.stages[0].note.as_deref(), Some("ok"));
        assert_eq!(run.device.as_deref(), Some("cpu"));
    }
}
