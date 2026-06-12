# earworm — Plan 8: UI layout abstractions (resize discipline)

> **For agentic workers:** Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Rebuild the UI's layout layer on a small set of shared primitives so every container handles resize/zoom correctly: nothing overflows onto siblings, control rows wrap as coherent groups, readouts never wrap mid-value, rails adapt instead of being fixed.

**Diagnosis (from live screenshot at ~2000 css-px window, zoom 1.75):**
1. `.transport` flex row has no wrap policy and default `overflow: visible` → children paint over the right rail when the stage narrows.
2. `00:41.2 / 03:13` readout wraps mid-value (flex shrink, no `nowrap`).
3. `BASS FOCUS` button wraps its own label to two lines; toggle/button sizing is ad-hoc per component.
4. Shell grid `240px 1fr 320px` is rigid; rails don't shrink/grow, columns lack `overflow` containment.
5. Control styling (chips, toggles, buttons) duplicated across Transport/Sections/Loops/PlanBuilder/Capture/StemMixer with divergent paddings/heights.

**The abstractions (all in `apps/desktop/src/lib/ui/`):**

- `Button.svelte` — single button primitive. Props: `variant: "default" | "chip" | "toggle" | "icon"`, `active?: boolean`, `accent?: boolean`. One height token (`--control-h: 28px`), `white-space: nowrap`, consistent padding/radius/focus style. Every button in the app becomes this.
- `Group.svelte` — labeled control cluster (`<span class="label">` + slot in a `inline-flex` that **never wraps internally** (`flex-wrap: nowrap`) and shrinks as a unit (`flex: 0 0 auto`). Optional `grow` prop for the one elastic child per toolbar (the rate slider).
- `Toolbar.svelte` — horizontal container: `display: flex; flex-wrap: wrap; row-gap; column-gap; align-items: center; min-width: 0; overflow: hidden`. Children are Groups; when space runs out, whole Groups wrap to the next line — never overflow, never split.
- `readout` global utility class — `font: mono; white-space: nowrap; font-variant-numeric: tabular-nums` for all time/rate/pitch values (tabular-nums also stops the playhead time from jittering).
- Shell grid in `App.svelte` →
  `grid-template-columns: minmax(170px, 240px) minmax(320px, 1fr) minmax(250px, 340px)`,
  every column `min-width: 0; overflow-y: auto` (stage: `overflow: hidden` since the canvas manages itself), so no column can be painted over.
- `lib/actions/canvasSize.ts` — extract Waveform's ResizeObserver+DPR sizing into a reusable Svelte action `use:canvasSize={(w, h, dpr) => ...}`; Waveform consumes it (behavior identical — this is consolidation, not change).

**Not in scope:** visual redesign, new features, component logic changes. Same look, correct behavior under resize.

---

### Task 1: Primitives

- [x] Create `lib/ui/{Button,Group,Toolbar}.svelte` + add `--control-h` and `.readout` to `app.css`. Tokens reuse the existing custom properties (`--line`, `--muted`, `--accent`, `--space`).
- [x] `pnpm build` clean. Commit: `feat(desktop): ui layout primitives (Button/Group/Toolbar)`

### Task 2: Transport on the primitives

- [ ] Rebuild `Transport.svelte`: Toolbar with Groups — [play, time readout] · [rate readout, slider (grow), preset chips] · [pitch chips, cents] · [BASS FOCUS (toggle, label "BASS FOCUS" nowrap), MUTE]. Slider `min-width: 120px; max-width: 320px`. Time/rate/pitch values get `.readout`.
- [ ] StemMixer's "Separate stems" row and the mixer strip adopt Button/Toolbar where applicable.
- [ ] `pnpm build` clean. Commit: `refactor(desktop): transport + stem mixer on toolbar abstractions`

### Task 3: Shell + panels sweep

- [ ] App.svelte grid → minmax columns with overflow containment as specified; help footer `overflow-wrap: anywhere`.
- [ ] Sweep Sections/Loops/PlanBuilder/Capture/DuePanel/Library/PlanRunner: replace raw `<button>`s with `Button`, fixed widths with `minmax`/`min-width: 0`, ensure each panel scrolls (`overflow-y: auto`) instead of growing the rail; inputs get `width: 100%; min-width: 0` inside their rows.
- [ ] Waveform: consume `canvasSize` action; everything else untouched.
- [ ] `pnpm build && pnpm vitest run` clean. Commit: `refactor(desktop): shell grid + panels on shared primitives`

### Task 4: Visual verification at three widths (the actual gate)

Launch the built release app and screenshot it **at three window sizes**; inspect the images yourself (Read tool) and iterate until all pass:

```bash
cd ~/src/earworm && cargo build -p earworm-desktop --release
target/release/earworm & sleep 4
ADDR=$(hyprctl clients -j | jq -r '.[] | select(.class=="earworm") | .address')
hyprctl dispatch setfloating address:$ADDR
for size in "2400 1300" "1500 900" "1000 700"; do
  hyprctl dispatch resizewindowpixel "exact ${size% *} ${size#* },address:$ADDR"
  hyprctl dispatch movewindowpixel "exact 100 100,address:$ADDR"; sleep 1
  GEO=$(hyprctl clients -j | jq -r '.[] | select(.address=="'$ADDR'") | "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"')
  grim -g "$GEO" /tmp/ew-$( echo $size | tr ' ' x ).png
done
pkill -f target/release/earworm
```

Pass criteria per screenshot:
- nothing from the stage paints over either rail; rails never overlap stage
- transport groups wrap to extra rows when narrow; no mid-value wrapping in any readout; no button label wraps
- waveform canvas spans exactly the stage width at every size
- right-rail content scrolls rather than stretching the column
- at 1000×700 the app is still fully usable (groups stacked, nothing clipped invisibly)

- [ ] All three screenshots pass. Leave them in /tmp for review. Commit any fixes: `fix(desktop): resize verification fixes`

### Task 5: Gate

- [ ] `cargo test && cargo clippy --workspace -- -D warnings && pnpm vitest run && pnpm build`; commit final state.
