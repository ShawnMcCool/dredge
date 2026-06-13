import type { ProfileRun, ProfileStage } from "./stores";

export interface EffortSummary {
  op: string;
  total_ms: number;
  device?: string;
  engine?: string;
  stages: ProfileStage[];
  maxLine: string | null;
}

/** Newest profile per op (analysis, then stems) from a most-recent-first list,
 *  shaped for the completion summary. */
export function effortSummaries(profiles: ProfileRun[]): EffortSummary[] {
  const out: EffortSummary[] = [];
  for (const op of ["analysis", "stems"]) {
    const p = profiles.find((r) => r.op === op);
    if (!p) continue;
    const maxLine =
      p.max_cpu_pct != null
        ? [
            `cpu ${p.max_cpu_pct}%`,
            p.max_gpu_util != null ? `gpu ${p.max_gpu_util}%` : null,
            p.vram_total_mb != null
              ? `vram ${((p.max_vram_used_mb ?? 0) / 1024).toFixed(1)}/${Math.round(p.vram_total_mb / 1024)} GB`
              : null,
          ]
            .filter(Boolean)
            .join(" · ")
        : null;
    out.push({ op: p.op, total_ms: p.total_ms, device: p.device, engine: p.engine, stages: p.stages, maxLine });
  }
  return out;
}
