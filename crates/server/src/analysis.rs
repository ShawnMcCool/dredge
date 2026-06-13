//! Auto analysis (beat grid + section suggestions): trait, `scripts/analyze`
//! subprocess impl, and a hermetic fake.
//!
//! The wrapper script owns the models and venvs; this side only knows the
//! JSON contract (`practice::model::Analysis`). Swapping section models
//! never touches Rust.

use practice::model::{Analysis, AnalysisSection};
use std::path::{Path, PathBuf};

pub trait Analyzer: Send + Sync {
    /// Blocking; runs the full analysis pipeline on one audio file.
    /// When `force_cpu` is true the subprocess is started with
    /// `CUDA_VISIBLE_DEVICES=""` so auto-detection falls back to CPU.
    fn analyze(&self, audio: &Path, force_cpu: bool) -> Result<Analysis, String>;
    fn is_available(&self) -> bool;
}

/// Runs the repo-shipped `scripts/analyze` wrapper (it bootstraps its own
/// venv) and parses the single JSON object it prints to stdout.
pub struct ScriptAnalyzer {
    script: Option<PathBuf>,
}

impl Default for ScriptAnalyzer {
    fn default() -> Self {
        Self {
            script: resolve_script(),
        }
    }
}

impl ScriptAnalyzer {
    /// Pin the script path explicitly (tests use a stub).
    pub fn with_script(script: PathBuf) -> Self {
        Self {
            script: Some(script),
        }
    }
}

/// Resolution order: `../../scripts/analyze` relative to the running
/// executable (`target/{debug,release}/earwormd` → repo root), then
/// `$EARWORM_ANALYZE`, then an `earworm-analyze` on PATH.
fn resolve_script() -> Option<PathBuf> {
    if let Some(candidate) = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("../../scripts/analyze")))
    {
        if candidate.is_file() {
            return candidate.canonicalize().ok();
        }
    }
    if let Some(env) = std::env::var_os("EARWORM_ANALYZE") {
        let p = PathBuf::from(env);
        if p.is_file() {
            return Some(p);
        }
    }
    crate::stems::find_in_path(
        "earworm-analyze",
        &std::env::var_os("PATH").unwrap_or_default(),
    )
}

impl Analyzer for ScriptAnalyzer {
    fn analyze(&self, audio: &Path, force_cpu: bool) -> Result<Analysis, String> {
        let script = self.script.as_ref().ok_or(
            "analysis script not found — expected <repo>/scripts/analyze (or set $EARWORM_ANALYZE)",
        )?;
        let mut cmd = std::process::Command::new(script);
        cmd.arg(audio);
        if force_cpu {
            cmd.env("CUDA_VISIBLE_DEVICES", "");
        }
        let output = cmd
            .output()
            .map_err(|e| format!("failed to run {}: {e}", script.display()))?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(format!(
                "analyze failed ({}): {}",
                output.status,
                crate::stems::stderr_tail(&stderr)
            ));
        }
        serde_json::from_slice(&output.stdout).map_err(|e| {
            format!(
                "analyze printed invalid JSON: {e} ({})",
                crate::stems::stderr_tail(&stderr)
            )
        })
    }

    fn is_available(&self) -> bool {
        self.script.is_some()
    }
}

/// Test double: a fixed beat grid (120 bpm, 4/4, downbeats every 2 s) plus
/// two suggested sections. Deterministic so tests can assert exact windows.
pub struct FakeAnalyzer;

pub fn fake_analysis() -> Analysis {
    Analysis {
        bpm: Some(120.0),
        beats: (0..=40).map(|i| f64::from(i) * 0.5).collect(),
        downbeats: (0..=10).map(|i| f64::from(i) * 2.0).collect(),
        sections: vec![
            AnalysisSection {
                label: "A".into(),
                start: 0.0,
                end: 4.9,
            },
            AnalysisSection {
                label: "B".into(),
                start: 4.9,
                end: 10.0,
            },
        ],
        engine: "fake".into(),
    }
}

