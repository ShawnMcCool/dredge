//! Live work sampling for the prepare flow: a shared work-state the heavy
//! workers update, plus a thread that samples elapsed/CPU/GPU ~1/s and emits
//! `WorkSample`s. CPU is read from /proc; GPU is a best-effort `nvidia-smi`.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// CPU percent over an interval: tick delta / clock-ticks-per-second / seconds.
/// Returns a process-tree-style number (can exceed 100 across cores).
pub fn cpu_pct(prev_ticks: u64, cur_ticks: u64, dt_secs: f64, clk_tck: u64) -> u32 {
    if dt_secs <= 0.0 || cur_ticks < prev_ticks || clk_tck == 0 {
        return 0;
    }
    let cpu_secs = (cur_ticks - prev_ticks) as f64 / clk_tck as f64;
    (cpu_secs / dt_secs * 100.0).round() as u32
}

/// Parse one `nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total
/// --format=csv,noheader,nounits` line, e.g. "38, 5120, 16376".
pub fn parse_nvidia_smi(line: &str) -> Option<(u32, u32, u32)> {
    let mut it = line.split(',').map(|s| s.trim().parse::<u32>());
    let util = it.next()?.ok()?;
    let used = it.next()?.ok()?;
    let total = it.next()?.ok()?;
    Some((util, used, total))
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkSample {
    pub op: String,
    pub stage: String,
    pub elapsed_ms: u64,
    pub cpu_pct: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_util: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_mem_used_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_mem_total_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ram_used_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ram_total_mb: Option<u32>,
}

/// What a heavy run is currently doing. Not serialized — internal only.
pub struct WorkState {
    pub op: String,
    pub stage: String,
    pub started: Instant,
    pub max_cpu: u32,
    pub max_gpu_util: Option<u32>,
    pub max_vram_used_mb: Option<u32>,
    pub vram_total_mb: Option<u32>,
}

/// The shared slot the sampler reads and the workers write.
pub type SharedWork = Arc<Mutex<Option<WorkState>>>;

/// Handle a worker uses to publish its progress into the shared slot.
#[derive(Clone)]
pub struct WorkReporter {
    state: SharedWork,
}

impl WorkReporter {
    pub fn new(state: SharedWork) -> Self {
        Self { state }
    }

    pub fn begin(&self, op: &str, stage: &str) {
        *self.state.lock().unwrap() = Some(WorkState {
            op: op.into(),
            stage: stage.into(),
            started: Instant::now(),
            max_cpu: 0,
            max_gpu_util: None,
            max_vram_used_mb: None,
            vram_total_mb: None,
        });
    }

    pub fn stage(&self, stage: &str) {
        if let Some(ws) = self.state.lock().unwrap().as_mut() {
            ws.stage = stage.into();
        }
    }

    pub fn end(&self) {
        *self.state.lock().unwrap() = None;
    }

    /// Fold one sample's metrics into the run's running maxima.
    pub fn observe(&self, cpu: u32, gpu: Option<(u32, u32, u32)>) {
        if let Some(ws) = self.state.lock().unwrap().as_mut() {
            ws.max_cpu = ws.max_cpu.max(cpu);
            if let Some((util, used, total)) = gpu {
                ws.max_gpu_util = Some(ws.max_gpu_util.unwrap_or(0).max(util));
                ws.max_vram_used_mb = Some(ws.max_vram_used_mb.unwrap_or(0).max(used));
                ws.vram_total_mb = Some(total);
            }
        }
    }

    /// Read the run's maxima (cpu, gpu_util, vram_used, vram_total), or None if idle.
    #[allow(clippy::type_complexity)]
    pub fn maxes(&self) -> Option<(u32, Option<u32>, Option<u32>, Option<u32>)> {
        self.state
            .lock()
            .unwrap()
            .as_ref()
            .map(|ws| (ws.max_cpu, ws.max_gpu_util, ws.max_vram_used_mb, ws.vram_total_mb))
    }
}

const CLK_TCK: u64 = 100; // USER_HZ on effectively all Linux
const SAMPLE_INTERVAL: Duration = Duration::from_millis(750);

/// True for the analysis/stems subprocess command lines we want to attribute
/// CPU to.
pub fn is_analysis_cmd(cmd: &str) -> bool {
    cmd.contains("songformer_impl") || cmd.contains("analyze_impl") || cmd.contains("demucs")
}

/// Sum utime+stime (clock ticks) across all processes whose cmdline matches
/// `is_analysis_cmd`. Best-effort: unreadable entries are skipped.
fn analysis_cpu_ticks() -> u64 {
    let mut total = 0u64;
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return 0;
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().filter(|s| s.bytes().all(|b| b.is_ascii_digit())) else {
            continue;
        };
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        // cmdline is NUL-separated; join with spaces for matching
        let cmd: String = cmdline
            .split(|b| *b == 0)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        if !is_analysis_cmd(&cmd) {
            continue;
        }
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            // fields after the last ')': index 0 = state (field 3); utime = field
            // 14 -> index 11, stime = field 15 -> index 12.
            if let Some(rest) = stat.rsplit(')').next() {
                let f: Vec<&str> = rest.split_whitespace().collect();
                if f.len() > 12 {
                    let utime = f[11].parse::<u64>().unwrap_or(0);
                    let stime = f[12].parse::<u64>().unwrap_or(0);
                    total += utime + stime;
                }
            }
        }
    }
    total
}

