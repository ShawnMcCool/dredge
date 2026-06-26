<script lang="ts">
  // Single slider primitive — webkit2gtk cannot render vertical native range
  // inputs (broken stubs), so we own the slider the way we own Button. Fully
  // custom: track line + fill + square thumb, pointer-driven with capture.
  import { posToValue, valueToPos01 } from "./fader-math";

  interface Props {
    value: number;
    min?: number;
    max?: number;
    step?: number;
    orientation?: "horizontal" | "vertical";
    accent?: boolean;
    disabled?: boolean;
    /** Fires live on every drag/keyboard step. */
    onchange?: (v: number) => void;
    /** Fires once on pointer-release (or keyboard step) — for changes too
     *  expensive to apply continuously, e.g. webview re-zoom. */
    oncommit?: (v: number) => void;
    /** Title/aria text for the current value. */
    format?: (v: number) => string;
  }

  let {
    value = $bindable(),
    min = 0,
    max = 1,
    step = 0.01,
    orientation = "horizontal",
    accent = false,
    disabled = false,
    onchange,
    oncommit,
    format,
  }: Props = $props();

  let root = $state<HTMLDivElement>();

  const vertical = $derived(orientation === "vertical");
  const pos = $derived(valueToPos01(value, min, max));
  const text = $derived(format ? format(value) : String(value));
  // thumb travel is inset by its own 12 px; fill runs to the thumb center
  const fillStyle = $derived(
    `${vertical ? "height" : "width"}: calc((100% - 12px) * ${pos} + 6px)`,
  );
  const thumbStyle = $derived(`${vertical ? "bottom" : "left"}: calc((100% - 12px) * ${pos})`);

  function set(v: number) {
    if (disabled || v === value) return;
    value = v;
    onchange?.(v);
  }

  function fromPointer(e: PointerEvent) {
    if (!root) return;
    const r = root.getBoundingClientRect();
    const pos01 = vertical
      ? 1 - (e.clientY - r.top) / r.height
      : (e.clientX - r.left) / r.width;
    set(posToValue(pos01, min, max, step));
  }

  function onpointerdown(e: PointerEvent) {
    if (disabled || e.button !== 0) return;
    root?.setPointerCapture(e.pointerId);
    fromPointer(e);
  }

  function onpointermove(e: PointerEvent) {
    if (!root?.hasPointerCapture(e.pointerId)) return;
    fromPointer(e);
  }

  function onpointerup(e: PointerEvent) {
    if (!root?.hasPointerCapture(e.pointerId)) return;
    root.releasePointerCapture(e.pointerId);
    oncommit?.(value);
  }

  function onkeydown(e: KeyboardEvent) {
    if (disabled) return;
    // Arrow keys are reserved for global waveform navigation — a focused fader
    // must not swallow them (it used to nudge its value and preventDefault,
    // which killed bar-nav after you touched a slider). Home/End still jump.
    switch (e.key) {
      case "Home":
        set(min);
        break;
      case "End":
        set(max);
        break;
      default:
        return;
    }
    oncommit?.(value);
    e.preventDefault();
  }
</script>

<!-- the whole element is the hit area: press anywhere jumps, then drags -->
<div
  bind:this={root}
  class="fader {orientation}"
  class:accent
  class:disabled
  role="slider"
  tabindex={disabled ? -1 : 0}
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={value}
  aria-valuetext={text}
  aria-orientation={orientation}
  aria-disabled={disabled}
  title={text}
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  {onkeydown}
>
  <div class="track"></div>
  <div class="fill" style={fillStyle}></div>
  <div class="thumb" style={thumbStyle}></div>
</div>

<style>
  .fader {
    position: relative;
    flex: 0 0 auto;
    cursor: pointer;
    touch-action: none;
    border-radius: var(--radius);
  }

  .fader:focus-visible {
    outline: 1px solid var(--accent-dim);
    outline-offset: -1px;
  }

  .fader.disabled {
    cursor: default;
  }

  .horizontal {
    height: var(--control-h);
    min-width: 80px;
    flex: 1 1 auto;
  }

  .vertical {
    width: var(--control-h);
    height: 100%;
  }

  .track,
  .fill {
    position: absolute;
    pointer-events: none;
  }

  .track {
    background: var(--line);
  }

  .fill {
    background: var(--muted);
  }

  .accent .fill {
    background: var(--accent);
  }

  .disabled .fill,
  .disabled.accent .fill {
    background: var(--line);
  }

  .horizontal .track {
    left: 0;
    right: 0;
    top: 50%;
    height: 2px;
    margin-top: -1px;
  }

  .horizontal .fill {
    left: 0;
    top: 50%;
    height: 2px;
    margin-top: -1px;
  }

  .vertical .track {
    top: 0;
    bottom: 0;
    left: 50%;
    width: 2px;
    margin-left: -1px;
  }

  .vertical .fill {
    bottom: 0;
    left: 50%;
    width: 2px;
    margin-left: -1px;
  }

  .thumb {
    position: absolute;
    width: 12px;
    height: 12px;
    background: var(--bg-raised);
    border: 1px solid var(--muted);
    border-radius: var(--radius);
    pointer-events: none;
  }

  .accent .thumb {
    border-color: var(--accent);
  }

  .disabled .thumb {
    border-color: var(--line);
  }

  .horizontal .thumb {
    top: 50%;
    margin-top: -6px;
  }

  .vertical .thumb {
    left: 50%;
    margin-left: -6px;
  }
</style>
