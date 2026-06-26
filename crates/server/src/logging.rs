//! When launched without a terminal (desktop entry), stderr/stdout vanish —
//! redirect them to a log file so errors are diagnosable after the fact.

use std::io::IsTerminal;
use std::path::PathBuf;

const MAX_LOG_BYTES: u64 = 1_000_000;

pub fn log_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("dredge/dredge.log")
}

/// Redirect stdout+stderr to the log file if they aren't a terminal.
/// Rotates by truncation when the log exceeds ~1 MB. Child processes
/// (webkit, demucs) inherit the redirected fds, so their noise lands
/// here too. No-op on failure — logging must never break the app.
pub fn redirect_if_headless(label: &str) {
    // With DREDGE_DEBUG set, leave stdout/stderr where the caller put them — a
    // debug session wants its output live in the terminal or in an explicit
    // `2>file`, not silently funnelled into the shared log. (Without it, a GUI
    // launched from a desktop entry still redirects so crashes are diagnosable.)
    if std::env::var_os("DREDGE_DEBUG").is_some() || std::io::stderr().is_terminal() {
        return;
    }
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Breadcrumb on the original stderr (e.g. a non-debug `2>file`) so it's never
    // a mystery where the output went.
    eprintln!("[dredge] backend log → {}", path.display());
    let truncate = std::fs::metadata(&path)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(!truncate)
        .truncate(truncate)
        .write(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(_) => return,
    };
    use std::os::fd::AsRawFd;
    let fd = file.as_raw_fd();
    // SAFETY: dup2 onto our own stdio fds with a fd that outlives the call
    // (file is leaked below so the fd stays valid for the process lifetime).
    unsafe {
        libc::dup2(fd, libc::STDOUT_FILENO);
        libc::dup2(fd, libc::STDERR_FILENO);
    }
    std::mem::forget(file);
    eprintln!(
        "--- {label} start {} ---",
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    );
}
