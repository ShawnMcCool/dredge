// The stage flow's gesture brain: per-box collapse (tap the header), header-drag
// reorder, and hide/show over a single wrap-flow container. Simpler than the dock
// coordinator (one container, 1-D reorder, no tabs, no cross-region) but it
// mirrors the dock's drag-feedback shape: a live `caret` ({x,y,h}) and the
// `pointer` position are computed on every move so the view can draw an insertion
// bar and a ghost chip, exactly like `dock-drag`. Set membership changes delegate
// to the pure transforms in `stage.ts`. Provided via context; Box consumes it. A
// missing context yields an inert default so Box still renders standalone.
import { getContext, setContext } from "svelte";
import { hide as hideInFlow, moveBox, show as showInFlow, toggleCollapsed, type BoxId, type FlowRegion } from "./stage";

const KEY = Symbol("stage-flow");
const DRAG_PX = 4;

export interface Caret {
  x: number;
  y: number;
  h: number;
}

export interface StageFlow {
  readonly dragId: string | null;
  /** Live insertion bar (viewport px) while reordering, or null. */
  readonly caret: Caret | null;
  /** Cursor position (viewport px) while reordering, or null — drives the ghost. */
  readonly pointer: { x: number; y: number } | null;
  isCollapsed(id: BoxId): boolean;
  hide(id: BoxId): void;
  show(id: BoxId): void;
  registerContainer(el: HTMLElement): void;
  onHeadDown(e: PointerEvent, id: BoxId): void;
  onHeadMove(e: PointerEvent): void;
  onHeadUp(e?: PointerEvent): void;
  didDrag(): boolean;
}

export function createStageFlow(getFlow: () => FlowRegion, onchange: (flow: FlowRegion) => void): StageFlow {
  let container: HTMLElement | null = null;
  let dragId = $state<string | null>(null);
  let caret = $state<Caret | null>(null);
  let pointer = $state<{ x: number; y: number } | null>(null);
  let downId: BoxId | null = null;
  let downX = 0;
  let downY = 0;
  let didDragFlag = false;
  let pendingIndex: number | null = null;

  // The drop the pointer is currently over: the index to splice the dragged box
  // into (computed among the OTHER boxes, which is what `moveBox` expects) plus
  // the insertion-bar rect. Used live for the caret AND on drop to apply — one
  // computation, so the bar always shows exactly where the box will land.
  function dropAt(e: PointerEvent): { toIndex: number; caret: Caret } | null {
    if (!container || dragId === null) return null;
    const el = document.elementFromPoint(e.clientX, e.clientY);
    const overBox = el?.closest<HTMLElement>(".box");
    if (!overBox || !container.contains(overBox)) return null;
    const targetId = overBox.dataset.box as BoxId | undefined;
    if (!targetId || targetId === dragId) return null;
    const r = overBox.getBoundingClientRect();
    const after = e.clientX > r.left + r.width / 2;
    const others = getFlow().order.filter((x) => x !== dragId);
    const toIndex = others.indexOf(targetId) + (after ? 1 : 0);
    return { toIndex, caret: { x: after ? r.right + 1 : r.left - 3, y: r.top, h: r.height } };
  }

  return {
    get dragId() {
      return dragId;
    },
    get caret() {
      return caret;
    },
    get pointer() {
      return pointer;
    },
    isCollapsed(id) {
      return getFlow().collapsed.includes(id);
    },
    hide(id) {
      onchange(hideInFlow(getFlow(), id));
    },
    show(id) {
      onchange(showInFlow(getFlow(), id));
    },
    registerContainer(el) {
      container = el;
    },
    didDrag() {
      return didDragFlag;
    },
    onHeadDown(e, id) {
      if (e.button !== 0) return;
      // A press that starts on a button (tools / hide-×) is that button's click,
      // not a box gesture — leave it alone so the button fires normally.
      if ((e.target as HTMLElement).closest("button")) return;
      downId = id;
      downX = e.clientX;
      downY = e.clientY;
      didDragFlag = false;
      // NOTE: do NOT capture here — capturing on pointerdown would redirect the
      // click to the header. Capture lazily once a real drag starts, so a plain
      // tap stays a tap (→ collapse).
    },
    onHeadMove(e) {
      if (downId === null) return;
      if (dragId === null) {
        if (Math.abs(e.clientX - downX) < DRAG_PX && Math.abs(e.clientY - downY) < DRAG_PX) return;
        dragId = downId;
        didDragFlag = true;
        try {
          (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        } catch {
          /* non-fatal */
        }
      }
      pointer = { x: e.clientX, y: e.clientY };
      // Track the drop target for the live bar; DON'T mutate order here — applying
      // mid-drag makes a hovered target oscillate as each move re-reads the
      // just-changed order. The order rewrites once, on drop.
      const d = dropAt(e);
      pendingIndex = d ? d.toIndex : null;
      caret = d ? d.caret : null;
    },
    onHeadUp(e) {
      if (e && dragId !== null) {
        if (pendingIndex !== null) onchange({ ...getFlow(), order: moveBox(getFlow().order, dragId as BoxId, pendingIndex) });
      } else if (e && dragId === null && downId !== null && !didDragFlag) {
        // a tap on the header background (not a button, no drag) → toggle collapse
        onchange(toggleCollapsed(getFlow(), downId));
      }
      dragId = null;
      downId = null;
      pendingIndex = null;
      caret = null;
      pointer = null;
    },
  };
}

export function setStageFlow(s: StageFlow) {
  setContext(KEY, s);
}
export function getStageFlow(): StageFlow {
  return (
    getContext<StageFlow>(KEY) ?? {
      dragId: null,
      caret: null,
      pointer: null,
      isCollapsed: () => false,
      hide: () => {},
      show: () => {},
      registerContainer: () => {},
      onHeadDown: () => {},
      onHeadMove: () => {},
      onHeadUp: () => {},
      didDrag: () => false,
    }
  );
}
