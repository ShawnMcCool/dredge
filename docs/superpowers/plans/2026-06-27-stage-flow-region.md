# Stage Flow Region Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the stage's control boxes a managed **flow region** of the workspace — reorderable (drag the header) and per-box collapsible, persisted — and unify the drifted header idioms into one `SurfaceHead`.

**Architecture:** A new `lib/stage.ts` holds the pure flow transforms (order + collapsed set, reconciled against a known box set). `Workspace` widens from `{left, right}` to `{left, right, stage}` with the stage a `FlowRegion`. A `lib/stage-flow.svelte.ts` controller (provided via context) owns collapse + header-drag reorder. `Box` becomes the managed unit: it takes an `id`, reads the controller for its collapsed state, and makes its header the drag surface. `App.svelte` renders the flow from `workspace.stage.order` via a box registry. One `SurfaceHead` primitive backs `Box` and `SectionHead`.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest. Frontend only — no Rust.

Spec: `docs/superpowers/specs/2026-06-27-stage-flow-region-design.md`.

---

## File Structure

- `apps/desktop/src/lib/stage.ts` — **create**: `STAGE_BOXES`, `BoxId`, `FlowRegion`, `defaultFlow`, `reconcileFlow`, `moveBox`, `toggleCollapsed`.
- `apps/desktop/src/lib/stage.test.ts` — **create**.
- `apps/desktop/src/lib/dock.ts` — **modify**: rename `Region`→`DockRegion`; widen `Workspace` to add `stage: FlowRegion`; `defaultWorkspace`/`reconcileWorkspace` seed + reconcile `stage`.
- `apps/desktop/src/lib/dock.test.ts` — **modify**: workspace tests carry `stage`.
- `apps/desktop/src/lib/workspace-migrate.ts` + `.test.ts` — **modify**: seed `stage` when an existing workspace lacks it.
- `apps/desktop/src/lib/stage-flow.svelte.ts` — **create**: the collapse + reorder controller (context).
- `apps/desktop/src/lib/ui/SurfaceHead.svelte` — **create**: shared label + tools (+ optional collapse caret) row.
- `apps/desktop/src/lib/ui/Box.svelte` — **modify**: use `SurfaceHead`; take `id`; read the controller for collapsed + header drag.
- `apps/desktop/src/lib/ui/SectionHead.svelte` — **modify**: render `SurfaceHead`.
- `apps/desktop/src/components/{MetronomeBox,Isolation,ClickTrack,Notes,Recordings,Tuner,Drill}.svelte` — **modify**: pass `id` to `<Box>`.
- `apps/desktop/src/App.svelte` — **modify**: provide the controller; render the flow from the registry; delete the hard-coded box block.

Verification:
- Unit: `cd apps/desktop && pnpm vitest run lib/stage.test.ts lib/dock.test.ts lib/workspace-migrate.test.ts`
- Lint: `just lint`
- Smoke (rendering/effects): vite `:5173` + chrome-devtools, watch for `effect_update_depth_exceeded`.

---

## Phase 1 — `SurfaceHead` extraction

### Task 1: `SurfaceHead.svelte` + adopt in `Box` and `SectionHead`

**Files:**
- Create: `apps/desktop/src/lib/ui/SurfaceHead.svelte`
- Modify: `apps/desktop/src/lib/ui/Box.svelte`, `apps/desktop/src/lib/ui/SectionHead.svelte`

- [ ] **Step 1: Create `SurfaceHead.svelte`** — the shared label + tools row, plus an optional collapse caret (off unless `collapsible`):

```svelte
<script lang="ts">
  // The one labelled-surface header row: a small-caps muted label, an optional
  // right-aligned tools slot, and an optional leading collapse caret. Backs both
  // the stage box header (Box) and the in-page group heading (SectionHead) so
  // every label header in the app is drawn once. Outer chrome (card border vs
  // page divider) belongs to the caller.
  import type { Snippet } from "svelte";

  interface Props {
    label: string;
    tools?: Snippet;
    /** Show a collapse caret before the label (stage boxes only). */
    collapsible?: boolean;
    collapsed?: boolean;
    oncollapse?: () => void;
  }
  let { label, tools, collapsible = false, collapsed = false, oncollapse }: Props = $props();
</script>

<span class="surface-head">
  {#if collapsible}
    <button
      class="caret"
      onclick={oncollapse}
      title={collapsed ? "expand" : "collapse"}
      aria-label={collapsed ? "expand" : "collapse"}>{collapsed ? "›" : "⌄"}</button
    >
  {/if}
  <span class="lbl">{label}</span>
  {#if tools}<span class="tools">{@render tools()}</span>{/if}
</span>

<style>
  .surface-head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1 1 auto;
  }
  .lbl {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    min-width: 0;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  /* uniform header tools: plain muted glyph buttons, matching height everywhere */
  .tools :global(button) {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    font-size: 0.95rem;
    line-height: 1;
  }
  .tools :global(button:hover) {
    color: var(--fg);
  }
  .caret {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    font-size: 0.95rem;
    line-height: 1;
    flex: 0 0 auto;
  }
  .caret:hover {
    color: var(--fg);
  }
</style>
```

