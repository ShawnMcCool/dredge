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
  onHeadUp(): void;
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
      try {
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      } catch {
        /* non-fatal */
      }
    },
    onHeadMove(e) {
      if (downId === null) return;
      if (dragId === null) {
        if (Math.abs(e.clientX - downX) < DRAG_PX && Math.abs(e.clientY - downY) < DRAG_PX) return;
        dragId = downId;
        didDragFlag = true;
      }
      if (!container) return;
      const el = document.elementFromPoint(e.clientX, e.clientY);
      const overBox = el?.closest<HTMLElement>(".box");
      if (!overBox || !container.contains(overBox)) return;
      const targetId = overBox.dataset.box as BoxId | undefined;
      if (!targetId || targetId === dragId) return;
      // insert before/after the target depending on which side of its centre the
      // pointer is on (the flow wraps, so use the larger axis — boxes are wider
      // than tall, so x reads as reading order within a row).
      const r = overBox.getBoundingClientRect();
      const after = e.clientX > r.left + r.width / 2;
      const order = getFlow().order;
      let toIndex = order.indexOf(targetId);
      if (after) toIndex += 1;
      onchange({ ...getFlow(), order: moveBox(order, dragId as BoxId, toIndex) });
    },
    onHeadUp() {
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
