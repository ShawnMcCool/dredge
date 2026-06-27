// The stage flow region: an ordered list of always-visible control boxes plus a
// set of collapsed ones. Pure transforms, colocated tests — the dock's `dock.ts`
// analogue for the flow arrangement. Presence (which boxes exist right now) is a
// state-driven render concern handled by App; this layer only owns order +
// collapse over the known box set.

/** Every stage control box, in canonical (default) order. */
export const STAGE_BOXES = ["metronome", "isolation", "click", "notes", "recordings", "tuner", "drill"] as const;
export type BoxId = (typeof STAGE_BOXES)[number];

export interface FlowRegion {
  order: BoxId[];
  collapsed: BoxId[]; // a set, stored as an array for JSON
}

/** First-run shape: canonical order, nothing collapsed. */
export function defaultFlow(): FlowRegion {
  return { order: [...STAGE_BOXES], collapsed: [] };
}

/** Reconcile a stored flow against the known box set: keep the first occurrence
 *  of each known id in stored order, drop unknown ids, append boxes new-in-code,
 *  prune collapsed entries to the known set. Empty/garbage → default order. */
export function reconcileFlow(
  flow: { order?: unknown; collapsed?: unknown },
  allBoxes: readonly string[],
): FlowRegion {
  const known = new Set(allBoxes);
  const seen = new Set<string>();
  const order: BoxId[] = [];
  for (const id of Array.isArray(flow?.order) ? flow.order : []) {
    if (typeof id === "string" && known.has(id) && !seen.has(id)) {
      seen.add(id);
      order.push(id as BoxId);
    }
  }
  for (const id of allBoxes) if (!seen.has(id)) order.push(id as BoxId);
  const collapsedSrc = Array.isArray(flow?.collapsed) ? flow.collapsed : [];
  const collapsed = [
    ...new Set(collapsedSrc.filter((id): id is BoxId => typeof id === "string" && known.has(id))),
  ];
  return { order, collapsed };
}

/** Move `id` to position `toIndex` in the order (no-op for an unknown id). */
export function moveBox(order: BoxId[], id: BoxId, toIndex: number): BoxId[] {
  const from = order.indexOf(id);
  if (from === -1) return order;
  const next = order.slice();
  next.splice(from, 1);
  next.splice(Math.max(0, Math.min(toIndex, next.length)), 0, id);
  return next;
}

/** Add `id` to the collapsed set if absent, remove it if present. */
export function toggleCollapsed(flow: FlowRegion, id: BoxId): FlowRegion {
  const has = flow.collapsed.includes(id);
  return { ...flow, collapsed: has ? flow.collapsed.filter((x) => x !== id) : [...flow.collapsed, id] };
}
