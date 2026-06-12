import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

let nextId = 1;

export interface Resp<T = unknown> {
  id: number;
  ok: boolean;
  data?: T;
  error?: string;
}

export async function cmd<T = unknown>(cmd: string, params: unknown = null): Promise<T> {
  const req = { id: nextId++, cmd, params };
  const resp = (await invoke("dispatch", { req })) as Resp<T>;
  if (!resp.ok) throw new Error(resp.error ?? `command ${cmd} failed`);
  return resp.data as T;
}

/** `EARWORM_OPEN=<song id>` dev affordance — null unless the env var is set. */
export function initialSong(): Promise<number | null> {
  return invoke<number | null>("initial_song");
}

export type EwEvent = { event: string; data: any };

export function onEvent(handler: (e: EwEvent) => void): Promise<() => void> {
  return listen<EwEvent>("earworm://event", (e) => handler(e.payload));
}
