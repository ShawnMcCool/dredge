import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const cmdMock = vi.fn();
vi.mock("./ipc", () => ({
  cmd: (...args: unknown[]) => cmdMock(...args),
  onEvent: () => () => {},
  initialSong: () => null,
}));

import { actions, workSample, vram, type WorkSample } from "./stores";

beforeEach(() => {
  cmdMock.mockReset();
  cmdMock.mockResolvedValue([]);
  workSample.set(null);
  vram.set(null);
});

const sample = (): WorkSample => ({
  op: "analysis",
  stage: "CPU recovery",
  elapsed_ms: 102000,
  cpu_pct: 483,
  gpu_util: 38,
  gpu_mem_used_mb: 5120,
  gpu_mem_total_mb: 16376,
});

describe("recordWorkSample", () => {
  it("stores the latest sample", () => {
    actions.recordWorkSample(sample());
    expect(get(workSample)?.stage).toBe("CPU recovery");
    expect(get(workSample)?.cpu_pct).toBe(483);
  });
});

describe("vram accumulation", () => {
  const s = (used: number, total = 16000): WorkSample => ({
    op: "analysis", stage: "GPU attempt", elapsed_ms: 1000, cpu_pct: 100,
    gpu_util: 40, gpu_mem_used_mb: used, gpu_mem_total_mb: total,
  });

  it("accumulates used, total, and peak", () => {
    actions.recordWorkSample(s(4000));
    actions.recordWorkSample(s(6000));
    actions.recordWorkSample(s(5000));
    const v = get(vram)!;
    expect(v.used).toEqual([4000, 6000, 5000]);
    expect(v.total).toBe(16000);
    expect(v.peak).toBe(6000);
  });

  it("keeps only the last 60 samples but peak persists after the window slides", () => {
    for (let i = 1; i <= 70; i++) actions.recordWorkSample(s(i * 100)); // 100..7000
    const v = get(vram)!;
    expect(v.used).toHaveLength(60);
    expect(v.used[0]).toBe(1100); // sample #11 (first 10 slid off)
    expect(v.used[v.used.length - 1]).toBe(7000);
    expect(v.peak).toBe(7000); // high-water mark survives the slide
  });

  it("ignores samples with no GPU memory", () => {
    const noGpu: WorkSample = { op: "analysis", stage: "x", elapsed_ms: 1, cpu_pct: 1 };
    actions.recordWorkSample(noGpu);
    expect(get(vram)).toBeNull();
  });
});
