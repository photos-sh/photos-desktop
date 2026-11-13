<script lang="ts">
  import { extend, mark, paint, radiusFor, toPhoto, toPixel, type Size } from "../lib/brush";
  import type { Point, Stroke } from "../lib/types";

  interface Props {
    /** The picture in effect. */
    current: string;
    /** The original, to compare against; none when the original is showing. */
    original: string | null;
    /** The current picture's size in its own pixels, when the core knows it. */
    size: Size | null;
    /** Painting a mask instead of comparing. */
    painting: boolean;
    strokes: Stroke[];
    /** The brush's diameter, in percent of the picture's longer side. */
    brushPercent: number;
    onstroke: (stroke: Stroke) => void;
    /** Clicking points on the picture instead of comparing. */
    pointing?: boolean;
    /** The points clicked so far, in the picture's own pixels. */
    points?: Point[];
    onpoint?: (point: Point) => void;
    disabled?: boolean;
  }

  let {
    current,
    original,
    size,
    painting,
    strokes,
    brushPercent,
    onstroke,
    pointing = false,
    points = [],
    onpoint,
    disabled = false,
  }: Props = $props();

  let split = $state(50);
  let stage = $state<HTMLDivElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let stageSize = $state<Size>({ width: 0, height: 0 });
  let natural = $state<{ src: string; size: Size } | null>(null);
  let failed = $state<string | null>(null);
  let originalFailed = $state<string | null>(null);
  let live = $state<Stroke | null>(null);
  let pointer = $state<{ x: number; y: number } | null>(null);

  // The picture's size: what the core says, else what the web view loaded.
  let photo = $derived(size ?? (natural?.src === current ? natural.size : null));

  // The picture's box on screen: fitted inside the stage, never enlarged
  // beyond its own pixels by more than the stage allows.
  let shown = $derived.by(() => {
    if (photo === null || stageSize.width === 0 || stageSize.height === 0) return null;
    const scale = Math.min(stageSize.width / photo.width, stageSize.height / photo.height);
    return { width: Math.max(1, Math.floor(photo.width * scale)), height: Math.max(1, Math.floor(photo.height * scale)) };
  });

  $effect(() => {
    if (stage === null) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) stageSize = { width: entry.contentRect.width, height: entry.contentRect.height };
    });
    observer.observe(stage);
    return () => observer.disconnect();
  });

  // Draw the mask painted so far, or the points clicked.
  $effect(() => {
    if (canvas === null || shown === null || photo === null) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(shown.width * ratio);
    canvas.height = Math.round(shown.height * ratio);
    const context = canvas.getContext("2d");
    if (context === null) return;
    const style = getComputedStyle(canvas);
    const scale = canvas.width / photo.width;
    if (pointing) {
      mark(context, points, scale, style.getPropertyValue("--ps-accent").trim() || "#ffb000", 6 * ratio);
    } else {
      const colour = style.getPropertyValue("--ps-mask").trim() || "rgb(255 176 0 / 55%)";
      paint(context, live ? [...strokes, live] : strokes, scale, colour);
    }
  });

  function loaded(event: Event) {
    const image = event.currentTarget as HTMLImageElement;
    natural = { src: current, size: { width: image.naturalWidth, height: image.naturalHeight } };
  }

  function point(event: PointerEvent): [number, number] | null {
    if (canvas === null || photo === null) return null;
    return toPhoto(event.clientX, event.clientY, canvas.getBoundingClientRect(), photo);
  }

  function down(event: PointerEvent) {
    if (disabled || photo === null || event.button !== 0) return;
    const at = point(event);
    if (at === null) return;
    if (pointing) {
      // A click is a point; there is nothing to drag.
      onpoint?.(toPixel(at, photo));
      return;
    }
    canvas?.setPointerCapture(event.pointerId);
    live = { radius: radiusFor(brushPercent, photo), points: [at] };
  }

  function move(event: PointerEvent) {
    const box = canvas?.getBoundingClientRect();
    if (box) pointer = { x: event.clientX - box.left, y: event.clientY - box.top };
    if (live === null) return;
    const at = point(event);
    if (at !== null) live = extend(live, at);
  }

  function up() {
    if (live !== null) onstroke(live);
    live = null;
  }

  let ring = $derived(
    photo !== null && shown !== null ? (radiusFor(brushPercent, photo) * 2 * shown.width) / photo.width : 0,
  );
