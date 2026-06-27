// The stage flow's gesture brain: per-box collapse and header-drag reorder over
// a single wrap-flow container. Simpler than the dock coordinator (one container,
// 1-D reorder, no tabs, no cross-region). Provided via context; Box consumes it.
// A missing context yields an inert default so Box still renders standalone.
import { getContext, setContext } from "svelte";
import { moveBox, type BoxId, type FlowRegion } from "./stage";

const KEY = Symbol("stage-flow");
const DRAG_PX = 4;

export interface StageFlow {
  readonly dragId: string | null;
  isCollapsed(id: BoxId): boolean;
  toggle(id: BoxId): void;
  registerContainer(el: HTMLElement): void;
  onHeadDown(e: PointerEvent, id: BoxId): void;
  onHeadMove(e: PointerEvent): void;
  onHeadUp(e?: PointerEvent): void;
  didDrag(): boolean;
}

export function createStageFlow(getFlow: () => FlowRegion, onchange: (flow: FlowRegion) => void): StageFlow {
  let container: HTMLElement | null = null;
  let dragId = $state<string | null>(null);
  let downId: BoxId | null = null;
  let downX = 0;
  let downY = 0;
  let didDragFlag = false;

  return {
    get dragId() {
      return dragId;
    },
    isCollapsed(id) {
      return getFlow().collapsed.includes(id);
    },
    toggle(id) {
      const f = getFlow();
      const has = f.collapsed.includes(id);
      onchange({ ...f, collapsed: has ? f.collapsed.filter((x) => x !== id) : [...f.collapsed, id] });
    },
    registerContainer(el) {
      container = el;
    },
    didDrag() {
      return didDragFlag;
    },
    onHeadDown(e, id) {
      if (e.button !== 0) return;
      downId = id;
      downX = e.clientX;
      downY = e.clientY;
      didDragFlag = false;
      // NOTE: do NOT capture here — capturing on pointerdown redirects the click
      // to the header and steals it from the caret/tools buttons. Capture lazily
      // once a real drag starts (below), so a plain click stays a click.
    },
    onHeadMove(e) {
      if (downId === null) return;
      if (dragId === null) {
        if (Math.abs(e.clientX - downX) < DRAG_PX && Math.abs(e.clientY - downY) < DRAG_PX) return;
        dragId = downId;
        didDragFlag = true;
        // a real drag began — now capture so moves keep flowing off the header
        try {
          (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        } catch {
          /* non-fatal */
        }
      }
      // No mutation during the drag — only the dragging cue (dragId) tracks. The
      // reorder is computed and applied once, on drop. Live-mutating here makes a
      // hovered target oscillate (each move re-evaluates the just-changed order).
    },
    onHeadUp(e) {
      if (dragId !== null && e && container) {
        const el = document.elementFromPoint(e.clientX, e.clientY);
        const overBox = el?.closest<HTMLElement>(".box");
        const targetId = overBox?.dataset.box as BoxId | undefined;
        if (overBox && container.contains(overBox) && targetId && targetId !== dragId) {
          // insert before/after the target by which side of its centre we dropped
          // on (boxes are wider than tall, so x reads as reading order in a row).
          // The index is computed among the OTHER boxes — exactly what `moveBox`
          // splices into after it removes the dragged id.
          const r = overBox.getBoundingClientRect();
          const order = getFlow().order;
          const others = order.filter((x) => x !== dragId);
          let toIndex = others.indexOf(targetId);
          if (e.clientX > r.left + r.width / 2) toIndex += 1;
          onchange({ ...getFlow(), order: moveBox(order, dragId as BoxId, toIndex) });
        }
      }
      dragId = null;
      downId = null;
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
      isCollapsed: () => false,
      toggle: () => {},
      registerContainer: () => {},
      onHeadDown: () => {},
      onHeadMove: () => {},
      onHeadUp: () => {},
      didDrag: () => false,
    }
  );
}
