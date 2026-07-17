# Campaign: Foot-pedal control

Drive dredge hands-free from a Bluetooth foot pedal, so a passage can be
looped / restarted / play-paused / stepped bar-to-bar with a guitar in both
hands. Researched 2026-07-14. **Status: superseded by a design spec —
see Outcome below.** Work directly on `main`.

## Outcome (2026-07-17)

The pedal arrived and works over USB in **U (USB device) mode**: ALSA MIDI
client `SINCO` (USB `4353:4b4d`), four switches sending Program Change 0–3 on
channel 0, press-only. Six assignable buttons total (two-switch combos act as
virtual buttons). The requirements then changed: Shawn wants assignments
customizable *inside dredge*, including actions with no keyboard shortcut
(play-from-marker, isolation-snapshot switching). That killed both paths
below — this is now a native dredge feature (markers + isolation snapshots +
a MIDI listener/mapping in the server crate), designed in
`docs/superpowers/specs/2026-07-17-pedal-control-design.md`. Paths A/B are
kept as history only.

## Why the Spark Control was rejected (don't revisit)

The original ask was to use a **Positive Grid Spark Control** foot pedal. It was
ruled out after research:

- It is a **proprietary BLE peripheral** that pairs to a phone/tablet and only
  functions with the **Spark app running** — it does *not* present as a standard
  BLE HID keyboard or BLE-MIDI device to a host.
- Its footswitch BLE protocol is **undocumented** (unlike the Spark *amp*, which
  the Soundshed / paulhamsh community reverse-engineered — the footswitch is not
  covered).
- Using it with a PC would require full BLE reverse-engineering (HCI snoop from
  an Android phone running the Spark app → Wireshark → decode the notify
  characteristic → a BlueZ/D-Bus bridge), with real risk it won't even hold a
  connection without the app as BLE central, and firmware updates could break it.

Verdict: **wrong tool.** Shawn owns one but chose to buy a proper pedal instead.

## The pedal (ordered)

**M-Vave Chocolate Plus** — 4 footswitches, Bluetooth, rechargeable, ~€50–60.
Amazon.nl ASIN `B0DSFWSD9M`. Chosen because the **Plus** has a true **HID
keyboard mode** (assign custom keys per switch, modifiers supported) *in addition
to* MIDI. That makes it the zero-code route: it enrols as a BLE keyboard and
drives dredge's existing shortcuts directly.

- **The one thing to verify on arrival:** confirm the unit is the **Plus** with
  an **HID / keyboard / page-turner mode**, not the MIDI-only original Chocolate.
  (The listing title dropped "Plus"; the ASIN is a Plus listing.) If it turns out
  to be MIDI-only, do **not** return it — run the MIDI-bridge path below instead.
- Config is one-time from the **M-Vave / Cuvave phone app** (iOS/Android). Their
  config app is not on Linux, but assignments persist onboard, so after the
  one-time setup Linux just sees a keyboard (or a MIDI device).

## Control surfaces this rides (verified 2026-07-14)

dredge exposes two seams a pedal can hit; both already exist, unchanged:

| Seam | Mechanism | Location |
|------|-----------|----------|
| **Keyboard-first shortcuts** | global keydown handler, focus-aware | `apps/desktop/src/lib/keys.ts` |
| — play/pause | `Space` → `actions.play`/`pause` | `keys.ts:149` |
| — step a bar (own accel hold-repeat on keydown→keyup, ignores OS auto-repeat) | `ArrowLeft`/`ArrowRight` → `startNav` | `keys.ts:155,159,99` |
| — restart the loop/song (jump to loop start) | `r` | `keys.ts:163` |
| — rate slower/faster | `[` / `]` | `keys.ts:174,177` |
| — loop the selection / drill / mute bass / bass focus | `l` / `d` / `m` / `b` | `keys.ts:180,212,219,205` |
| **Socket command dispatcher** (JSON-lines) | `$XDG_RUNTIME_DIR/dredge.sock`; `just cmd '{"id":1,"cmd":"pause"}'` | `crates/server/src/socket.rs`, `app.rs` |

Because dredge is keyboard-first, an HID-keyboard pedal needs **no dredge code at
all**. The socket seam is the fallback for the MIDI route (bridge = another
client, matching the "one dispatch surface, many clients" design).

