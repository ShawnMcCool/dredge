//! MP3 encoding via an out-of-process `ffmpeg` (same dependency the decode
//! ffmpeg-fallback already relies on). WAV needs no encoder — `capture::write_wav`
//! covers it — so this is the only optional output path: callers check
//! [`ffmpeg_available`] up front and offer MP3 only when it returns true.

use crate::error::{Error, Result};
use std::path::Path;
use std::process::{Command, Stdio};

/// Is an `ffmpeg` binary on PATH and runnable? Used to gate the MP3 option in
/// the UI so it's never offered-then-failed.
pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Encode `wav` to `mp3` at `bitrate_kbps` (CBR) via ffmpeg/libmp3lame.
pub fn encode_mp3(wav: &Path, mp3: &Path, bitrate_kbps: u32) -> Result<()> {
    if let Some(dir) = mp3.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(wav)
        .args(["-c:a", "libmp3lame", "-b:a"])
        .arg(format!("{bitrate_kbps}k"))
        .arg(mp3)
        .output()
        .map_err(|e| Error::Encode(format!("install ffmpeg to export MP3: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr.lines().last().unwrap_or("").trim();
        return Err(Error::Encode(format!("ffmpeg: {tail}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_mp3_writes_a_nonempty_file() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg not present — skipping MP3 encode test");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("in.wav");
        // 0.1 s of a quiet constant tone — enough for lame to emit frames.
        crate::capture::write_wav(&wav, &vec![0.1f32; 4_800 * 2]).unwrap();
        let mp3 = dir.path().join("out.mp3");
        encode_mp3(&wav, &mp3, 192).unwrap();
        let len = std::fs::metadata(&mp3).unwrap().len();
        assert!(len > 0, "mp3 file is empty");
    }
}
