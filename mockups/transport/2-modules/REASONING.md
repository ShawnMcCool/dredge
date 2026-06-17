# Transport redesign — Direction 2: Hero + Labelled Modules

A self-contained HTML mockup of Dredge's playback control strip, committing
fully to a **transport hero + labelled control modules** layout. The strip sits
full-width directly under the waveform in the centre stage column.

## Style

Dredge's house aesthetic: near-black canvas (`--bg #101014`), a single amber
accent (`--accent #e0a458`), system-ui for chrome and a monospace stack with
`tabular-nums` for every number that moves. Micro-labels are the app's signature
device — 10px, uppercase, `letter-spacing .08em`, `--muted` — borrowed from the
modernised settings panel. The strip is calm and instrument-like, not a dense
pro-audio bar and not generic SaaS. Borders are hairlines (`--line`), radii are
2px, there are no drop shadows. State is expressed almost entirely through
**colour** (muted → fg → accent), keeping the surface quiet until something is
active.

## Design decisions

**Hero at left.** The one primary action (play/pause) is the only filled-amber
control on the strip — a 52px amber disc that is unmistakably *the* button.
Beside it the time reads large in mono (`00:00.0` in `--fg`, `/ 04:24` in
`--muted` below it). This pairing anchors the strip: your eye lands on the
transport first, the secondary instrument controls second.

**One hairline divider.** A single vertical `--line` rule separates the hero
from the module rack. There are deliberately **no** pipe separators between
individual controls — modules are grouped by whitespace and the consistent
label-over-body rhythm instead.

**Labelled module rack.** Each of SPEED, PITCH, VOLUME, BASS FOCUS is a small
module: a 10px uppercase micro-label on top, the control body beneath, on a
shared 28px control-height grid so the row reads as a tidy panel.

- **SPEED** carries its live value inline with the label (`100%` in accent),
  the four presets `50 70 85 100` as compact mono chips (active = amber chip),
  and a slim 25–200% slider grouped underneath — everything speed-related under
  one label.
- **PITCH** is a single unified stepper `−  0 st  +` with a centred mono value,
  so ±semitone reads as one object rather than three buttons.
- **VOLUME** is mute icon + slim 0–150% slider + `%` in the label.
- **BASS FOCUS** is a pip+state toggle; shown both OFF (muted) and ON (amber,
  glowing pip) per the brief, to demonstrate the active state.

**Reset workspace** is pushed to the far right of the rack as a quiet
recovery affordance: a rotate-ccw icon at `--wave` (barely visible), brightening
to `--fg` on hover and spinning on press. Low-contrast until needed, exactly as
in the real app.

**Icon system.** All emoji are replaced with inline, monochrome, Feather-style
line SVGs (~16–18px, `stroke="currentColor"`, stroke-width 2, round caps): play
(filled triangle), pause, volume (speaker + waves), mute (speaker + x),
rotate-ccw. Because they are `currentColor`-driven, every state change is just a
colour change — no icon swaps beyond play↔pause and vol↔mute, which toggle via a
class.

## Requirements mapping

- Play/pause → amber hero disc (`.play`, `is-playing` shows pause).
- Time readout → mono `00:00.0 / 04:24`, current `--fg`, total `--muted`.
- Volume → mute icon toggle (incl. muted instance in narrow) + slider + mono %.
- Bass focus → toggle shown OFF and ON/active in accent.
- Speed → presets `50 70 85 100` + continuous slider, value `100%` in accent,
  active preset highlighted.
- Pitch → unified `−  0 st  +` stepper, mono centred, ±12 range noted.
- Reset workspace → quiet rotate-ccw, low-contrast until hover.
- Monochrome SVG icons only, micro-labelled modules, one accent primary, mono
  tabular readouts, hairline (no pipes), no shadows/multicolour.
- In situ: faint waveform stub + 1px `--line` top border on the strip.
- Reflow: wide (~1000px) primary and narrow (~520px) copy below. Narrow lets the
  hero take a full row, speed module go full-width, divider hide, and reset drop
  inline — proving graceful collapse.

## Trade-offs

This direction **trades density for clarity and labelled structure**. The
micro-labels and per-module spacing cost horizontal room and a little vertical
height versus a tight pro-bar where icons stand alone, so it carries more chrome.
In exchange every control is self-explanatory, the active state of each module is
legible at a glance, and the layout echoes the settings panel the user is already
fluent in. I kept the control height tight (28px) and the label-to-body gap small
(7px) so "structured" never tips into "wasteful" — it remains a control strip,
not a settings page. The hero's lone amber disc preserves the "one accent"
discipline by making play the only saturated element; presets, toggles and
active values use dim-amber tints so nothing competes with the primary action.
