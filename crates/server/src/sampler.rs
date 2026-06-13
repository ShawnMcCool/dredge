//! Live work sampling for the prepare flow: a shared work-state the heavy
//! workers update, plus a thread that samples elapsed/CPU/GPU ~1/s and emits
//! `WorkSample`s. CPU is read from /proc; GPU is a best-effort `nvidia-smi`.

use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
}

/// What a heavy run is currently doing. Not serialized — internal only.
pub struct WorkState {
    pub op: String,
    pub stage: String,
    pub started: Instant,
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
