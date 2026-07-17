//! Stem separation: trait, Demucs subprocess impl, and a hermetic fake.
//!
//! Stem order is a fixed contract everywhere (separator output, cache
//! layout, auto-load, UI): `practice::model::STEM_NAMES`.

use std::path::{Path, PathBuf};

pub use practice::model::STEM_NAMES;

pub trait StemSeparator: Send + Sync {
    /// Blocking; writes `<out_dir>/<stem>.wav` for every stem, returns
    /// their paths in STEM_NAMES order. `cancel` can kill the subprocess
    /// mid-run (returns `Err("cancelled")`).
    fn separate(
        &self,
        audio: &Path,
        out_dir: &Path,
        force_cpu: bool,
        cancel: &crate::proc::CancelToken,
    ) -> Result<Vec<PathBuf>, String>;
    fn is_available(&self) -> bool;
}

/// The demucs model. The 6-stem variant separates piano and guitar as their
/// own channels, which pulls piano's left hand OUT of the bass stem (the
/// 4-stem models hear piano-driven ballads as bass). Its six channels map
/// 1:1 onto `STEM_NAMES` — dredge exposes everything the model provides.
const MODEL: &str = "htdemucs_6s";

/// Runs `demucs -n htdemucs_6s -o <tmp> <audio>` and moves the stem WAVs
/// demucs writes under `<tmp>/htdemucs_6s/<track>/<stem>.wav` into place.
pub struct DemucsSeparator {
    pub binary: String, // default "demucs"
}

impl Default for DemucsSeparator {
    fn default() -> Self {
        Self {
            binary: "demucs".into(),
        }
    }
}

impl DemucsSeparator {
    /// The binary to spawn: the loose lookup's hit, or the bare name (letting
    /// the OS do its own PATH search) when nothing was found.
    fn resolved(&self) -> PathBuf {
        resolve_binary(
            &self.binary,
            &std::env::var_os("PATH").unwrap_or_default(),
            std::env::var_os("HOME").as_deref(),
        )
        .unwrap_or_else(|| PathBuf::from(&self.binary))
    }

    /// Pure: the exact argv (after the binary) for one separation run.
    fn command_args(audio: &Path, tmp: &Path) -> Vec<String> {
        vec![
            "-n".into(),
            MODEL.into(),
            "-o".into(),
            tmp.to_string_lossy().into_owned(),
            audio.to_string_lossy().into_owned(),
        ]
    }
}

impl StemSeparator for DemucsSeparator {
    fn separate(
        &self,
        audio: &Path,
        out_dir: &Path,
        force_cpu: bool,
        cancel: &crate::proc::CancelToken,
    ) -> Result<Vec<PathBuf>, String> {
        std::fs::create_dir_all(out_dir)
            .map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
        let tmp = out_dir.join(".demucs-tmp");
        // clear any staging left by a separation that was killed mid-run before
        // re-creating it — partial output never accumulates or gets reused
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| format!("cannot create tmp dir: {e}"))?;

        let bin = self.resolved();
        let mut cmd = std::process::Command::new(&bin);
        cmd.args(Self::command_args(audio, &tmp));
        if force_cpu {
            cmd.env("CUDA_VISIBLE_DEVICES", "");
        }
        let output = match crate::proc::run_cancellable(cmd, cancel) {
            crate::proc::Outcome::Done(o) => o,
            crate::proc::Outcome::Cancelled => return Err("cancelled".into()),
            crate::proc::Outcome::Err(e) => {
                return Err(format!("failed to run {}: {e}", bin.display()))
            }
        };
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(format!(
                "demucs failed ({}): {}",
                output.status,
                stderr_tail(&stderr)
            ));
        }

        let track = audio
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("audio path has no file stem")?;
        let src_dir = tmp.join(MODEL).join(track);
        let mut out = Vec::with_capacity(STEM_NAMES.len());
        for name in STEM_NAMES {
            let src = src_dir.join(format!("{name}.wav"));
            let dst = out_dir.join(format!("{name}.wav"));
            if !src.is_file() {
                return Err(format!(
                    "demucs did not produce {}: {}",
                    src.display(),
                    stderr_tail(&stderr)
                ));
            }
            std::fs::rename(&src, &dst)
                .map_err(|e| format!("cannot move {} into place: {e}", src.display()))?;
            normalize_stem_to_48k(&dst)?;
            out.push(dst);
        }
        let _ = std::fs::remove_dir_all(&tmp);
        Ok(out)
    }

    fn is_available(&self) -> bool {
        resolve_binary(
            &self.binary,
            &std::env::var_os("PATH").unwrap_or_default(),
            std::env::var_os("HOME").as_deref(),
        )
        .is_some()
    }
}

