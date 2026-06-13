# Campaign: select a loop on the waveform, resize it with edge handles, delete with Delete

Status: backlog
Raised: 2026-06-13

## Idea

Direct loop editing on the waveform:
1. **Click a loop** region on the waveform to **select** it.
2. When selected, show draggable **handles** on its left and right edges; drag
   each handle to move that boundary (resize the loop).
3. Press the **Delete** key while a loop is selected to delete it.

## Context

- Loops are drawn in `apps/desktop/src/components/Waveform.svelte` `draw()`
  (~line 177-192): a translucent fill plus two vertical accent edge lines per
  loop (junction = dashed, manual = solid).
- `currentLoop` (store) is the loop the transport is pointed at; `actions.selectLoop(l)`
  sets it. This is the natural "selected loop" — clicking a loop body selects it
  (already partly wired); the handles + delete operate on `currentLoop`.
- A resize needs a `loop.update`-style command (server) to persist new
  start/end; check what exists (`loop.update` / `loop.create` in `server::app`),
  and `actions` in `lib/stores.ts` (`selectLoop`, `deleteLoop`).
- `keys.ts` owns key handling; bind `Delete`/`Backspace` → delete `currentLoop`
  (guard so it doesn't fire while typing, and decide precedence vs `esc`).

## Likely shape (frontend + maybe a server command)

- Hit-testing in `Waveform.svelte`: pointer-down near a loop edge (within N px of
  `x0`/`x1`) grabs that handle; pointer-move updates the loop's start/end (snap to
  downbeats when grid-snap on); pointer-up commits via `loop.update`. Pointer-down
  on a loop body (not near an edge) selects it. Render handles (small grips) on
  the selected loop's edges.
- `Delete` key when `currentLoop` set → `actions.deleteLoop(currentLoop.id)`.

## Next step

Brainstorm → spec → plan → build. Decide: does selecting a loop also point the
transport at it (reuse `currentLoop`) or a separate "edit selection"? Confirm a
`loop.update` command exists for resize (add if not). Handle visual affordance +
the Delete keybinding precedence.
