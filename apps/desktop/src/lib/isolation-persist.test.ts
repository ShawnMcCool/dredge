import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const cmdMock = vi.fn();
vi.mock("./ipc", () => ({
  cmd: (...args: unknown[]) => cmdMock(...args),
  onEvent: () => () => {},
  initialSong: () => null,
}));

import { actions, openSong, stemMix, bassFocus } from "./stores";

beforeEach(() => {
  vi.useFakeTimers();
  cmdMock.mockReset();
  cmdMock.mockResolvedValue(null);
  openSong.set(null);
  bassFocus.set(false);
});

afterEach(() => {
  vi.useRealTimers();
});

const isolationSets = () => cmdMock.mock.calls.filter((c) => c[0] === "isolation.set");

describe("persistIsolation", () => {
  it("debounces and saves the live isolation state for the open song", async () => {
    openSong.set({ song: { id: 42 }, stems: false } as never);

    await actions.toggleStemMute(1);
    // nothing written yet — still inside the debounce window
    expect(isolationSets()).toHaveLength(0);

    vi.advanceTimersByTime(350);

    const calls = isolationSets();
    expect(calls).toHaveLength(1);
    const [, payload] = calls[0] as [string, Record<string, unknown>];
    expect(payload.song_id).toBe(42);
    expect(payload.mutes).toEqual([false, true, false, false, false, false]);
    expect(payload.bass_focus).toBe(false);
  });

  it("collapses a burst of edits into a single write", async () => {
    openSong.set({ song: { id: 7 }, stems: false } as never);

    await actions.setStemLevel(0, 90);
    await actions.setStemLevel(0, 80);
    await actions.setStemLevel(0, 70);
    vi.advanceTimersByTime(350);

    expect(isolationSets()).toHaveLength(1);
    const [, payload] = isolationSets()[0] as [string, Record<string, unknown>];
    expect((payload.levels as number[])[0]).toBe(70);
  });

  it("does not save when the stem mix is set directly (e.g. routine playback)", () => {
    openSong.set({ song: { id: 9 }, stems: false } as never);

    // applyRoutineMix writes the store directly, never through the actions —
    // so no persist is scheduled.
    stemMix.set({ levels: [50, 50, 50, 50, 50, 50], mutes: Array(6).fill(false), solos: Array(6).fill(false) });
    vi.advanceTimersByTime(1000);

    expect(isolationSets()).toHaveLength(0);
  });

  it("is a no-op with no song open", () => {
    actions.persistIsolation();
    vi.advanceTimersByTime(1000);
    expect(isolationSets()).toHaveLength(0);
  });
});