/// `which`-style scan: first executable file named `binary` in `path_var`
/// (a `PATH`-formatted OsStr). Pure in its inputs so tests don't have to
/// mutate the process environment.
pub(crate) fn find_in_path(binary: &str, path_var: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var).find_map(|dir| executable_at(dir.join(binary)))
}

/// `find_in_path` plus one loose fallback: `$HOME/.local/bin`, where
/// `uv tool install` and pipx put their shims. GUI-launched sessions often
/// carry a PATH without it, and a "not installed" error against a tool that
/// is installed is worse than a lookup that tries too hard.
pub(crate) fn resolve_binary(
    binary: &str,
    path_var: &std::ffi::OsStr,
    home: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    find_in_path(binary, path_var)
        .or_else(|| executable_at(Path::new(home?).join(".local/bin").join(binary)))
}

fn executable_at(candidate: PathBuf) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(&candidate).ok()?;
    (meta.is_file() && meta.permissions().mode() & 0o111 != 0).then_some(candidate)
}

/// Make a spawned child die (SIGKILL) when this process does, so quitting
/// mid-run never orphans the analyzer / Demucs process that's holding CPU, GPU
/// and VRAM. PR_SET_PDEATHSIG is preserved across the analyze wrapper's `exec`
/// into python, so it reaches the real worker, not just the shell.
#[cfg(target_os = "linux")]
pub(crate) fn die_with_parent(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: only async-signal-safe calls (prctl/getppid) run in the child
    // between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            // race guard: if the parent already exited (reparented to init)
            // before we armed the signal, bail before doing any work
            if libc::getppid() == 1 {
                return Err(std::io::Error::other("parent exited before child start"));
            }
            Ok(())
        });
    }
}

/// macOS / other: PR_SET_PDEATHSIG has no portable equivalent. Best-effort
/// no-op — a child orphaned by an abrupt parent exit is acceptable here
/// (analyzer / Demucs runs are short-lived and bounded).
#[cfg(not(target_os = "linux"))]
pub(crate) fn die_with_parent(_cmd: &mut std::process::Command) {}

/// Replace a stem WAV with `interleaved` at the engine's 48 kHz —
/// write-to-tmp + rename so a crash never leaves a truncated cache entry.
pub(crate) fn rewrite_wav_48k(path: &Path, interleaved: &[f32]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    engine::capture::write_wav(&tmp, interleaved).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
}

/// One sinc pass at separation time, never again on open: if the WAV's
/// header rate isn't 48 kHz (demucs writes 44.1 kHz), decode it (which
/// resamples to 48 kHz) and rewrite it in place.
pub(crate) fn normalize_stem_to_48k(path: &Path) -> Result<(), String> {
    let rate = engine::capture::wav_header_rate(path).map_err(|e| e.to_string())?;
    if rate == engine::buffer::SAMPLE_RATE {
        return Ok(());
    }
    let buf = engine::decode::decode_file(path).map_err(|e| e.to_string())?;
    rewrite_wav_48k(path, &buf.data)
}

pub(crate) fn stderr_tail(stderr: &str) -> String {
    let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
    tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
}

/// Test double: writes one copy of the input per stem, each scaled by a
/// distinct factor so tests can tell stems apart.
pub struct FakeSeparator;

const FAKE_SCALES: [f32; 6] = [0.4, 0.3, 0.2, 0.16, 0.12, 0.1];

impl StemSeparator for FakeSeparator {
    fn separate(
        &self,
        audio: &Path,
        out_dir: &Path,
        _force_cpu: bool,
        _cancel: &crate::proc::CancelToken,
    ) -> Result<Vec<PathBuf>, String> {
        let buf = engine::decode::decode_file(audio).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(STEM_NAMES.len());
        for (name, scale) in STEM_NAMES.iter().zip(FAKE_SCALES) {
            let scaled: Vec<f32> = buf.data.iter().map(|s| s * scale).collect();
            let path = out_dir.join(format!("{name}.wav"));
            engine::capture::write_wav(&path, &scaled).map_err(|e| e.to_string())?;
            // same invariant as the real separator: caches are always 48 kHz
            normalize_stem_to_48k(&path)?;
            out.push(path);
        }
        Ok(out)
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demucs_command_args_are_exact() {
        let args = DemucsSeparator::command_args(Path::new("/songs/a.mp3"), Path::new("/tmp/out"));
        assert_eq!(
            args,
            vec!["-n", "htdemucs_6s", "-o", "/tmp/out", "/songs/a.mp3"]
        );
    }

    #[test]
    fn is_available_scans_path_for_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("demucs");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let path_var = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(find_in_path("demucs", &path_var), Some(bin));
        assert_eq!(find_in_path("not-a-binary", &path_var), None);

        // a non-executable file does not count
        let plain = dir.path().join("plain");
        std::fs::write(&plain, "data").unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(find_in_path("plain", &path_var), None);
    }

