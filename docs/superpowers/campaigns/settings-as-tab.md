# Campaign: settings as a right-column tab instead of a gear-icon modal

Status: backlog
Raised: 2026-06-13

## Idea

Replace the gear icon + settings **modal** with another **tab** in the right
column (`sections / loops / plan / capture / due / profile / settings`) that
renders a settings **page** inline — consistent with every other panel, no
overlay.

## Context

- `apps/desktop/src/App.svelte`: the right column renders a tab nav from
  `const TABS = ["sections","loops","plan","capture","due","profile"]` and a
  `{#if tab === ...}` block; the gear button is a separate `<Button variant="icon"
  ... onclick={() => settingsOpen.set(true)}>⚙</Button>` in the tab nav, and
  `<SettingsModal />` is mounted at the bottom.
- `apps/desktop/src/components/SettingsModal.svelte`: the actual settings content
  (ui scale fader, grid-snap toggle, capture-buffer chips, analysis-device
  auto/cpu toggle) wrapped in a `<Modal>`.
- `settingsOpen` store (visibility); the `,` key (`keys.ts`) currently does
  `settingsOpen.set(true)`.

## Likely shape

- Extract the settings controls out of `SettingsModal.svelte` into a
  `SettingsPanel.svelte` (no `<Modal>` wrapper — just the rows), styled to sit in
  the panel column like `DuePanel`/`ProfilingPanel`.
- Add `"settings"` to `TABS`; render `<SettingsPanel />` for that tab. Remove the
  gear `<Button>` and the `<SettingsModal />` element (delete the modal
  component, or keep it only if something else needs it).
- Repoint the `,` key to select the settings tab (set the `tab` state) instead of
  opening the modal; drop the `settingsOpen` store if nothing else uses it.

## Next step

Brainstorm → spec → plan → build (frontend only). Decide: keep `,` as a
shortcut that switches to the settings tab; whether the tab label "settings" fits
the nav width (it's longer than the others); whether to keep `settingsOpen` for
any non-tab callers.
