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
      title: "loops & practice",
      keys: [
        ["l", "make a loop from the selection"],
        ["p", "quick-practice the selection"],
        ["del", "delete the current loop"],
        ["1 / 2 / 3", "rate a rep: miss / shaky / solid"],
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
        ["ctrl [  /  ]", "collapse library / panels"],
        ["corner click", "far top-left / top-right toggles the library / panels column"],
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
      "quick practice",
      "Select a span on the waveform and press p — a zero-ceremony micro-session: it plays, you rate it 1/2/3, and the loop enters the spaced-repetition schedule.",
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
      "sections & transitions",
      "Sections are the song's structure (verse / chorus). Transition loops are auto-derived at section boundaries so you can drill the changes between them.",
    ],
    [
      "ratings",
      "miss / shaky / solid describe how a rep felt — they drive when each loop resurfaces in the due queue.",
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
</style>