## Key mapping (agreed)

Four switches, chosen for loop-practice with hands full:

| Switch | Key | dredge action | Mode |
|--------|-----|---------------|------|
| SW1 | `Space` | play / pause | Pulse (single tap) |
| SW2 | `r` | jump to loop start (restart the passage) | Pulse |
| SW3 | `Left Arrow` | step back one bar | **Momentary** |
| SW4 | `Right Arrow` | step forward one bar | **Momentary** |

**Momentary tip:** dredge runs its *own* accelerating hold-repeat on the arrow
keys (`keys.ts:99`, keydown→keyup, OS auto-repeat ignored). A switch set to
Momentary holds the arrow down while your foot is down → **hold to fast-scan
bars, tap to nudge one**. Set SW3/SW4 Momentary, SW1/SW2 Pulse.

Reassign freely — `m` (mute the recorded bass, the "one-key move"), `d` (drill),
`b` (bass focus), `l` (loop selection) are all single keys if a different set
suits.

## Path A — HID keyboard (expected; zero dredge code)

Run this if the pedal confirms HID/keyboard mode.

### Phase A1 — Assign keys (one-time, phone)
In the M-Vave/Cuvave app, HID/keyboard mode: SW1 `Space`, SW2 `r`,
SW3 `Left Arrow` (Momentary), SW4 `Right Arrow` (Momentary).
**Gate:** the app shows the four assignments saved to the pedal.

### Phase A2 — Pair on Arch
`bluetoothctl` → `scan on` → `pair <mac>` → `trust <mac>` → `connect <mac>`.
**Gate:** the pedal shows as a connected input device.

### Phase A3 — Verify the events
`sudo libinput debug-events` (or `wev`): stomp each switch, confirm
`space / r / Left / Right` (and that Momentary switches hold while pressed).
**Gate:** all four keys fire; hold-to-scan works on the arrows.

### Phase A4 — Drive dredge; document
Focus dredge, run through play/pause, restart, bar-scan against a real loop.
Write the setup (assignments + pairing + verify) into `docs/` so it survives.
**Gate:** all four actions work in the running app; setup note committed.

## Path B — BLE-MIDI only (fallback; small bridge daemon)

Run this only if the pedal turns out to be MIDI-only.

The pedal can't drive dredge directly, so add a **bridge**: read the pedal's MIDI
over ALSA → post JSON to `$XDG_RUNTIME_DIR/dredge.sock`. ~40 lines. A proper
dredge client, not a core change. Reference: `varlen/pyfootctrl` (Python
companion for the M-Vave Chocolate; confirms Linux/ALSA works).

### Phase B1 — Map the pedal's MIDI
Pair over BLE-MIDI; `aseqdump` (or equivalent) to learn each switch's
note/CC + on/off values.
**Gate:** each of the four switches' MIDI messages documented.

### Phase B2 — Bridge daemon
Small daemon (Rust or Python): subscribe to the pedal's MIDI, translate each
switch → a dredge command (`play`/`pause` toggle, loop-restart via
`setTransportLoop`+`seek`, bar step via a seek to the prev/next downbeat), write
JSON lines to the socket. Momentary/hold handling for bar-scan if wanted.
**Gate:** each switch triggers the intended dredge command over the socket;
unit-test the MIDI→command mapping.

### Phase B3 — systemd user service + document
Package as a `--user` systemd service that starts with the session; document.
**Gate:** service auto-starts, reconnects on pedal wake; setup note committed.

## Decisions (final)

- **Touch dredge as little as possible.** The pedal rides existing seams. Path A
  is zero dredge code; Path B adds a standalone client, never core changes.
- **HID over MIDI when available.** Fewer moving parts (no daemon to keep
  running), so the Plus's HID mode is the default plan.
- **4 switches, loop-practice mapping.** play/pause + restart + two-way bar scan
  is the core; hold-to-scan via Momentary is the one nicety worth configuring.

## Deferred / open

- Extra actions beyond four switches (bank/layer switching, `m`/`d`/`b`/`l` on a
  second bank) — only if the four prove limiting.
- A pedal action dredge has *no* shortcut for → would justify either a new
  `keys.ts` binding or the socket bridge; none identified yet.
- Reconnection robustness / auto-reconnect on Bluetooth wake — handle if it
  bites in daily use.