/// Parse /proc/meminfo for (used_mb, total_mb). used = MemTotal - MemAvailable.
pub fn parse_meminfo(text: &str) -> Option<(u32, u32)> {
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = rest.split_whitespace().next().and_then(|n| n.parse().ok());
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = rest.split_whitespace().next().and_then(|n| n.parse().ok());
        }
    }
    let total = total_kb?;
    let avail = avail_kb?;
    Some(((total.saturating_sub(avail) / 1024) as u32, (total / 1024) as u32))
}

/// Best-effort system RAM snapshot (used_mb, total_mb). None on failure.
fn ram_snapshot() -> Option<(u32, u32)> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// Best-effort GPU snapshot via `nvidia-smi`. None on any failure.
fn gpu_snapshot() -> Option<(u32, u32, u32)> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_nvidia_smi(text.lines().next()?)
}

/// Sampler loop: while a run is active, emit a `WorkSample` ~every 750 ms.
/// Exits when `shutdown` is set. Runs on its own thread (never the pump).
pub fn run(state: SharedWork, tx: Sender<WorkSample>, shutdown: Arc<AtomicBool>) {
    let mut prev_ticks = 0u64;
    let mut prev_at = Instant::now();
    let mut gpu_ok = true; // stop probing nvidia-smi after the first failure
    let reporter = WorkReporter::new(state.clone());
    while !shutdown.load(Ordering::SeqCst) {
        std::thread::sleep(SAMPLE_INTERVAL);
        let (op, stage, elapsed_ms) = {
            let guard = state.lock().unwrap();
            match guard.as_ref() {
                Some(ws) => (ws.op.clone(), ws.stage.clone(), ws.started.elapsed().as_millis() as u64),
                None => {
                    prev_ticks = 0; // reset between runs
                    continue;
                }
            }
        };
        let now = Instant::now();
        let cur_ticks = analysis_cpu_ticks();
        let dt = now.duration_since(prev_at).as_secs_f64();
        let cpu = if prev_ticks == 0 { 0 } else { cpu_pct(prev_ticks, cur_ticks, dt, CLK_TCK) };
        prev_ticks = cur_ticks;
        prev_at = now;
        let gpu = if gpu_ok {
            match gpu_snapshot() {
                Some(g) => Some(g),
                None => { gpu_ok = false; None }
            }
        } else {
            None
        };
        reporter.observe(cpu, gpu);
        let ram = ram_snapshot();
        let _ = tx.send(WorkSample {
            op,
            stage,
            elapsed_ms,
            cpu_pct: cpu,
            gpu_util: gpu.map(|g| g.0),
            gpu_mem_used_mb: gpu.map(|g| g.1),
            gpu_mem_total_mb: gpu.map(|g| g.2),
            ram_used_mb: ram.map(|r| r.0),
            ram_total_mb: ram.map(|r| r.1),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_pct_computes_process_tree_percent() {
        // 250 ticks over 0.5 s at 100 Hz = 2.5 cpu-seconds / 0.5 s = 500%
        assert_eq!(cpu_pct(1000, 1250, 0.5, 100), 500);
        // no progress → 0
        assert_eq!(cpu_pct(1000, 1000, 0.5, 100), 0);
        // guards
        assert_eq!(cpu_pct(1000, 900, 0.5, 100), 0);
        assert_eq!(cpu_pct(0, 100, 0.0, 100), 0);
    }

    #[test]
    fn parse_nvidia_smi_reads_three_fields() {
        assert_eq!(parse_nvidia_smi("38, 5120, 16376"), Some((38, 5120, 16376)));
        assert_eq!(parse_nvidia_smi("0,0,8192"), Some((0, 0, 8192)));
        assert_eq!(parse_nvidia_smi(""), None);
        assert_eq!(parse_nvidia_smi("garbage"), None);
        assert_eq!(parse_nvidia_smi("38, 5120"), None);
    }

    #[test]
    fn matches_analysis_process_cmdlines() {
        assert!(is_analysis_cmd("/x/songformer-venv/bin/python /x/scripts/songformer_impl.py a.mp3"));
        assert!(is_analysis_cmd("/x/analyze-venv/bin/python /x/scripts/analyze_impl.py a.mp3"));
        assert!(is_analysis_cmd("/home/u/.local/bin/demucs -n htdemucs -o /tmp a.mp3"));
        assert!(!is_analysis_cmd("/usr/bin/firefox"));
        assert!(!is_analysis_cmd(""));
    }

    #[test]
    fn reporter_observe_tracks_maxima() {
        let state = std::sync::Arc::new(std::sync::Mutex::new(None));
        let r = WorkReporter::new(state);
        r.begin("analysis", "x");
        assert_eq!(r.maxes(), Some((0, None, None, None)));
        r.observe(100, Some((40, 5000, 16000)));
        r.observe(80, Some((50, 6000, 16000)));
        r.observe(120, None);
        assert_eq!(r.maxes(), Some((120, Some(50), Some(6000), Some(16000))));
        r.end();
        assert_eq!(r.maxes(), None);
    }

    #[test]
    fn parse_meminfo_reads_used_and_total() {
        let text = "MemTotal:       32000000 kB\nMemFree:         1000000 kB\nMemAvailable:    8000000 kB\nBuffers:          200000 kB\n";
        // used = (total - available)/1024 MB = (32000000-8000000)/1024 = 23437; total = 32000000/1024 = 31250
        assert_eq!(parse_meminfo(text), Some((23437, 31250)));
        assert_eq!(parse_meminfo("garbage"), None);
        assert_eq!(parse_meminfo("MemTotal: 100 kB"), None); // no MemAvailable
    }

    #[test]
    fn reporter_begin_stage_end_drive_shared_state() {
        let state = std::sync::Arc::new(std::sync::Mutex::new(None));
        let r = WorkReporter::new(state.clone());
        assert!(state.lock().unwrap().is_none());

        r.begin("analysis", "GPU attempt");
        {
            let g = state.lock().unwrap();
            let ws = g.as_ref().unwrap();
            assert_eq!(ws.op, "analysis");
            assert_eq!(ws.stage, "GPU attempt");
        }

        r.stage("CPU recovery");
        assert_eq!(state.lock().unwrap().as_ref().unwrap().stage, "CPU recovery");

        r.end();
        assert!(state.lock().unwrap().is_none());
    }
}
