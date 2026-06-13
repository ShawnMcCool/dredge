//! Live work sampling for the prepare flow: a shared work-state the heavy
//! workers update, plus a thread that samples elapsed/CPU/GPU ~1/s and emits
//! `WorkSample`s. CPU is read from /proc; GPU is a best-effort `nvidia-smi`.

use serde::Serialize;

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
}