- [ ] **Step 2: Adopt in `Box.svelte`** — replace the inline `<header>` contents with `SurfaceHead`, keeping the card chrome (`.head` border/min-height). Leave `id`/collapse/drag for Phase 3 — this step is pure de-dup, no behavior change:

```svelte
<!-- script: add -->
import SurfaceHead from "./SurfaceHead.svelte";
<!-- markup: replace the <header class="head">…</header> with -->
<header class="head">
  <SurfaceHead {label} {tools} />
</header>
```

Remove the now-unused `.lbl` / `.head-actions` rules from `Box.svelte` (they live in `SurfaceHead`); keep `.head` (the card-header chrome: `display:flex; min-height:32px; padding:4px 10px; border-bottom`).

- [ ] **Step 3: Adopt in `SectionHead.svelte`** — render `SurfaceHead` inside the `.section-head` divider chrome:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import SurfaceHead from "./SurfaceHead.svelte";
  interface Props { children: Snippet; tools?: Snippet }
  let { children, tools }: Props = $props();
  // SectionHead's label is a snippet; render it into a string-free SurfaceHead by
  // passing the text through. SurfaceHead takes `label: string`, so SectionHead
  // keeps its snippet API and renders the row itself using SurfaceHead's styles.
</script>
```

Because `SectionHead`'s label is a `children` snippet (not a string) and may carry markup, keep `SectionHead` rendering its own `<h3>{@render children()}</h3>` + `tools`, but **delete its duplicated `.tools` / label styling and import the shared rules**. Simplest concrete approach: leave `SectionHead.svelte` markup as-is but replace its `<style>` label/tools rules with the same values as `SurfaceHead` (single source by reference in review). If the snippet label can be a plain string in every consumer, convert to `<SurfaceHead label=… {tools}/>` instead.

> Decision for the implementer: check the 6 consumers (`Devices`, `Sections`, `Export`, `SettingsPanel`, `Loops`, `Guide`). If every `<SectionHead>…</SectionHead>` body is plain text, convert `SectionHead` to a thin wrapper over `SurfaceHead` (`<SurfaceHead label={text} {tools}/>`). If any pass markup, keep the `<h3>` snippet form and share only the CSS. Pick one and apply uniformly.

- [ ] **Step 4: Verify** — `just lint` clean; visually unchanged.

Run: `just lint`
Expected: `0 ERRORS 0 WARNINGS`; clippy/fmt unaffected.

- [ ] **Step 5: Smoke-test** — vite `:5173` + chrome: stage box headers and page section headings look identical to before; no console errors beyond the known Tauri-absent ones.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/lib/ui/SurfaceHead.svelte apps/desktop/src/lib/ui/Box.svelte apps/desktop/src/lib/ui/SectionHead.svelte apps/desktop/src/components
git commit -m "refactor(ui): one SurfaceHead primitive backs Box + SectionHead"
```

---

## Phase 2 — Stage model + migration

### Task 2: `lib/stage.ts` pure transforms

**Files:** Create `apps/desktop/src/lib/stage.ts`, `apps/desktop/src/lib/stage.test.ts`.

- [ ] **Step 1: Failing test** (`stage.test.ts`):

