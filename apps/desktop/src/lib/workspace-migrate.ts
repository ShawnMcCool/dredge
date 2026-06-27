// One-time read of the durable settings into a Workspace. An existing
// `workspace` wins; otherwise the legacy `panel_layout` becomes the right
// region, `library` is seeded left, and the old collapse booleans carry over.
// Always reconciled against the current tab set. The legacy keys are read here
// and nowhere else — once written back as `workspace` they go quiet.
import { defaultWorkspace, reconcileWorkspace, type DockLayout, type Workspace } from "./dock";

export function migrateWorkspace(all: Record<string, unknown>, allTabs: string[]): Workspace {
  const existing = all.workspace;
  if (existing && typeof existing === "object" && "left" in existing && "right" in existing) {
    return reconcileWorkspace(existing as Workspace, allTabs);
  }
  const [first, ...rest] = allTabs;
  const rightLayout = Array.isArray(all.panel_layout) ? (all.panel_layout as DockLayout) : null;
  if (!rightLayout && !rest.length) return defaultWorkspace(allTabs);
  // Intermediate stored shape; reconcileWorkspace validates it and seeds `stage`.
  const ws = {
    left: {
      layout: first ? [{ tabs: [first], active: first, weight: 1 }] : [],
      collapsed: all.library_collapsed === true,
    },
    right: {
      layout: rightLayout ?? (rest.length ? [{ tabs: rest, active: rest[0], weight: 1 }] : []),
      collapsed: all.panels_collapsed === true,
    },
  };
  return reconcileWorkspace(ws, allTabs);
}
