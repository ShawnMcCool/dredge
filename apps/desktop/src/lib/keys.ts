// Keyboard-first: global bindings. Focus-aware by design — a focused editor
// owns the keyboard, and the keymap never acts on a key another handler already
// consumed. Custom keyboard widgets declare themselves with data-keys="capture".

import { get } from "svelte/store";
import { quit } from "./ipc";
import { zoomIn, zoomOut, zoomReset } from "./zoom";
import {
  actions,
  activeLoop,
  BASS_STEM,
  bassFocus,
  currentLoop,
  drillSpan,
  gridSnap,
  openSong,
  position,
  selection,
  settingsOpen,
  workingLoop,
} from "./stores";

/** True when the keyboard belongs to a focused editor — a native field, a
 *  contenteditable, or any custom widget that opts in with data-keys="capture"
 *  (e.g. the tablature grid). Global shortcuts never fire in these. */
function isEditingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable
  ) {
    return true;
  }
  return target.closest('[data-keys="capture"]') !== null;
}

// ── Waveform bar navigation ──────────────────────────────────────────────────
// Left/Right step the playhead one bar at a time. Two things the naive version
// got wrong, fixed here:
//   1. ACCUMULATION. Stepping from the *live* playhead breaks during playback —
//      the playhead creeps forward between presses, so repeated Lefts snap back
//      to the same bar. Instead we step a `pendingTarget` that accumulates while
//      presses stay rapid (within ACCUM_WINDOW); a longer gap resets it to the
//      live playhead so a deliberate new move starts from "here".
//   2. OUR OWN KEY-REPEAT. We ignore the OS auto-repeat (`e.repeat`) and run our
//      own timer on keydown→keyup, so the repeat rate/acceleration is ours to
//      tune (and a future on-screen control can drive the same mechanism).
const ACCUM_WINDOW = 600; // ms: presses within this accumulate; longer resets
const HOLD_DELAY = 300; // ms a key is held before auto-repeat kicks in
const REPEAT_START = 140; // ms: first auto-repeat interval
const REPEAT_MIN = 55; // ms: fastest interval after acceleration
const REPEAT_ACCEL = 12; // ms shaved off each tick

let navDir: -1 | 0 | 1 = 0;
let holdTimer: ReturnType<typeof setTimeout> | null = null;
let repeatTimer: ReturnType<typeof setTimeout> | null = null;
let pendingTarget: number | null = null;
let lastStepAt = 0;

/** The previous/next bar (downbeat) from `from`, falling back to beats, then a
 *  2 s nudge when the song isn't analyzed. Clamped to the song. */
function barTarget(from: number, dir: 1 | -1): number {
  const open = get(openSong);
  if (!open) return from;
  const grid = open.analysis?.downbeats?.length
    ? open.analysis.downbeats
    : (open.analysis?.beats ?? []);
  const eps = 1e-3;
  let target: number | undefined;
  if (grid.length) {
    target =
      dir > 0 ? grid.find((t) => t > from + eps) : [...grid].reverse().find((t) => t < from - eps);
  }
  if (target === undefined) target = from + dir * 2;
  return Math.max(0, Math.min(open.song.duration_secs, target));
}

function stepBar(dir: 1 | -1): void {
  if (!get(openSong)) return;
  const now = performance.now();
  if (pendingTarget === null || now - lastStepAt > ACCUM_WINDOW) {
    pendingTarget = get(position).secs; // fresh move: start from the live playhead
  }
  pendingTarget = barTarget(pendingTarget, dir);
  lastStepAt = now;
  void actions.seek(pendingTarget);
}

function clearNavTimers(): void {
  if (holdTimer !== null) clearTimeout(holdTimer);
  if (repeatTimer !== null) clearTimeout(repeatTimer);
  holdTimer = null;
  repeatTimer = null;
}

/** Begin navigating: one immediate step, then our own accelerating repeat. */
function startNav(dir: 1 | -1): void {
  if (navDir === dir) return; // already running this direction
  clearNavTimers();
  navDir = dir;
  stepBar(dir);
  holdTimer = setTimeout(() => {
    let interval = REPEAT_START;
    const tick = () => {
      if (navDir !== dir) return;
      stepBar(dir);
      interval = Math.max(REPEAT_MIN, interval - REPEAT_ACCEL);
      repeatTimer = setTimeout(tick, interval);
    };
    tick();
  }, HOLD_DELAY);
}