impl Analyzer for FakeAnalyzer {
    fn analyze(&self, _audio: &Path, _force_cpu: bool) -> Result<Analysis, String> {
        Ok(fake_analysis())
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// Run analysis honoring the device setting, recovering a CUDA-OOM SongFormer
/// fallback onto CPU when `auto`. Records stages into `timer`. Returns the
/// chosen result and the resolved device label.
pub fn analyze_with_recovery(
    analyzer: &dyn Analyzer,
    audio: &Path,
    device_setting: &str,
    timer: &mut crate::profile::Timer,
) -> (Result<Analysis, String>, Option<String>) {
    if device_setting == "cpu" {
        let r = timer.stage("analyze", || analyzer.analyze(audio, true));
        return (r, Some("cpu".into()));
    }
    // auto: GPU first
    let r = timer.stage("analyze (gpu)", || analyzer.analyze(audio, false));
    match &r {
        Ok(a) if a.engine == "songformer" => (r, Some("gpu".into())),
        Ok(_) if songformer_venv_present() => {
            timer.note_last("songformer fell back; retrying on cpu");
            let r2 = timer.stage("analyze (cpu)", || analyzer.analyze(audio, true));
            match &r2 {
                Ok(a2) if a2.engine == "songformer" => (r2, Some("cpu".into())),
                _ => (r2, None),
            }
        }
        _ => (r, None),
    }
}

/// True when the optional SongFormer venv python is present and executable —
/// mirrors `scripts/analyze_impl.py::songformer_python`.
pub fn songformer_venv_present() -> bool {
    use std::os::unix::fs::PermissionsExt;
    let venv = std::env::var_os("EARWORM_SONGFORMER_VENV")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".local/share/earworm/songformer-venv")
        });
    let py = venv.join("bin/python");
    std::fs::metadata(&py)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn stub_script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("analyze");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn analyze_parses_the_stdout_contract_and_ignores_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let script = stub_script(
            dir.path(),
            r#"echo "bootstrap noise" >&2
echo '{"bpm": 98.2, "beats": [0.61, 1.22], "downbeats": [0.61],
       "sections": [{"label": "A", "start": 0.0, "end": 31.4}],
       "engine": "beat_this+novelty"}'"#,
        );
        let a = ScriptAnalyzer::with_script(script)
            .analyze(Path::new("/tmp/x.mp3"), false)
            .unwrap();
        assert_eq!(a.bpm, Some(98.2));
        assert_eq!(a.beats, vec![0.61, 1.22]);
        assert_eq!(a.downbeats, vec![0.61]);
        assert_eq!(a.sections.len(), 1);
        assert_eq!(a.engine, "beat_this+novelty");
    }

    #[test]
    fn analyze_surfaces_stderr_tail_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let script = stub_script(dir.path(), "echo 'cuda exploded' >&2\nexit 3");
        let err = ScriptAnalyzer::with_script(script)
            .analyze(Path::new("/tmp/x.mp3"), false)
            .unwrap_err();
        assert!(err.contains("cuda exploded"), "err: {err}");
    }

    #[test]
    fn force_cpu_sets_cuda_visible_devices_empty() {
        let dir = tempfile::tempdir().unwrap();
        // stub prints engine reflecting whether CUDA was hidden
        let script = stub_script(
            dir.path(),
            r#"if [ -z "${CUDA_VISIBLE_DEVICES+x}" ]; then ENG=unset; \
elif [ -z "$CUDA_VISIBLE_DEVICES" ]; then ENG=cpu; else ENG=gpu; fi
echo "{\"bpm\":1.0,\"beats\":[],\"downbeats\":[],\"sections\":[],\"engine\":\"$ENG\"}""#,
        );
        let a = ScriptAnalyzer::with_script(script);
        assert_eq!(a.analyze(Path::new("/tmp/x.mp3"), true).unwrap().engine, "cpu");
        assert_eq!(a.analyze(Path::new("/tmp/x.mp3"), false).unwrap().engine, "unset");
    }

    #[test]
    fn unavailable_without_a_script() {
        let a = ScriptAnalyzer { script: None };
        assert!(!a.is_available());
        assert!(a.analyze(Path::new("/tmp/x.mp3"), false).is_err());
    }
}
