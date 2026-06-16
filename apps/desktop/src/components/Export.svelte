<script lang="ts">
  // Export tab (receipt-first): render the open song to disk, baking in the
  // *current* mix — stem gains, speed, pitch, bass focus — for the whole song,
  // the active loop, or the current selection. Master volume is deliberately
  // not part of the export (it's a monitoring level, not a mix decision). The
  // receipt card up top shows exactly what will land on disk before you commit.
  import { onDestroy, onMount } from "svelte";
  import { pickExportDir } from "../lib/file-picker";
  import { fmtBytes, fmtDur } from "../lib/format";
  import { cmd, onEvent } from "../lib/ipc";
  import {
    actions,
    activeLoop,
    bassFocus,
    EXPORT_DIR,
    EXPORT_FORMAT,
    openSong,
    pitch,
    position,
    selection,
    settings,
    stemMix,
    STEM_LABELS,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";

  type Scope = "all" | "loop" | "selection";
  type Phase = "idle" | "rendering" | "done" | "failed" | "cancelled";

  // --- local, ephemeral UI state -------------------------------------------
  let scope = $state<Scope>("all");
  let format = $state<"wav" | "mp3">("wav");
  let dir = $state("");
  let filename = $state("");
  let caps = $state<{ mp3: boolean }>({ mp3: false });

  let phase = $state<Phase>("idle");
  let stage = $state(""); // "decoding" | "rendering" | "encoding"
  let percent = $state(0);
  let resultPath = $state("");
  let resultBytes = $state(0);
  let errorMsg = $state("");

  // restore the persisted dir/format once settings have loaded
  $effect(() => {
    const d = $settings[EXPORT_DIR];
    if (typeof d === "string" && !dir) dir = d;
    const f = $settings[EXPORT_FORMAT];
    if ((f === "wav" || f === "mp3") && format !== f) format = f;
  });

  // MP3 only when the backend reports an encoder; never offer-then-fail.
  onMount(() => {
    void cmd<{ mp3: boolean }>("export.caps").then((c) => {
      caps = c;
      if (!c.mp3 && format === "mp3") format = "wav";
    });
    const unlisten = onEvent((e) => {
      if (e.event !== "export_progress") return;
      const d = e.data ?? {};
      switch (d.state) {
        case "decoding":
        case "encoding":
          phase = "rendering";
          stage = d.state;
          break;
        case "rendering":
          phase = "rendering";
          stage = "rendering";
          percent = d.percent ?? 0;
          break;
        case "done":
          phase = "done";
          resultPath = d.path ?? "";
          resultBytes = d.bytes ?? 0;
          break;
        case "failed":
          phase = "failed";
          errorMsg = d.error ?? "export failed";
          break;
        case "cancelled":
          phase = "idle";
          break;
      }
    });
    onDestroy(() => void unlisten.then((f) => f()));
  });

  // --- derived: what's being exported --------------------------------------
  const loopAvail = $derived(!!$activeLoop);
  const selAvail = $derived(!!$selection);
  // fall back to whole-song if the chosen scope's span vanished
  const effScope = $derived<Scope>(
    (scope === "loop" && !loopAvail) || (scope === "selection" && !selAvail) ? "all" : scope,
  );

  interface Span {
    start: number | null;
    end: number | null;
    dur: number;
  }
  const span = $derived<Span>(spanFor(effScope));
  function spanFor(s: Scope): Span {
    const songDur = $openSong?.song.duration_secs ?? 0;
    if (s === "loop" && $activeLoop) {
      return { start: $activeLoop.start, end: $activeLoop.end, dur: $activeLoop.end - $activeLoop.start };
    }
    if (s === "selection" && $selection) {
      return { start: $selection.start, end: $selection.end, dur: $selection.end - $selection.start };
    }
    return { start: null, end: null, dur: songDur };
  }

  const rate = $derived($position.rate || 1);
  const outSecs = $derived(span.dur / rate);
  // wav = 48k · stereo · 16-bit (4 B/frame); mp3 ≈ 320 kbps
  const estBytes = $derived(
    format === "mp3" ? Math.round(outSecs * (320_000 / 8)) : Math.round(outSecs * 48_000 * 4),
  );

  const gains = $derived($openSong?.stems ? actions.stemGainsVector($stemMix) : []);

  const bakeLine = $derived(buildBakeLine());
  function buildBakeLine(): string {
    const parts: string[] = [];
    if (rate !== 1) parts.push(`${rate}× speed`);
    const { semitones, cents, octaveUp } = $pitch;
    if (semitones || cents || octaveUp) {
      const st = semitones ? `${semitones > 0 ? "+" : ""}${semitones} st` : "";
      const oct = octaveUp ? "+1 oct" : "";
      parts.push([st, oct].filter(Boolean).join(" "));
    }
    if ($bassFocus) parts.push("bass focus");
    if ($openSong?.stems) {
      STEM_LABELS.forEach((label, i) => {
        const g = gains[i] ?? 1;
        const lvl = $stemMix.levels[i];
        if (g === 0) parts.push(`${label.toLowerCase()} off`);
        else if (lvl !== 100) parts.push(`${label.toLowerCase()} ${lvl}%`);
      });
    }
    return parts.length ? parts.join(" · ") : "original mix";
  }

  // default filename, seeded per song; user edits stick until the song changes
  let seededFor = $state<number | null>(null);
  $effect(() => {
    const song = $openSong?.song;
    if (song && seededFor !== song.id) {
      seededFor = song.id;
      let name = song.title || "export";
      if (rate !== 1) name += ` ${rate}x`;
      if ($bassFocus) name += " bass";
      filename = name;
    }
  });

  // --- actions --------------------------------------------------------------
  async function chooseDir() {
    const chosen = await pickExportDir();
    if (chosen) {
      dir = chosen;
      void actions.setSetting(EXPORT_DIR, chosen);
    }
  }

  function setFormat(f: "wav" | "mp3") {
    format = f;
    void actions.setSetting(EXPORT_FORMAT, f);
  }

  const canExport = $derived(
    !!$openSong && !!dir.trim() && !!filename.trim() && phase !== "rendering",
  );

  async function startExport() {
    if (!$openSong || !canExport) return;
    phase = "rendering";
    stage = "decoding";
    percent = 0;
    errorMsg = "";
    await cmd("export.start", {
      song_id: $openSong.song.id,
      dir,
      filename: filename.trim(),
      format,
      start_secs: span.start,
      end_secs: span.end,
      rate,
      semitones: $pitch.semitones,
      cents: $pitch.cents,
      octave_up: $pitch.octaveUp,
      bass_focus: $bassFocus,
      gains,
    });
  }

  function cancelExport() {
    void cmd("export.cancel");
  }

  function reset() {
    phase = "idle";
  }

  function filenameOf(path: string): string {
    const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return i >= 0 ? path.slice(i + 1) : path;
  }
</script>

<h2>export</h2>

{#if !$openSong}
  <p class="empty mono">open a song to export it</p>
{:else if phase === "rendering"}
  <div class="state">
    <div class="line">
      {stage === "decoding" ? "reading audio…" : stage === "encoding" ? "encoding mp3…" : "rendering…"}
    </div>
    <div class="progress"><i style="width:{percent}%"></i></div>
    <div class="state-foot">
      <span class="readout pct">{percent}%</span>
      <Button onclick={cancelExport}>cancel</Button>
    </div>
  </div>
{:else if phase === "done"}
  <div class="state">
    <div class="line"><span class="ok">✓</span> exported</div>
    <div class="readout filemeta">{filenameOf(resultPath)} · {fmtBytes(resultBytes)}</div>
    <div class="state-foot">
      <Button onclick={reset}>export again</Button>
    </div>
  </div>
{:else if phase === "failed"}
  <div class="state">
    <div class="line"><span class="bad">✕</span> export failed</div>
    <p class="error">{errorMsg}</p>
    <div class="state-foot">
      <Button onclick={reset}>back</Button>
    </div>
  </div>
{:else}
  <!-- receipt card: what lands on disk, reacting to scope/format/mix -->
  <div class="receipt">
    <div class="head">
      <span class="dur readout">{fmtDur(outSecs, true)}</span>
      <span class="fmt">{format.toUpperCase()}</span>
      <span class="size readout">~{fmtBytes(estBytes)}</span>
    </div>
    <div class="bake">{bakeLine}</div>
    <div class="bake muted">master volume not applied</div>
  </div>

  <section class="group">
    <div class="group-head">scope</div>
    <button class="radio" class:sel={effScope === "all"} onclick={() => (scope = "all")}>
      <span class="dot"></span>
      <span class="lbl">whole song</span>
      <span class="meta readout">{fmtDur($openSong.song.duration_secs, true)}</span>
    </button>
    <button
      class="radio"
      class:sel={effScope === "loop"}
      disabled={!loopAvail}
      onclick={() => (scope = "loop")}
    >
      <span class="dot"></span>
      <span class="lbl">active loop {#if loopAvail && $activeLoop}<span class="meta">· {$activeLoop.name}</span>{/if}</span>
      <span class="meta readout">{loopAvail && $activeLoop ? fmtDur($activeLoop.end - $activeLoop.start, true) : "none"}</span>
    </button>
    <button
      class="radio"
      class:sel={effScope === "selection"}
      disabled={!selAvail}
      onclick={() => (scope = "selection")}
    >
      <span class="dot"></span>
      <span class="lbl">current selection</span>
      <span class="meta readout">{selAvail && $selection ? fmtDur($selection.end - $selection.start, true) : "none"}</span>
    </button>
  </section>

  <section class="group">
    <div class="group-head">format</div>
    <div class="chips">
      <Button variant="chip" active={format === "wav"} onclick={() => setFormat("wav")}>WAV</Button>
      <Button variant="chip" active={format === "mp3"} disabled={!caps.mp3} onclick={() => setFormat("mp3")}>
        MP3
      </Button>
    </div>
    {#if !caps.mp3}
      <p class="hint">MP3 needs ffmpeg — not found. WAV always works.</p>
    {/if}
  </section>

  <section class="group">
    <div class="group-head">destination</div>
    <div class="field">
      <label for="ex-name">file name</label>
      <input id="ex-name" class="txt" bind:value={filename} spellcheck="false" />
    </div>
    <div class="field">
      <label for="ex-dir">folder</label>
      <div class="dirrow">
        <input id="ex-dir" class="txt" bind:value={dir} placeholder="choose a folder…" spellcheck="false" />
        <button class="pick" onclick={chooseDir} title="choose folder">…</button>
      </div>
    </div>
  </section>

  <button class="export-btn" disabled={!canExport} onclick={startExport}>Export</button>
{/if}

<style>
  .empty {
    font-size: 12px;
    color: var(--muted);
  }

  /* receipt card — the confidence hero */
  .receipt {
    border: 1px solid var(--accent-dim);
    border-radius: var(--radius);
    background: var(--bg-raised);
    padding: 10px 12px;
    margin-bottom: calc(var(--space) * 1.5);
  }
  .receipt .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 6px;
  }
  .receipt .dur {
    font-size: 20px;
    color: var(--fg);
  }
  .receipt .fmt {
    font-size: 12px;
    letter-spacing: 0.04em;
    color: var(--accent);
  }
  .receipt .size {
    margin-left: auto;
    font-size: 12px;
    color: var(--muted);
  }
  .bake {
    font-size: 12px;
    color: var(--fg);
    line-height: 1.5;
  }
  .bake.muted {
    color: var(--muted);
    font-size: 11px;
  }

  .group {
    margin-bottom: calc(var(--space) * 2);
  }
  .group-head {
    margin: 0 0 calc(var(--space) / 2);
    padding-bottom: 6px;
    border-bottom: 1px solid var(--line);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }

  /* radios — reuse the .setting rhythm */
  .radio {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 0;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    color: var(--fg);
  }
  .radio:disabled {
    cursor: default;
  }
  .radio .dot {
    flex: 0 0 auto;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    border: 1px solid var(--muted);
    position: relative;
  }
  .radio.sel .dot {
    border-color: var(--accent);
  }
  .radio.sel .dot::after {
    content: "";
    position: absolute;
    inset: 3px;
    border-radius: 50%;
    background: var(--accent);
  }
  .radio .lbl {
    flex: 1;
    font-size: 13px;
  }
  .radio .meta {
    font-size: 11px;
    color: var(--muted);
  }
  .radio:disabled .lbl,
  .radio:disabled .dot {
    opacity: 0.4;
  }

  .chips {
    display: flex;
    gap: 4px;
  }
  .hint {
    font-size: 11px;
    color: var(--muted);
    margin: 8px 0 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 6px 0;
  }
  .field label {
    font-size: 11px;
    color: var(--muted);
  }
  .txt {
    font: inherit;
    font-size: 13px;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 6px 8px;
    width: 100%;
  }
  .dirrow {
    display: flex;
    gap: 4px;
  }
  .dirrow .txt {
    flex: 1;
  }
  .pick {
    flex: 0 0 auto;
    padding: 0 10px;
  }

  .export-btn {
    width: 100%;
    margin-top: 4px;
    padding: 9px;
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    color: var(--bg);
    background: var(--accent);
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    cursor: pointer;
  }
  .export-btn:hover {
    filter: brightness(1.08);
  }
  .export-btn:disabled {
    color: var(--muted);
    background: var(--bg-raised);
    border-color: var(--line);
    cursor: default;
    filter: none;
  }

  /* rendering / done / failed states */
  .state {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .state .line {
    font-size: 13px;
    color: var(--fg);
  }
  .ok {
    color: var(--solid);
  }
  .bad {
    color: var(--miss);
  }
  .filemeta {
    font-size: 12px;
    color: var(--muted);
  }
  .progress {
    height: 6px;
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: 99px;
    overflow: hidden;
  }
  .progress > i {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 120ms linear;
  }
  .state-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space);
  }
  .pct {
    font-size: 12px;
    color: var(--muted);
  }
  .error {
    font-size: 12px;
    color: var(--miss);
    margin: 0;
  }
</style>