```ts
import { describe, it, expect } from "vitest";
import { STAGE_BOXES, defaultFlow, reconcileFlow, moveBox, toggleCollapsed } from "./stage";

describe("defaultFlow", () => {
  it("is the canonical order, nothing collapsed", () => {
    const f = defaultFlow();
    expect(f.order).toEqual([...STAGE_BOXES]);
    expect(f.collapsed).toEqual([]);
  });
});

describe("reconcileFlow", () => {
  it("keeps known order, drops unknown, appends missing, prunes collapsed", () => {
    const f = reconcileFlow({ order: ["tuner", "zzz", "metronome"], collapsed: ["zzz", "tuner"] }, [
      "metronome",
      "tuner",
      "drill",
    ]);
    expect(f.order).toEqual(["tuner", "metronome", "drill"]); // unknown dropped, missing appended
    expect(f.collapsed).toEqual(["tuner"]); // unknown collapsed pruned
  });
  it("defaults from an empty/garbage flow", () => {
    expect(reconcileFlow({ order: [], collapsed: [] }, ["metronome", "tuner"])).toEqual({
      order: ["metronome", "tuner"],
      collapsed: [],
    });
  });
});

describe("moveBox", () => {
  it("reorders to a target index", () => {
    expect(moveBox(["a", "b", "c"], "c", 0)).toEqual(["c", "a", "b"]);
    expect(moveBox(["a", "b", "c"], "a", 2)).toEqual(["b", "c", "a"]);
  });
  it("is a no-op for an unknown id", () => {
    expect(moveBox(["a", "b"], "x", 0)).toEqual(["a", "b"]);
  });
});

describe("toggleCollapsed", () => {
  it("adds then removes an id from the set", () => {
    const a = toggleCollapsed({ order: ["x"], collapsed: [] }, "x");
    expect(a.collapsed).toEqual(["x"]);
    expect(toggleCollapsed(a, "x").collapsed).toEqual([]);
  });
});
```

- [ ] **Step 2: Run → FAIL** — `cd apps/desktop && pnpm vitest run lib/stage.test.ts`.

- [ ] **Step 3: Implement** (`stage.ts`):

```ts
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
export function reconcileFlow(flow: { order?: unknown; collapsed?: unknown }, allBoxes: readonly string[]): FlowRegion {
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
  const collapsed = [...new Set(collapsedSrc.filter((id): id is BoxId => typeof id === "string" && known.has(id)))];
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
```

- [ ] **Step 4: Run → PASS.**

- [ ] **Step 5: Commit** — `git add apps/desktop/src/lib/stage.ts apps/desktop/src/lib/stage.test.ts && git commit -m "feat(stage): pure flow-region transforms (order + collapse)"`

### Task 3: widen `Workspace` with the stage region

**Files:** Modify `apps/desktop/src/lib/dock.ts`, `apps/desktop/src/lib/dock.test.ts`.

- [ ] **Step 1: Update the workspace tests** — the `wkeys` helper and existing workspace tests must tolerate a `stage`. Change `defaultWorkspace`/`reconcileWorkspace` expectations to assert `stage` is seeded. Add to `dock.test.ts`:

```ts
import { defaultFlow } from "./stage";

it("defaultWorkspace seeds a default stage flow", () => {
  expect(defaultWorkspace(["library", "a", "b"]).stage).toEqual(defaultFlow());
});
it("reconcileWorkspace seeds a missing stage and reconciles a present one", () => {
  const ws = reconcileWorkspace(
    {
      left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
      right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
    } as never,
    ["library", "a", "b"],
  );
  expect(ws.stage).toEqual(defaultFlow()); // absent → seeded
});
```

(The existing `wkeys`-based assertions keep passing — they only read `left`/`right`.)

- [ ] **Step 2: Run → FAIL** — `pnpm vitest run lib/dock.test.ts`.

- [ ] **Step 3: Implement** in `dock.ts`:
  - Rename `export interface Region` → `export interface DockRegion` (update the two `Workspace` field types and any internal references).
  - Import the flow types and widen `Workspace`:

```ts
import { defaultFlow, reconcileFlow, type FlowRegion } from "./stage";

export interface DockRegion {
  layout: DockLayout;
  collapsed: boolean;
}
export interface Workspace {
  left: DockRegion;
  right: DockRegion;
  stage: FlowRegion;
}
```

  - In `defaultWorkspace`, add `stage: defaultFlow()` to the returned object.
  - In `reconcileWorkspace`, carry the stage: at the end, build the result with
    `stage: reconcileFlow(ws?.stage ?? {}, STAGE_BOXES)` (import `STAGE_BOXES`).
    The early `seen.size === 0 → defaultWorkspace(allTabs)` branch already yields a
    default stage. Every other `return` must include the reconciled stage.

- [ ] **Step 4: Run → PASS** (`pnpm vitest run lib/dock.test.ts`).

- [ ] **Step 5: Commit** — `git commit -am "feat(dock): widen Workspace with the stage flow region"`

### Task 4: migration seeds the stage

