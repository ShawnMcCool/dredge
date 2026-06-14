// Native window frame (decorations) toggle. Like zoom.ts this talks straight to
// Tauri rather than the dispatch — it's a shell concern, not app state. The
// durable `window_decorations` setting is authoritative; default is on (matches
// Tauri's default), so only an explicit `false` turns the frame off.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { get } from "svelte/store";
import { settings, WINDOW_DECORATIONS } from "./stores";

/** Show/hide the native title bar + min/max/close buttons. */
export async function applyDecorations(on: boolean): Promise<void> {
  await getCurrentWindow().setDecorations(on);
}

/** Call after `loadSettings()` — restore the saved frame preference. */
export async function initDecorations(): Promise<void> {
  await applyDecorations(get(settings)[WINDOW_DECORATIONS] !== false);
}
