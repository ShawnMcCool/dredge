import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const cmdMock = vi.fn();
vi.mock("./ipc", () => ({
  cmd: (...args: unknown[]) => cmdMock(...args),
  onEvent: () => () => {},
  initialSong: () => null,
}));

import { actions, profiles, type ProfileRun } from "./stores";

beforeEach(() => {
  cmdMock.mockReset();
  cmdMock.mockResolvedValue([]);
  profiles.set([]);
});

const run = (op: string): ProfileRun => ({
  op,
  total_ms: 1500,
  ok: true,
  device: "cpu",
  engine: "songformer",
  started_at: "2026-06-13 10:00:00",
  stages: [{ name: "analyze", ms: 1500 }],
});

describe("loadProfiles", () => {
  it("fetches profiles.list and fills the store", async () => {
    cmdMock.mockResolvedValue([run("analysis")]);
    await actions.loadProfiles();
    expect(cmdMock).toHaveBeenCalledWith("profiles.list", { limit: 50 });
    expect(get(profiles)).toHaveLength(1);
    expect(get(profiles)[0].op).toBe("analysis");
  });
});

describe("recordProfile", () => {
  it("prepends a run (most-recent-first)", () => {
    profiles.set([run("stems")]);
    actions.recordProfile(run("analysis"));
    const list = get(profiles);
    expect(list).toHaveLength(2);
    expect(list[0].op).toBe("analysis");
    expect(list[1].op).toBe("stems");
  });
});
