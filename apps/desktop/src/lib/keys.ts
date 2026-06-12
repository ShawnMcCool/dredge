// Keyboard-first: global bindings, skipped while typing in a field.

import { get } from "svelte/store";
import {
  actions,
  bassFocusOn,
  currentLoop,
  openSong,
  pendingRatings,
  position,
  selection,
} from "./stores";

export const KEY_HELP =
  "space play/pause · r restart loop · [ ] rate ∓5% · l loop selection · b bass focus · esc clear · 1/2/3 rate miss/shaky/solid";

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
    case "Escape":
      selection.set(null);
      break;
    case "b":
      await actions.bassFocus(!get(bassFocusOn));
      break;
    case "1":
    case "2":
    case "3": {
      if (get(pendingRatings).length > 0) {
        const rating = (["miss", "shaky", "solid"] as const)[Number(e.key) - 1];
        await actions.resolveRating(rating);
      }
      break;
    }
  }
}

export function installKeys(): () => void {
  const listener = (e: KeyboardEvent) => void handle(e);
  window.addEventListener("keydown", listener);
  return () => window.removeEventListener("keydown", listener);
}