**Files:** Modify `apps/desktop/src/lib/workspace-migrate.ts`, `.test.ts`.

- [ ] **Step 1: Failing test** — add to `workspace-migrate.test.ts`:

```ts
import { defaultFlow } from "./stage";

it("seeds a default stage when migrating a stage-less workspace", () => {
  const ws = migrateWorkspace(
    {
      workspace: {
        left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
        right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
      },
    },
    ALL,
  );
  expect(ws.stage).toEqual(defaultFlow());
});
it("seeds a stage on legacy migration too", () => {
  expect(migrateWorkspace({ panel_layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }] }, ALL).stage).toEqual(
    defaultFlow(),
  );
});
```

- [ ] **Step 2: Run → FAIL** — `pnpm vitest run lib/workspace-migrate.test.ts`.

- [ ] **Step 3: Implement** — `migrateWorkspace` already routes both the existing-workspace and legacy paths through `reconcileWorkspace`, which (after Task 3) seeds `stage`. The only gap is the legacy `Workspace` literal it builds: it has no `stage`, but `reconcileWorkspace` seeds it. So **no code change may be needed** — run the test. If the existing-workspace branch returns before `reconcileWorkspace` adds stage, confirm it calls `reconcileWorkspace` (it does). If a TS error appears because the legacy literal lacks `stage`, add `stage: defaultFlow()` to that literal (import `defaultFlow`).

- [ ] **Step 4: Run → PASS.**

- [ ] **Step 5: Commit** — `git commit -am "feat(workspace): migration seeds the stage flow region"`

---

## Phase 3 — Render the flow from the model + collapse

### Task 5: `stage-flow.svelte.ts` controller

**Files:** Create `apps/desktop/src/lib/stage-flow.svelte.ts`.

- [ ] **Step 1: Implement** — collapse + header-drag reorder over one flow container:

```ts
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
      // insert before/after the target depending on which half the pointer is in
      const r = overBox.getBoundingClientRect();
      const after = e.clientX > r.left + r.width / 2;
      const order = getFlow().order;
      let toIndex = order.indexOf(targetId);
      if (after) toIndex += 1;
      // moveBox handles the from-removal offset; recompute against current order
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
```

> Note: `onHeadMove` reorders live as the pointer crosses boxes (like the dock's
> live tab reorder), so the FLIP animation reads continuously. Because each move
> persists through `onchange`, the gesture is idempotent per frame.

- [ ] **Step 2: Lint** — `just lint` (svelte-check covers `.svelte.ts`). Clean.

- [ ] **Step 3: Commit** — `git add apps/desktop/src/lib/stage-flow.svelte.ts && git commit -m "feat(stage): collapse + header-drag reorder controller"`

### Task 6: `Box` reads the controller (`id`, collapse, drag surface)

**Files:** Modify `apps/desktop/src/lib/ui/Box.svelte`.

- [ ] **Step 1: Implement** — add `id`, wire collapse + header drag:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import SurfaceHead from "./SurfaceHead.svelte";
  import { getStageFlow } from "../stage-flow.svelte";
  import type { BoxId } from "../stage";

  interface Props {
    id: BoxId;
    label: string;
    dim?: boolean;
    grow?: boolean;
    wide?: boolean;
    tools?: Snippet;
    children: Snippet;
  }
  let { id, label, dim = false, grow = true, wide = false, tools, children }: Props = $props();

  const flow = getStageFlow();
  const collapsed = $derived(flow.isCollapsed(id));
</script>

