import { describe, expect, it } from "vitest";
import { defaultName, resolveTunerInput } from "./devices";
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

describe("resolveTunerInput", () => {
  const inputs: AudioDevice[] = [
    { id: "a", name: "Mic", is_default: false },
    { id: "b", name: "Interface", is_default: true },
  ];

  it("returns the explicit override when one is set", () => {
    expect(resolveTunerInput("a", "b", inputs)).toBe("a");
  });

  it('"default" with a global input returns the global', () => {
    expect(resolveTunerInput("default", "a", inputs)).toBe("a");
  });

  it('"default" with no global returns the is_default device', () => {
    expect(resolveTunerInput("default", null, inputs)).toBe("b");
  });

  it('"default" with no global and none flagged returns the first input', () => {
    const none: AudioDevice[] = [
      { id: "a", name: "Mic", is_default: false },
      { id: "b", name: "Interface", is_default: false },
    ];
    expect(resolveTunerInput("default", null, none)).toBe("a");
  });

  it("returns null for empty inputs with no global", () => {
    expect(resolveTunerInput("default", null, [])).toBeNull();
  });
});
