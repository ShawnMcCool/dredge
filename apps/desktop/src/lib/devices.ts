import type { AudioDevice } from "./stores";

/** Name of the device currently flagged as the system default, or null. */
export function defaultName(devices: AudioDevice[]): string | null {
  return devices.find((d) => d.is_default)?.name ?? null;
}

/** Resolve the tuner's effective input id from its selection + the global input. */
export function resolveTunerInput(
  sel: string | "default",
  globalInput: string | null,
  inputs: AudioDevice[],
): string | null {
  if (sel !== "default") return sel;
  if (globalInput) return globalInput;
  const def = inputs.find((d) => d.is_default);
  return def?.id ?? inputs[0]?.id ?? null;
}