function stopNav(dir?: 1 | -1): void {
  if (dir !== undefined && navDir !== dir) return;
  navDir = 0;
  clearNavTimers();
}

async function handle(e: KeyboardEvent): Promise<void> {
  // Never act on a key another handler already consumed — a modal, the tab
  // editor, anything that called preventDefault as it bubbled up to us.
  if (e.defaultPrevented) return;
  // UI zoom works everywhere, even while typing
  if (e.ctrlKey && !e.metaKey && !e.altKey) {
    if (e.key === "=" || e.key === "+") {
      e.preventDefault();
      await zoomIn();
    } else if (e.key === "-") {
      e.preventDefault();
      await zoomOut();
    } else if (e.key === "0") {
      e.preventDefault();
      await zoomReset();
    } else if (e.key === "[" && !isEditingTarget(e.target)) {
      e.preventDefault();
      await actions.toggleLibrary();
    } else if (e.key === "]" && !isEditingTarget(e.target)) {
      e.preventDefault();
      await actions.togglePanels();
    }
    return;
  }
  if (isEditingTarget(e.target) || e.ctrlKey || e.metaKey || e.altKey) return;

  switch (e.key) {
    case " ": {
      e.preventDefault();
      if (get(position).playing) await actions.pause();
      else await actions.play();
      break;
    }
    case "ArrowRight":
      e.preventDefault();
      if (!e.repeat) startNav(1); // ignore the OS auto-repeat; our timer drives it
      break;
    case "ArrowLeft":
      e.preventDefault();
      if (!e.repeat) startNav(-1);
      break;
    case "r": {
      const l = get(activeLoop);
      if (l) {
        // re-sending the loop bounds jumps the engine to the loop start
        await actions.setTransportLoop(l.start, l.end);
        await actions.seek(l.start);
      } else {
        await actions.seek(0);
      }
      break;
    }
    case "[":
      await actions.setRate(get(position).rate - 0.05);
      break;
    case "]":
      await actions.setRate(get(position).rate + 0.05);
      break;
    case "l": {
      // mirror the waveform's loop glyph: spin up a working loop and drill it
      const sel = get(selection);
      if (sel && get(openSong)) {
        selection.set(null);
        await actions.loopSpan(sel.start, sel.end);
      }
      break;
    }
    case "Escape":
      // a closable Modal may have consumed this Escape already
      if (e.defaultPrevented) break;
      if (get(selection)) selection.set(null);
      break;
    case "q":
      // immediate quit — state is saved as we go, no exit ceremony
      await quit();
      break;
    case "a":
      // one button: analysis then stems, with the progress modal
      if (get(openSong)) await actions.prepare();
      break;
    case ",":
      settingsOpen.set(true);
      break;
    case "b":
      await actions.bassFocus(!get(bassFocus));
      break;
    case "g":
      // loop/selection edges snap to the analyzed grid while on (persisted)
      await actions.setGridSnap(!get(gridSnap));
      break;
    case "d":
      // reveal the tempo trainer for the active loop and arm/disarm it
      if (get(drillSpan)) {
        actions.showDrillTool("trainer");
        await actions.toggleTrainer();
      }
      break;
    case "m":
      // THE one-key move: mute the recorded bass, I play it
      if (get(openSong)?.stems) await actions.toggleStemMute(BASS_STEM);
      break;
    case "Delete":
    case "Backspace": {
      // delete the saved loop, or discard a working one (it was never saved)
      const l = get(currentLoop);
      if (l) await actions.deleteLoop(l.id);
      else if (get(workingLoop)) await actions.clearTransportLoop();
      break;
    }
  }
}

export function installKeys(): () => void {
  const onKeydown = (e: KeyboardEvent) => void handle(e);
  // Arrow release stops our custom repeat. Window blur does too, so a key held
  // as focus leaves can't get stuck repeating.
  const onKeyup = (e: KeyboardEvent) => {
    if (e.key === "ArrowRight") stopNav(1);
    else if (e.key === "ArrowLeft") stopNav(-1);
  };
  const onBlur = () => stopNav();
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("keyup", onKeyup);
  window.addEventListener("blur", onBlur);
  return () => {
    window.removeEventListener("keydown", onKeydown);
    window.removeEventListener("keyup", onKeyup);
    window.removeEventListener("blur", onBlur);
    clearNavTimers();
  };
}
