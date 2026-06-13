import { describe, expect, it } from "vitest";
import { effortSummaries } from "./livesummary";
import type { ProfileRun } from "./stores";

const run = (op: string, over: Partial<ProfileRun> = {}): ProfileRun => ({
  op, started_at: "", total_ms: 1000, ok: true, stages: [], ...over,
});

describe("effortSummaries", () => {
  it("picks the newest profile per op and formats the max line", () => {
    const profiles: ProfileRun[] = [
      run("stems", { total_ms: 15000, stages: [{ name: "demucs", ms: 15000 }], max_cpu_pct: 180, max_gpu_util: 62, max_vram_used_mb: 3200, vram_total_mb: 16000 }),
      run("analysis", { total_ms: 217000, device: "cpu", engine: "songformer",
        stages: [{ name: "GPU attempt", ms: 22600 }, { name: "CPU recovery", ms: 194400 }],
        max_cpu_pct: 496, max_gpu_util: 41, max_vram_used_mb: 6100, vram_total_mb: 16000 }),
      run("analysis", { total_ms: 999999 }), // older analysis — must be ignored
    ];
    const s = effortSummaries(profiles);
    expect(s.map((e) => e.op)).toEqual(["analysis", "stems"]);
    const a = s[0];
    expect(a.total_ms).toBe(217000);
    expect(a.engine).toBe("songformer");
    expect(a.stages).toHaveLength(2);
    expect(a.maxLine).toBe("cpu 496% · gpu 41% · vram 6.0/16 GB");
    expect(s[1].maxLine).toBe("cpu 180% · gpu 62% · vram 3.1/16 GB");
  });

  it("omits the max line when absent and skips missing ops", () => {
    const s = effortSummaries([run("analysis", { total_ms: 100 })]);
    expect(s).toHaveLength(1);
    expect(s[0].maxLine).toBeNull();
  });
});
