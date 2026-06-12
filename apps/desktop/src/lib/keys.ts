// Keyboard-first: global bindings, skipped while typing in a field.

import { get } from "svelte/store";
import { zoomIn, zoomOut, zoomReset } from "./zoom";
import {
  actions,
  BASS_STEM,
  bassFocusOn,
  currentLoop,
  gridSnap,
  openSong,
  pendingRatings,
  position,
  quickPromptVisible,
  selection,
} from "./stores";

export const KEY_HELP =
  "space play/pause · r restart loop · [ ] rate ∓5% · l loop selection · p quick practice · b bass focus · m mute bass stem · g grid snap · esc clear · 1/2/3 rate miss/shaky/solid · ctrl ± 0 zoom";

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable
  );
}

function autoLoopName(): string {
  const open = get(openSong);
  const n = (open?.loops.filter((l) => l.kind.kind === "manual").length ?? 0) + 1;
  return `loop ${n}`;
}

async function handle(e: KeyboardEvent): Promise<void> {
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
    }
    return;
  }
  if (isTyping(e.target) || e.ctrlKey || e.metaKey || e.altKey) return;

  switch (e.key) {
    case " ": {
      e.preventDefault();
      if (get(position).playing) await actions.pause();
      else await actions.play();
      break;
    }
    case "r": {
      const l = get(currentLoop);
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
      const sel = get(selection);
      if (sel && get(openSong)) {
        const l = await actions.createLoop(autoLoopName(), sel.start, sel.end);
        await actions.selectLoop(l);
        selection.set(null);
      }
      break;
    }
    case "p": {
      // zero-ceremony practice: selection → instant micro-session
      const sel = get(selection);
      if (sel && get(openSong)) await actions.quickPractice(sel.start, sel.end);
      break;
    }
    case "Escape":
      if (get(quickPromptVisible)) await actions.quickDiscard();
      else selection.set(null);
      break;
    case "b":
      await actions.bassFocus(!get(bassFocusOn));
      break;
    case "g":
      // loop/selection edges snap to analyzed downbeats while on
      gridSnap.update((on) => !on);
      break;
    case "m":
      // THE one-key move: mute the recorded bass, I play it
      if (get(openSong)?.stems) await actions.toggleStemMute(BASS_STEM);
      break;
    case "1":
    case "2":
    case "3": {
      const rating = (["miss", "shaky", "solid"] as const)[Number(e.key) - 1];
      if (get(quickPromptVisible)) await actions.quickRate(rating);
      else if (get(pendingRatings).length > 0) await actions.resolveRating(rating);
      break;
    }
  }
}

export function installKeys(): () => void {
  const listener = (e: KeyboardEvent) => void handle(e);
  window.addEventListener("keydown", listener);
  return () => window.removeEventListener("keydown", listener);
}
