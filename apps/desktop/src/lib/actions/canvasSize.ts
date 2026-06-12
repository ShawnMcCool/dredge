// Reusable element-size tracking for canvas hosts: observes the node with a
// ResizeObserver and reports CSS-pixel size plus devicePixelRatio, firing
// once immediately so the canvas is sized before first paint.
import type { Action } from "svelte/action";

export type CanvasSizeCallback = (width: number, height: number, dpr: number) => void;

export const canvasSize: Action<HTMLElement, CanvasSizeCallback> = (node, callback) => {
  let cb = callback;
  const fire = () => cb(node.clientWidth, node.clientHeight, window.devicePixelRatio || 1);
  fire();
  const ro = new ResizeObserver(fire);
  ro.observe(node);
  return {
    update(next: CanvasSizeCallback) {
      cb = next;
      fire();
    },
    destroy() {
      ro.disconnect();
    },
  };
};