<section class="box" class:dim class:nogrow={!grow} class:wide class:collapsed data-box={id}>
  <!-- the header is the drag surface; the caret/tools still click (threshold) -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header
    class="head"
    onpointerdown={(e) => flow.onHeadDown(e, id)}
    onpointermove={(e) => flow.onHeadMove(e)}
    onpointerup={() => flow.onHeadUp()}
    onpointercancel={() => flow.onHeadUp()}
  >
    <SurfaceHead {label} {tools} collapsible collapsed={collapsed} oncollapse={() => { if (!flow.didDrag()) flow.toggle(id); }} />
  </header>
  {#if !collapsed}
    <div class="body">{@render children()}</div>
  {/if}
</section>
```

Add `cursor: grab;` to `.head` and `.box.collapsed .head { border-bottom: none; }` so a collapsed box is just its header strip. Keep the rest of `.box` CSS.

- [ ] **Step 2:** the `data-box` attribute + `cursor: grab` are the only visible changes pre-wiring; lint after Task 7 (App provides the context). `Box.svelte` type-checks now.

- [ ] **Step 3: Commit** — `git commit -am "feat(ui): Box is the managed flow unit (id, collapse, drag surface)"`

### Task 7: pass `id` to every stage `<Box>`

**Files:** Modify the 7 components.

- [ ] **Step 1:** add the matching `id` to each `<Box …>`:
  - `MetronomeBox.svelte`: `<Box id="metronome" label="metronome">`
  - `Isolation.svelte`: `<Box id="isolation" label="isolation" grow={!hasStems}>`
  - `ClickTrack.svelte`: `<Box id="click" label="click track">`
  - `Notes.svelte`: `<Box id="notes" label={…} wide>`
  - `Recordings.svelte`: `<Box id="recordings" label="recordings">`
  - `Tuner.svelte`: `<Box id="tuner" label="tuner" …>`
  - `Drill.svelte`: `<Box id="drill" label="drill" wide>`

- [ ] **Step 2: Commit** — `git commit -am "feat(ui): stage boxes declare their flow id"`

### Task 8: App renders the flow from the registry

**Files:** Modify `apps/desktop/src/App.svelte`.

- [ ] **Step 1: Script** — add the box registry + controller:

```svelte
<script lang="ts">
  import { createStageFlow, setStageFlow } from "./lib/stage-flow.svelte";
  import { STAGE_BOXES, type BoxId } from "./lib/stage";
  // registry: id → component + presence predicate (the stage analogue of TAB_VIEWS)
  const STAGE_REGISTRY: Record<BoxId, { component: Component; present: () => boolean }> = {
    metronome: { component: MetronomeBox, present: () => true },
    isolation: { component: Isolation, present: () => !!$openSong },
    click: { component: ClickTrack, present: () => !!$openSong },
    notes: { component: Notes, present: () => !!$openSong },
    recordings: { component: Recordings, present: () => !!$openSong },
    tuner: { component: Tuner, present: () => true },
    drill: { component: Drill, present: () => !!$openSong && !!$drillSpan },
  };
  const stageFlow = createStageFlow(
    () => $workspace.stage,
    (flow) => void actions.setWorkspace({ ...$workspace, stage: flow }),
  );
  setStageFlow(stageFlow);
  // the present boxes, in saved order
  const stageBoxes = $derived($workspace.stage.order.filter((id) => STAGE_REGISTRY[id].present()));
</script>
```

- [ ] **Step 2: Markup** — replace the hard-coded `.boxes` block with the registry render. The container registers with the controller (via an action) for hit-testing:

```svelte
<div class="boxes" use:registerStage>
  {#each stageBoxes as id (id)}
    {@const Box = STAGE_REGISTRY[id].component}
    <div class="box-slot" animate:flip={{ duration: 180 }}>
      <Box />
    </div>
  {/each}
</div>
```

Wait — the registry's `component` IS the tool (e.g. `Tuner`), which itself renders `<Box id=…>`. So render the tool directly; the FLIP wrapper must carry the key. Use:

```svelte
<div class="boxes" use:registerStage>
  {#each stageBoxes as id (id)}
    {@const Tool = STAGE_REGISTRY[id].component}
    <Tool />
  {/each}
</div>
```

`animate:flip` requires a keyed direct child with the directive; since each `Tool` renders a `<section class="box">`, apply the key/flip by wrapping is not possible without a DOM node owning the directive. Put the `animate:flip` on the tool via a wrapper only in Phase 4 (reorder). For Phase 3, render `<Tool />` in order without flip (order still honored on re-render). Add the container action:

```svelte
<script lang="ts">
  function registerStage(el: HTMLElement) {
    stageFlow.registerContainer(el);
    return {};
  }
</script>
```

Import `flip` from `svelte/animate` is deferred to Phase 4.

- [ ] **Step 2b:** remove the now-unused direct conditional imports? No — the tools are still imported and referenced by the registry. Keep the imports.

- [ ] **Step 3: Gate** — `just lint` clean; `pnpm vitest run` green.

- [ ] **Step 4: Smoke-test** (vite + chrome):
  - boxes render in `STAGE_BOXES` order; with no song only metronome + tuner show.
  - clicking a box's collapse caret hides its body and leaves the header; reload (or re-derive) keeps it collapsed.
  - **no `effect_update_depth_exceeded`.**
  - To exercise with data absent from the backend, temporarily seed `$workspace` via the Svelte devtools is not available — rely on the unit tests for order/collapse correctness and verify presence + caret render structurally.

- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat(stage): render the flow from workspace.stage + per-box collapse"`

---

## Phase 4 — Box reorder drag

### Task 9: live header-drag reorder + FLIP

**Files:** Modify `apps/desktop/src/App.svelte` (FLIP wrapper), confirm `Box`/controller wiring.

- [ ] **Step 1:** give each rendered tool a keyed, flip-animated wrapper so reorder glides. Since `animate:flip` must sit on a keyed element that is a direct child of the `{#each}`, wrap each tool:

```svelte
<script lang="ts">
  import { flip } from "svelte/animate";
</script>

<div class="boxes" use:registerStage>
  {#each stageBoxes as id (id)}
    {@const Tool = STAGE_REGISTRY[id].component}
    <div class="box-flip" animate:flip={{ duration: 180 }}>
      <Tool />
    </div>
  {/each}
</div>
```

Style `.box-flip` to be layout-transparent so the flex-flow still sizes the inner
`.box` (it must carry the flex item sizing): give `.box-flip` `display: contents`
**only if** FLIP still tracks it; FLIP needs a real box, so instead make
`.box-flip` the flex item (`flex: 1 1 240px; min-width: 0; display: flex;`) and
change `Box`'s `.box` to `flex: 1 1 auto` within it. Verify the wrap/grow still
matches today; if `display: contents` breaks FLIP, the wrapper-as-flex-item is the
fallback (and `Box`'s `grow`/`wide` classes move to the wrapper via a prop).

> Implementer note: the cleanest seam is for the **wrapper** to own flex sizing and
> `Box` to fill it. If that shifts the `grow`/`wide`/`nogrow` behavior, thread those
> as data attributes on the wrapper and port the three flex rules there. Keep the
> visual result identical to Phase 3 before the drag.

- [ ] **Step 2:** the controller's `onHeadMove` already reorders live; confirm the
  header drag updates `$workspace.stage.order` and the FLIP animates the move.

- [ ] **Step 3: Gate** — `just lint` clean; `pnpm vitest run` green.

- [ ] **Step 4: Smoke-test** (vite + chrome):
  - drag a box by its header → it reorders and glides; order persists across reload.
  - the collapse caret and any header tools still click (no accidental drag under threshold).
  - dragging does not cross into a dock (stage-only).
  - no effect loop.

- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat(stage): drag a box header to reorder the flow"`

---

## Self-Review

**Spec coverage:** SurfaceHead unification (Task 1) ✓; `Workspace` widened to 3 regions (Task 3) ✓; `FlowRegion` model + reconcile/move/toggle (Task 2) ✓; presence orthogonal to order (Task 8 registry `present()`) ✓; migration seeds stage (Task 4) ✓; Box as managed unit with id + collapse + header-drag surface (Tasks 6–7) ✓; whole-header drag, no grip glyph (Tasks 6, 9) ✓; per-box collapse persists incl. absent boxes (model keeps collapsed for all known ids — Task 2) ✓; reorder stage-only (Task 5 controller, no region crossing) ✓; transport/waveform outside the flow (untouched in App) ✓; dock tab-bar not merged into SurfaceHead (Task 1 — Box/SectionHead only) ✓.

**Placeholder scan:** the two `> Decision/Implementer note` blocks (SectionHead snippet-vs-string; FLIP wrapper flex seam) are genuine either/or seams with both branches specified and a "pick one, apply uniformly" instruction — not deferrals of unknown work. No TODO/TBD.

**Type consistency:** `FlowRegion {order, collapsed}` defined Task 2, used in `dock.ts` (Task 3), `stage-flow` (Task 5), App (Task 8); `BoxId` from `stage.ts` used in `Box` (Task 6), the 7 tools (Task 7), the registry (Task 8); `DockRegion` rename applied in Task 3 and `Workspace.left/right` typed to it; controller methods (`isCollapsed`/`toggle`/`registerContainer`/`onHeadDown|Move|Up`/`didDrag`/`dragId`) match between Task 5 definition and Task 6/8 call sites.

**Phasing:** Phase 1 ships standalone (pure refactor). Phase 2 is model-only (no UI change). Phase 3 wires render+collapse and is shippable. Phase 4 adds drag. Tasks 6–8 carry the only intermediate window where `Box` expects a context App hasn't provided until Task 8 — land Tasks 6–8 together (commit per task, lint at Task 8), like the workspace slice's convergence.
