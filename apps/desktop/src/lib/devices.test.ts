import { describe, expect, it } from "vitest";
import { defaultName } from "./devices";
import type { AudioDevice } from "./stores";

describe("defaultName", () => {
  it("returns the name of the flagged default device", () => {
    const devices: AudioDevice[] = [
      { id: "a", name: "Headphones", is_default: false },
      { id: "b", name: "HDMI Output", is_default: true },
    ];
    expect(defaultName(devices)).toBe("HDMI Output");
  });

  it("returns null when no device is flagged as default", () => {
    const devices: AudioDevice[] = [
      { id: "a", name: "Headphones", is_default: false },
      { id: "b", name: "HDMI Output", is_default: false },
    ];
    expect(defaultName(devices)).toBeNull();
  });

  it("returns null for an empty list", () => {
    expect(defaultName([])).toBeNull();
  });
});
