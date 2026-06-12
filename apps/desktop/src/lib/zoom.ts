// UI zoom via the webview's zoom level — for displays where the compositor
// scale is 1 (e.g. 4K at Hyprland scale 1.00) and the app must size itself.

import { getCurrentWebview } from "@tauri-apps/api/webview";

const STORAGE_KEY = "earworm-zoom";
const MIN = 0.5;
const MAX = 3.0;
const STEP = 0.125;

let current = 1.0;

function firstRunDefault(): number {
  // compositor scale 1 on a 4K-wide screen → content is tiny without help
  return window.screen.width >= 3000 ? 1.75 : 1.0;
}

async function apply(z: number): Promise<void> {
  current = Math.min(MAX, Math.max(MIN, z));
  await getCurrentWebview().setZoom(current);
  localStorage.setItem(STORAGE_KEY, String(current));
}

export async function initZoom(): Promise<void> {
  const saved = Number(localStorage.getItem(STORAGE_KEY));
  await apply(Number.isFinite(saved) && saved >= MIN && saved <= MAX ? saved : firstRunDefault());
}

export const zoomIn = (): Promise<void> => apply(current + STEP);
export const zoomOut = (): Promise<void> => apply(current - STEP);
export const zoomReset = (): Promise<void> => apply(1.0);
