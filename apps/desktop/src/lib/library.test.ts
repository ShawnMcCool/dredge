import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const cmdMock = vi.fn();
vi.mock("./ipc", () => ({
  cmd: (...args: unknown[]) => cmdMock(...args),
  onEvent: () => () => {},
  initialSong: () => null,
}));

import { actions, openSong } from "./stores";

beforeEach(() => {
  cmdMock.mockReset();
  cmdMock.mockResolvedValue(null);
  openSong.set(null);
});

describe("deleteSong", () => {
  it("sends song.delete, clears the open song, and refreshes the list", async () => {
    openSong.set({ song: { id: 5 } } as never);
    cmdMock.mockImplementation((name: string) =>
      name === "song.list" ? Promise.resolve([]) : Promise.resolve(null),
    );

    await actions.deleteSong(5);

    expect(cmdMock).toHaveBeenCalledWith("song.delete", { song_id: 5 });
    expect(get(openSong)).toBeNull();
    expect(cmdMock).toHaveBeenCalledWith("song.list");
  });

  it("leaves a different open song in place", async () => {
    openSong.set({ song: { id: 9 } } as never);
    cmdMock.mockImplementation((name: string) =>
      name === "song.list" ? Promise.resolve([]) : Promise.resolve(null),
    );

    await actions.deleteSong(5);

    expect(get(openSong)).not.toBeNull();
  });
});

describe("updateSong", () => {
  it("sends song.update and patches the open song's metadata", async () => {
    openSong.set({ song: { id: 5, title: "old", artist: null } } as never);
    cmdMock.mockImplementation((name: string) => {
      if (name === "song.update")
        return Promise.resolve({ id: 5, title: "new", artist: "B" });
      if (name === "song.list") return Promise.resolve([]);
      return Promise.resolve(null);
    });

    await actions.updateSong(5, "new", "B");

    expect(cmdMock).toHaveBeenCalledWith("song.update", {
      song_id: 5,
      title: "new",
      artist: "B",
    });
    expect(get(openSong)?.song.title).toBe("new");
  });
});

describe("reanalyze", () => {
  it("sends analysis.run with force for the open song", async () => {
    openSong.set({ song: { id: 7 } } as never);

    await actions.reanalyze();

    expect(cmdMock).toHaveBeenCalledWith("analysis.run", {
      song_id: 7,
      force: true,
    });
  });

  it("no-ops when nothing is open", async () => {
    await actions.reanalyze();
    expect(cmdMock).not.toHaveBeenCalled();
  });
});
