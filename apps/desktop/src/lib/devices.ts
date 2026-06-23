import type { AudioDevice } from "./stores";

/** Name of the device currently flagged as the system default, or null. */
export function defaultName(devices: AudioDevice[]): string | null {
  return devices.find((d) => d.is_default)?.name ?? null;
}
