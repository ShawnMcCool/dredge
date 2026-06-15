import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { cmd } from "./ipc";

const invokeMock = vi.mocked(invoke);

beforeEach(() => {
  invokeMock.mockReset();
});

describe("cmd", () => {
  it("resolves data when the response is ok", async () => {
    invokeMock.mockResolvedValue({ id: 1, ok: true, data: [{ id: 7 }] });
    const out = await cmd("song.list");
    expect(out).toEqual([{ id: 7 }]);
    expect(invokeMock).toHaveBeenCalledWith("dispatch", {
      req: expect.objectContaining({ cmd: "song.list", params: null }),
    });
  });

  it("throws with the server error message on ok:false", async () => {
    invokeMock.mockResolvedValue({ id: 2, ok: false, error: "no song open" });
    await expect(cmd("stems.gains", { gains: [1, 1, 1, 1] })).rejects.toThrow("no song open");
  });

  it("throws a generic message when error is absent", async () => {
    invokeMock.mockResolvedValue({ id: 3, ok: false });
    await expect(cmd("pause")).rejects.toThrow("command pause failed");
  });

  it("increments request ids across calls", async () => {
    invokeMock.mockResolvedValue({ id: 0, ok: true, data: null });
    await cmd("play");
    await cmd("pause");
    const ids = invokeMock.mock.calls.map(([, args]) => (args as { req: { id: number } }).req.id);
    expect(ids[1]).toBe(ids[0] + 1);
  });
});
