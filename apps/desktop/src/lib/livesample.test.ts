import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const cmdMock = vi.fn();
vi.mock("./ipc", () => ({
  cmd: (...args: unknown[]) => cmdMock(...args),
  onEvent: () => () => {},
  initialSong: () => null,
}));

import { actions, workSample, type WorkSample } from "./stores";

beforeEach(() => {
  cmdMock.mockReset();
  cmdMock.mockResolvedValue([]);
  workSample.set(null);
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
