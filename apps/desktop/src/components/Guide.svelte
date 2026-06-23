<script lang="ts">
  // Static reference: keyboard shortcuts + what the concepts mean. Replaces
  // the old one-line KEY_HELP footer. Keep in sync with lib/keys.ts.
  const GROUPS: { title: string; keys: [string, string][] }[] = [
    {
      title: "playback",
      keys: [
        ["space", "play / pause"],
        ["q", "quit immediately"],
        ["r", "restart the current loop (or seek to start)"],
        ["[  ]", "speed −/＋ 5%"],
      ],
    },
    {
      title: "loops & drill",
      keys: [
        ["l", "loop the selection — saves it as a loop & opens the drill"],
        ["d", "arm / disarm the drill tempo trainer"],
        ["del", "delete the current loop"],
      ],
    },
    {
      title: "tone",
      keys: [
        ["b", "toggle bass focus"],
        ["m", "mute the bass stem"],
      ],
    },
    {
      title: "analysis & grid",
      keys: [
        ["a", "analyze track (structure + stems)"],
        ["g", "toggle grid snap"],
      ],
    },
    {
      title: "view",
      keys: [
        ["ctrl + / − / 0", "zoom in / out / reset"],
        ["corner icons", "the ‹ / › icon at the top-left / top-right hides the library / panels — click the thin rail to show it again"],
        ["ctrl [  /  ]", "keyboard shortcut for the same hide / show"],
        [",", "open settings"],
        ["esc", "clear selection / dismiss"],
      ],
    },
    {
      title: "waveform mouse",
      keys: [
        ["left-drag", "select a range · click to seek · click a loop to select, click away to deselect"],
        ["left lane", "drag across section headers · click a header to loop it"],
        ["right-drag", "resize — grabs the nearest loop edge from anywhere (the only resize button); also the scrollbar window edges"],
        ["middle-drag", "zoom into the dragged range · click with no drag to fit"],
        ["wheel", "zoom · shift + wheel to pan"],
      ],
    },
  ];

  const CONCEPTS: [string, string][] = [
    [
      "drill box",
      "Select a loop to open the drill box — a live workbench for that one passage. The tempo trainer (d) ramps the speed up across loop passes (ladder / oscillate / dwell); region toys isolate, nudge, or add a run-up to drill the entrance; and recall mutes the recording so you play a pass from memory. It works a scratch span — your saved loop is never changed; ⟲ snaps back.",
    ],
    [
      "bass focus",
      "Low-passes the mix and shifts it up an octave, so the bassline is easy to hear and transcribe.",
    ],
    [
      "grid snap",
      "Loop and selection edges snap to analyzed downbeats, for clean musical boundaries.",
    ],
    [
      "looping & saving",
      "Drag a selection, then ⟳ (or l) turns it into a saved loop, makes it the active loop, and starts playing — which opens the drill box on it. Saved loops are named automatically from the sections they cover (“verse 2”, “verse 2 → chorus 1”); double-click a name to pin your own, and “fit” snaps a loop's edges to the nearest section boundaries.",
    ],
    [
      "sections",
      "Sections are the song's structure (verse / chorus / inst). Click a section header to play from there; double-click to loop it. Saved loops are named from the sections they cover.",
    ],
    [
      "devices",
      "The devices tab picks the audio output and input. “System default” follows the system; the tuner's input defaults to following the chosen input.",
    ],
  ];

  // The build — what every part of the app is made of. Kept honest against the
  // workspace manifests; update when the stack actually changes.
  const STACK: [string, string][] = [
    ["interface", "Svelte 5 + TypeScript, bundled by Vite, running inside a Tauri 2 webview."],
    ["desktop host", "Rust + Tauri 2. The UI is just another client of one JSON command dispatcher."],
    [
      "audio engine",
      "Rust real-time core: Symphonia decode · Rubber Band R3 pitch-preserving time-stretch · PipeWire output & input · lock-free rtrb ring buffers between the audio and control threads.",
    ],
    [
      "practice core",
      "Rust domain — songs, sections, loops, and analysis — persisted to a single bundled SQLite database via rusqlite.",
    ],
    [
      "analysis",
      "Out-of-process Python in uv-managed venvs: librosa + PyTorch for beats & downbeats, SongFormer (ASLP-lab) for song structure.",
    ],
    [
      "stems",
      "Demucs (htdemucs) splits each track into vocals / drums / bass / other for the mixer and bass focus.",
    ],
  ];
</script>

<h2>guide</h2>

{#each GROUPS as g (g.title)}
  <h3 class="grp mono">{g.title}</h3>
  <ul class="keys">
    {#each g.keys as [k, desc] (k)}
      <li><kbd class="mono">{k}</kbd><span class="desc">{desc}</span></li>
    {/each}
  </ul>
{/each}

<h3 class="grp mono">concepts</h3>
<dl class="concepts">
  {#each CONCEPTS as [term, desc] (term)}
    <dt>{term}</dt>
    <dd>{desc}</dd>
  {/each}
</dl>

<h3 class="grp mono">colophon</h3>
<dl class="concepts">
  {#each STACK as [area, desc] (area)}
    <dt>{area}</dt>
    <dd>{desc}</dd>
  {/each}
</dl>
<p class="sig">
  One Rust core, two front ends — this Tauri desktop app and a headless daemon
  (<span class="mono">dredged</span>) — over a single command surface. Ear-first practice,
  built for Linux.
</p>

<style>
  .grp {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--accent);
    margin: var(--space) 0 calc(var(--space) / 2);
  }

  .keys {
    display: flex;
    flex-direction: column;
    gap: calc(var(--space) / 2);
  }

  .keys li {
    display: flex;
    align-items: baseline;
    gap: var(--space);
    min-width: 0;
  }

  kbd {
    flex: 0 0 auto;
    min-width: 6.5em;
    font-size: 11px;
    color: var(--fg);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 1px 5px;
    text-align: center;
  }

  .desc {
    font-size: 12px;
    color: var(--muted);
    min-width: 0;
  }

  .concepts {
    margin: 0;
  }

  .concepts dt {
    font-size: 12px;
    color: var(--fg);
    margin-top: var(--space);
  }

  .concepts dd {
    margin: 2px 0 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--muted);
  }

  .sig {
    margin: var(--space) 0 0;
    font-size: 11px;
    line-height: 1.6;
    color: var(--muted);
    border-top: 1px solid var(--line);
    padding-top: var(--space);
    max-width: 64ch;
  }
</style>