    #[test]
    fn resolve_binary_falls_back_to_home_local_bin() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let local = home.path().join(".local/bin");
        std::fs::create_dir_all(&local).unwrap();
        let bin = local.join("demucs");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        // PATH doesn't have it (the GUI-session case) → the home fallback finds it
        let empty = std::ffi::OsString::new();
        assert_eq!(
            resolve_binary("demucs", &empty, Some(home.path().as_os_str())),
            Some(bin.clone())
        );
        // no home → no fallback
        assert_eq!(resolve_binary("demucs", &empty, None), None);
        // PATH hit wins over the fallback
        let dir = tempfile::tempdir().unwrap();
        let path_bin = dir.path().join("demucs");
        std::fs::write(&path_bin, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path_bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path_var = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(
            resolve_binary("demucs", &path_var, Some(home.path().as_os_str())),
            Some(path_bin)
        );
    }

    #[test]
    fn normalize_rewrites_non_48k_wav_in_place() {
        // 1 s of 440 Hz stereo sine at 44.1 kHz — the legacy cache format
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bass.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..44_100 {
            let v = (i as f32 / 44_100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let s = (v * i16::MAX as f32) as i16;
            w.write_sample(s).unwrap();
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();

        normalize_stem_to_48k(&path).unwrap();
        assert_eq!(engine::capture::wav_header_rate(&path).unwrap(), 48_000);
        // duration survives the resample: 1 s → 48 000 frames
        let buf = engine::decode::decode_file(&path).unwrap();
        assert!(
            (buf.duration_secs() - 1.0).abs() < 0.01,
            "duration = {}",
            buf.duration_secs()
        );

        // already-48 kHz input is a no-op (and stays valid)
        normalize_stem_to_48k(&path).unwrap();
        assert_eq!(engine::capture::wav_header_rate(&path).unwrap(), 48_000);
    }

    #[test]
    fn separate_forwards_force_cpu_env() {
        let dir = tempfile::tempdir().unwrap();
        // stub `demucs` that fails unless CUDA_VISIBLE_DEVICES is empty,
        // proving force_cpu reached the Command env.
        let bin = dir.path().join("demucs");
        std::fs::write(
            &bin,
            "#!/bin/sh\nif [ -z \"${CUDA_VISIBLE_DEVICES+x}\" ]; then exit 7; fi\nexit 9\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let sep = DemucsSeparator {
            binary: bin.to_string_lossy().into_owned(),
        };
        let out = dir.path().join("out");
        // force_cpu=true → env present → stub exits 9 (not 7); separate returns Err
        // whose message contains the exit code, proving the env was set.
        let err = sep
            .separate(
                Path::new("/tmp/a.mp3"),
                &out,
                true,
                &crate::proc::CancelToken::default(),
            )
            .unwrap_err();
        assert!(err.contains("9"), "force_cpu must set the env: {err}");
    }

    #[test]
    fn fake_separator_writes_decodable_stems_with_distinct_rms() {
        // 1 s of 440 Hz sine at 0.5 amplitude
        let samples: Vec<f32> = (0..48_000)
            .flat_map(|i| {
                let v = (i as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                [v, v]
            })
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("song.wav");
        engine::capture::write_wav(&src, &samples).unwrap();

        let out_dir = dir.path().join("stems");
        let paths = FakeSeparator
            .separate(&src, &out_dir, false, &crate::proc::CancelToken::default())
            .unwrap();
        assert_eq!(paths.len(), STEM_NAMES.len());

        let rms = |data: &[f32]| -> f64 {
            (data.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / data.len() as f64).sqrt()
        };
        let mut stem_rms = Vec::new();
        for (path, name) in paths.iter().zip(STEM_NAMES) {
            assert_eq!(path, &out_dir.join(format!("{name}.wav")));
            assert!(path.is_file(), "missing {}", path.display());
            // cache invariant: stems are stored at the engine's 48 kHz
            assert_eq!(engine::capture::wav_header_rate(path).unwrap(), 48_000);
            let buf = engine::decode::decode_file(path).unwrap();
            stem_rms.push(rms(&buf.data));
        }
        // ratios follow FAKE_SCALES against the loudest stem
        for (i, expect) in FAKE_SCALES
            .map(|s| (s / FAKE_SCALES[0]) as f64)
            .into_iter()
            .enumerate()
        {
            let ratio = stem_rms[i] / stem_rms[0];
            assert!(
                (ratio - expect).abs() < 0.02,
                "stem {i} ratio = {ratio}, expected {expect}"
            );
        }
    }
}