</script>

<div class="stage" bind:this={stage}>
  {#if failed === current}
    <p class="stage__message muted">
      This format cannot be shown here. The tools still work on it, and their results are shown.
    </p>
  {:else if painting || pointing}
    <div class="frame checker" style:width={shown ? `${shown.width}px` : undefined} style:height={shown ? `${shown.height}px` : undefined}>
      <img
        src={current}
        alt={pointing ? "Click the object to cut out" : "Paint over what should be erased"}
        draggable="false"
        onload={loaded}
        onerror={() => (failed = current)}
      />
      <canvas
        bind:this={canvas}
        class:canvas--off={disabled}
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}
        onpointerleave={() => (pointer = null)}
      ></canvas>
      {#if pointer !== null && !disabled && !pointing}
        <span class="ring" style:width="{ring}px" style:height="{ring}px" style:left="{pointer.x}px" style:top="{pointer.y}px"></span>
      {/if}
    </div>
  {:else}
    <div class="compare checker" style:width={shown ? `${shown.width}px` : undefined} style:height={shown ? `${shown.height}px` : undefined}>
      <img
        src={current}
        alt={original === null ? "Original" : "Edited"}
        draggable="false"
        onload={loaded}
        onerror={() => (failed = current)}
      />
      {#if original !== null && originalFailed !== original}
        <img
          class="before"
          src={original}
          alt="Original, for comparison"
          draggable="false"
          style:clip-path="inset(0 {100 - split}% 0 0)"
          onerror={() => (originalFailed = original)}
        />
        <span class="divider" style:left="{split}%" aria-hidden="true"><span class="grip"></span></span>
        <span class="tag tag--before" aria-hidden="true">Before</span>
        <span class="tag tag--after" aria-hidden="true">After</span>
        <input
          class="slider"
          type="range"
          min="0"
          max="100"
          step="1"
          bind:value={split}
          aria-label="Before and after: move to compare with the original"
        />
      {/if}
    </div>
  {/if}
</div>

<style>
  .stage {
    position: relative;
    display: grid;
    width: 100%;
    height: 100%;
    min-height: 0;
    place-items: center;
    overflow: hidden;
  }
  .stage__message {
    max-width: 24rem;
    padding: var(--ps-space-6);
    text-align: center;
  }
  .frame,
  .compare {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    overflow: hidden;
    border-radius: 4px;
    box-shadow: var(--ps-shadow);
  }
  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .before {
    position: absolute;
    inset: 0;
    background: var(--ps-surface);
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    cursor: crosshair;
    touch-action: none;
  }
  .canvas--off {
    cursor: progress;
  }
  .ring {
    position: absolute;
    border: 1.5px solid #fff;
    border-radius: 50%;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 60%);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }
  .divider {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 35%);
    pointer-events: none;
  }
  .grip {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 1.75rem;
    height: 1.75rem;
    border: 2px solid #fff;
    border-radius: 50%;
    background: var(--ps-accent);
    box-shadow: 0 0 0 1px rgb(0 0 0 / 35%);
    transform: translate(-50%, -50%);
  }
  .tag {
    position: absolute;
    top: 0.5rem;
    padding: 0.1rem 0.5rem;
    border-radius: var(--ps-radius-pill);
    background: rgb(20 21 24 / 70%);
    color: #fff;
    font-size: 0.6875rem;
    font-weight: 600;
    pointer-events: none;
  }
  .tag--before {
    left: 0.5rem;
  }
  .tag--after {
    right: 0.5rem;
  }
  /* The whole picture is the slider: drag anywhere, or use the arrow keys. */
  .slider {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: ew-resize;
  }
  .compare:has(.slider:focus-visible) {
    outline: 2px solid var(--ps-focus);
    outline-offset: 2px;
  }
</style>
