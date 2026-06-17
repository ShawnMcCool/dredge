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
  const listener = (e: KeyboardEvent) => void handle(e);
  window.addEventListener("keydown", listener);
  return () => window.removeEventListener("keydown", listener);
}
