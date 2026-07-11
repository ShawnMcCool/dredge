// UI zoom via the webview's zoom level — for displays where the compositor
// scale is 1 (e.g. 4K at Hyprland scale 1.00) and the app must size itself.
// The durable `ui_scale` setting is authoritative; localStorage only feeds a
// one-time migration from the pre-settings era.

import { getCurrentWebview } from "@tauri-apps/api/webview";
import { get } from "svelte/store";
import { actions, settings, UI_SCALE } from "./stores";

const LEGACY_STORAGE_KEY = "dredge-zoom";
const MIN = 0.5;
const MAX = 3.0;
const STEP = 0.125;

let current = 1.0;

function firstRunDefault(): number {
  // compositor scale 1 on a 4K-wide screen → content is tiny without help
  return window.screen.width >= 3000 ? 1.75 : 1.0;
}

const valid = (z: number): boolean => Number.isFinite(z) && z >= MIN && z <= MAX;

async function apply(z: number): Promise<void> {
  current = Math.min(MAX, Math.max(MIN, z));
  await getCurrentWebview().setZoom(current);
  await actions.setSetting(UI_SCALE, current);
}

/** Call after `loadSettings()` — reads `ui_scale` from the mirror. */
export async function initZoom(): Promise<void> {
  const saved = Number(get(settings)[UI_SCALE]);
  if (valid(saved)) {
    current = saved;
    await getCurrentWebview().setZoom(current);
    // the initial zoom lands while the webview surface is still settling,
    // which can bake in the render/hit-test desync with no later trigger to
    // heal it (no resize happens after launch) — force one resync once the
    // surface has settled
    setTimeout(() => void resyncZoom(), 1000);
    return;
  }
  // one-time migration: adopt the old localStorage zoom. The key stays put
  // (it's ignored from now on): localStorage is shared across DREDGE_DB
  // profiles, so deleting it here would rob another profile's migration.
  const legacy = Number(localStorage.getItem(LEGACY_STORAGE_KEY));
  await apply(valid(legacy) ? legacy : firstRunDefault());
}

/** Force the webview to re-apply the current zoom without persisting.
 *  WebKitGTK can desync the render scale from the input/hit-test scale (across
 *  resizes/fullscreen, but also without any resize), which drifts click targets
 *  — worse the further they sit from the top-left. Crucially, WebKitGTK
 *  IGNORES set_zoom_level calls whose value is unchanged, so re-asserting
 *  `current` alone never reaches the engine: nudge by an imperceptible epsilon
 *  first so the restore is a real change that resyncs the two scales. */
export async function resyncZoom(): Promise<void> {
  const wv = getCurrentWebview();
  // fire the nudge and restore back-to-back WITHOUT awaiting the nudge: both
  // land in webkit's main loop before it paints, so the intermediate zoom is
  // never composited (awaiting the first call flashed the whole page twice)
  void wv.setZoom(current + 0.0001);
  await wv.setZoom(current);
}

export const zoomIn = (): Promise<void> => apply(current + STEP);
export const zoomOut = (): Promise<void> => apply(current - STEP);
export const zoomReset = (): Promise<void> => apply(1.0);
/** Settings-modal fader: live-applied and persisted like ctrl±. */
export const setZoom = (z: number): Promise<void> => apply(z);
export const getZoom = (): number => current;
