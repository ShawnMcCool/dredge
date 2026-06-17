import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTracing, trace, traceErr } from "./trace";

let nextId = 1;

export interface Resp<T = unknown> {
  id: number;
  ok: boolean;
  data?: T;
  error?: string;
}

export async function cmd<T = unknown>(cmd: string, params: unknown = null): Promise<T> {
  const id = nextId++;
  const req = { id, cmd, params };
  // Fast path when telemetry is off — no timers, no timing.
  if (!isTracing()) {
    const resp = (await invoke("dispatch", { req })) as Resp<T>;
    if (!resp.ok) throw new Error(resp.error ?? `command ${cmd} failed`);
    return resp.data as T;
  }
  // Watchdogs (DREDGE_DEBUG): a dispatch that never resolves (frozen invoke /
  // wedged backend) is the prime suspect for the stuck-spinner bug — surface it
  // instead of hanging silently. These timers only fire if the main thread is
  // still alive.
  const w3 = setTimeout(() => traceErr("ipc", `${cmd} #${id} still pending after 3s`), 3000);
  const w10 = setTimeout(() => traceErr("ipc", `${cmd} #${id} STILL pending after 10s — likely hung`), 10000);
  const start = performance.now();
  try {
    const resp = (await invoke("dispatch", { req })) as Resp<T>;
    const dt = Math.round(performance.now() - start);
    if (dt > 800) trace("ipc", `${cmd} #${id} slow: ${dt}ms`);
    if (!resp.ok) throw new Error(resp.error ?? `command ${cmd} failed`);
    return resp.data as T;
  } finally {
    clearTimeout(w3);
    clearTimeout(w10);
  }
}

/** `DREDGE_OPEN=<song id>` dev affordance — null unless the env var is set. */
export function initialSong(): Promise<number | null> {
  return invoke<number | null>("initial_song");
}

/** Confirmed exit — the host process terminates. */
export function quit(): Promise<void> {
  return invoke("quit");
}

export type EwEvent = { event: string; data: any };

export function onEvent(handler: (e: EwEvent) => void): Promise<() => void> {
  return listen<EwEvent>("dredge://event", (e) => handler(e.payload));
}
