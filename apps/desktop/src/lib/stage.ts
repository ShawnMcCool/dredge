// The stage flow region: an ordered list of always-visible control boxes plus a
// set of collapsed ones. Pure transforms, colocated tests — the dock's `dock.ts`
// analogue for the flow arrangement. Presence (which boxes exist right now) is a
// state-driven render concern handled by App; this layer only owns order +
// collapse over the known box set.

/** Every stage control box, in canonical (default) order. */
export const STAGE_BOXES = ["metronome", "isolation", "click", "notes", "recordings", "tuner", "drill"] as const;
export type BoxId = (typeof STAGE_BOXES)[number];

/** A box's canonical display name — the single source for the header label, the
 *  restore menu (which lists boxes that are hidden, hence unmounted, so their
 *  names can't come from a mounted `<Box>`), and the drag ghost chip. A box may
 *  still pass a richer header override (e.g. notes appends its section). */
export const BOX_LABELS: Record<BoxId, string> = {
  metronome: "metronome",
  isolation: "isolation",
  click: "click",
  notes: "notes",
  recordings: "recordings",
  tuner: "tuner",
  drill: "drill",
};

export interface FlowRegion {
  order: BoxId[];
  // Two orthogonal dispositions, each a set stored as an array for JSON:
  collapsed: BoxId[]; // minimized to the header strip while on the stage
  hidden: BoxId[]; // removed from the stage entirely (restorable from the + tool menu)
}

/** First-run shape: canonical order, nothing collapsed, nothing hidden. */
export function defaultFlow(): FlowRegion {
  return { order: [...STAGE_BOXES], collapsed: [], hidden: [] };
}

/** Restrict an untrusted id list to the known box set, de-duped. */
function knownSet(src: unknown, known: Set<string>): BoxId[] {
  const list = Array.isArray(src) ? src : [];
  return [...new Set(list.filter((id): id is BoxId => typeof id === "string" && known.has(id)))];
}

/** Reconcile a stored flow against the known box set: keep the first occurrence
 *  of each known id in stored order, drop unknown ids, append boxes new-in-code,
 *  prune the collapsed + hidden sets to the known set. Empty/garbage → default
 *  order. */
export function reconcileFlow(
  flow: { order?: unknown; collapsed?: unknown; hidden?: unknown },
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
  return { order, collapsed: knownSet(flow?.collapsed, known), hidden: knownSet(flow?.hidden, known) };
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

/** Toggle `id`'s membership of a disposition set (`collapsed` or `hidden`). The
 *  two axes are independent — hiding a collapsed box keeps it collapsed, so it
 *  returns minimized when restored. */
function toggleIn(flow: FlowRegion, key: "collapsed" | "hidden", id: BoxId): FlowRegion {
  const has = flow[key].includes(id);
  return { ...flow, [key]: has ? flow[key].filter((x) => x !== id) : [...flow[key], id] };
}

/** Add `id` to the collapsed set if absent, remove it if present. */
export function toggleCollapsed(flow: FlowRegion, id: BoxId): FlowRegion {
  return toggleIn(flow, "collapsed", id);
}

/** Remove `id` from the stage (idempotent). */
export function hide(flow: FlowRegion, id: BoxId): FlowRegion {
  return flow.hidden.includes(id) ? flow : toggleIn(flow, "hidden", id);
}

/** Return `id` to the stage (idempotent). */
export function show(flow: FlowRegion, id: BoxId): FlowRegion {
  return flow.hidden.includes(id) ? toggleIn(flow, "hidden", id) : flow;
}
