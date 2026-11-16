<script lang="ts">
  import { explain } from "../lib/errors";
  import { FORMATS, MAX_MARK_TEXT, POSITIONS, SHAPES, choice, usesOutput, type Draft } from "../lib/operations";
  import type { AppError, Catalog } from "../lib/types";
  import Slider from "./Slider.svelte";

  interface Props {
    draft: Draft;
    catalog: Catalog | null;
    catalogError: AppError | null;
    disabled?: boolean;
    /** Makes the fields' ids unique when two forms are on screen. */
    prefix: string;
  }

  let { draft = $bindable(), catalog, catalogError, disabled = false, prefix }: Props = $props();

  let listProblem = $derived(
    catalogError
      ? explain(catalogError).text
      : catalog === null
        ? "The list comes from the server: add your API key in Settings to load it."
        : null,
  );
  let preset = $derived(catalog?.presets.find((p) => p.name === draft.preset));
  let warning = $derived(choice(draft.kind).warning);
</script>

<fieldset class="form stack" {disabled}>
  <legend class="visually-hidden">Options</legend>

  {#if warning}
    <p class="caution tone-warning small selectable">{warning}</p>
  {/if}

  {#if draft.kind === "remove_background"}
    <div class="field">
      <label class="check">
        <input type="checkbox" aria-describedby="{prefix}-studio-help" bind:checked={draft.studio} /> Studio shot
      </label>
      <span class="help" id="{prefix}-studio-help">
        Product on a plain light backdrop: keep the box or lid shown with it.
      </span>
    </div>
    <div class="field">
      <span class="label" id="{prefix}-bg-label">Background</span>
      <div class="row row--wrap" role="radiogroup" aria-labelledby="{prefix}-bg-label">
        <label class="check">
          <input type="radio" name="{prefix}-bg" value="transparent" bind:group={draft.background} /> Transparent
        </label>
        <label class="check">
          <input type="radio" name="{prefix}-bg" value="white" bind:group={draft.background} /> White
        </label>
        <label class="check">
          <input type="radio" name="{prefix}-bg" value="colour" bind:group={draft.background} /> A colour
        </label>
        {#if draft.background === "colour"}
          <input class="input" type="color" aria-label="Background colour" bind:value={draft.colour} />
        {/if}
      </div>
    </div>
    <label class="check">
      <input type="checkbox" bind:checked={draft.crop} /> Crop to the product
    </label>
    {#if draft.crop}
      <div class="field">
        <label for="{prefix}-padding">Margin around it (pixels)</label>
        <input
          class="input"
          id="{prefix}-padding"
          type="number"
          min="0"
          step="1"
          placeholder="0"
          bind:value={draft.padding}
        />
      </div>
    {/if}
    <div class="field">
      <label for="{prefix}-shadow">Shadow under the product</label>
      <select class="select" id="{prefix}-shadow" bind:value={draft.shadow}>
        <option value="none">None</option>
        <option value="contact">Contact: soft, at its base</option>
        <option value="drop">Drop: its outline, offset and blurred</option>
      </select>
      {#if draft.shadow !== "none"}
        <span class="help">The shadow is cut off at the photo's edge. When cropping, leave a margin for it.</span>
      {/if}
    </div>
  {:else if draft.kind === "upscale"}
    <div class="field">
      <span class="label" id="{prefix}-scale-label">Enlarge</span>
      <div class="row" role="radiogroup" aria-labelledby="{prefix}-scale-label">
        {#each [2, 3, 4] as scale (scale)}
          <label class="check">
            <input type="radio" name="{prefix}-scale" value={scale} bind:group={draft.scale} />
            {scale}×
          </label>
        {/each}
      </div>
    </div>
  {:else if draft.kind === "remove_text"}
    <div class="field">
      <label for="{prefix}-region">Remove</label>
      <select class="select" id="{prefix}-region" bind:value={draft.region}>
        <option value="background">Text beside the product (watermarks, captions, prices)</option>
        <option value="all">All text, labels on the product too</option>
      </select>
    </div>
    <div class="field">
      <label for="{prefix}-matching">Only text matching (optional)</label>
      <input
        class="input mono"
        id="{prefix}-matching"
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="sale|€|www\."
        aria-describedby="{prefix}-matching-help"
        bind:value={draft.matching}
      />
      <span class="help" id="{prefix}-matching-help">
        A regular expression, upper or lower case alike. Leave empty to remove all text found.
      </span>
    </div>
  {:else if draft.kind === "resize"}
    <div class="grid-2">
      <div class="field">
        <label for="{prefix}-width">Width (pixels)</label>
        <input class="input" id="{prefix}-width" type="number" min="1" step="1" placeholder="auto" bind:value={draft.width} />
      </div>
      <div class="field">
        <label for="{prefix}-height">Height (pixels)</label>
        <input class="input" id="{prefix}-height" type="number" min="1" step="1" placeholder="auto" bind:value={draft.height} />
      </div>
    </div>
    <div class="field">
      <label for="{prefix}-fit">When both are given</label>
      <select class="select" id="{prefix}-fit" bind:value={draft.fit}>
        <option value="contain">Fit inside, keeping the shape</option>
        <option value="cover">Fill the size, cropping the overflow</option>
        <option value="fill">Stretch to the size</option>
      </select>
    </div>
    <label class="check">
      <input type="checkbox" bind:checked={draft.enlarge} /> Allow enlarging small photos
    </label>
    <div class="field">
      <label for="{prefix}-max">Largest file size (KB, optional)</label>
      <input
        class="input"
        id="{prefix}-max"
        type="number"
        min="1"
        step="1"
        placeholder="no limit"
        aria-describedby="{prefix}-max-help"
        bind:value={draft.maxKb}
      />
      <span class="help" id="{prefix}-max-help">The quality is lowered until the file fits.</span>
    </div>
    <div class="row">
      <label class="check">
        <input type="checkbox" bind:checked={draft.flatten} /> Flatten transparency onto
      </label>
      <input
        class="input"
        type="color"
        aria-label="Colour to flatten transparency onto"
        disabled={!draft.flatten}
        bind:value={draft.flattenColour}
      />
    </div>
  {:else if draft.kind === "enhance"}
    <div class="field">
      <span class="label" id="{prefix}-enhance-label">Light and colour</span>
      <div class="row row--wrap" role="radiogroup" aria-labelledby="{prefix}-enhance-label">
        <label class="check">
          <input type="radio" name="{prefix}-enhance" value="auto" bind:group={draft.enhanceMode} /> Automatic
        </label>
        <label class="check">
          <input type="radio" name="{prefix}-enhance" value="manual" bind:group={draft.enhanceMode} /> My own settings
        </label>
      </div>
      {#if draft.enhanceMode === "auto"}
        <span class="help">Stretches the tones to the full range and takes out a colour cast.</span>
      {/if}
    </div>
    {#if draft.enhanceMode === "manual"}
      <Slider id="{prefix}-brightness" label="Brightness" min={-100} max={100} bind:value={draft.brightness} />
      <Slider id="{prefix}-contrast" label="Contrast" min={-100} max={100} bind:value={draft.contrast} />
      <Slider
        id="{prefix}-saturation"
        label="Saturation"
        min={-100}
        max={100}
        help="-100 is black and white."
        bind:value={draft.saturation}
      />
      <Slider id="{prefix}-sharpen" label="Sharpen" min={0} max={100} bind:value={draft.sharpen} />
      <label class="check">
        <input type="checkbox" bind:checked={draft.keepAuto} /> Also fix light and colour automatically
      </label>
    {/if}
  {:else if draft.kind === "denoise"}
    <Slider
      id="{prefix}-denoise-strength"
      label="Strength"
      min={1}
      max={100}
      help="100 uses the cleaned photo as it is; lower blends it with the original. A photo that is already clean loses a little fine texture."
      bind:value={draft.denoiseStrength}
    />
  {:else if draft.kind === "colorize"}
    <Slider
      id="{prefix}-colorize-strength"
      label="Strength"
      min={1}
      max={100}
      help="100 is the model's colour as it is; lower is paler. A photo that already has colour is treated as black and white."
      bind:value={draft.colorizeStrength}
    />
  {:else if draft.kind === "restore_faces"}
    <Slider
      id="{prefix}-restore-strength"
      label="Strength"
      min={1}
      max={100}
      help="How much of the restored face replaces the original. A face that is already large and detailed is left alone."
      bind:value={draft.restoreStrength}
    />
  {:else if draft.kind === "blur_faces"}
    <div class="field">
      <label for="{prefix}-face-mode">Hide each face by</label>
      <select class="select" id="{prefix}-face-mode" aria-describedby="{prefix}-face-mode-help" bind:value={draft.faceMode}>
        <option value="blur">Blurring it</option>
        <option value="pixelate">Pixelating it</option>
        <option value="fill">Covering it with black</option>
      </select>
      <span class="help" id="{prefix}-face-mode-help">
        Blurring and pixelating stop a person from recognising the face. Only covering leaves nothing of it.
      </span>
    </div>
    {#if draft.faceMode !== "fill"}
      <Slider id="{prefix}-face-strength" label="Strength" min={1} max={100} bind:value={draft.faceStrength} />
    {/if}
    <p class="help">
      Small faces, and faces seen from behind or mostly covered, are not found. Each result says how many faces
      were hidden.
    </p>
  {:else if draft.kind === "crop"}
    <div class="field">
      <label for="{prefix}-crop-shape">Shape (width:height)</label>
      <select class="select" id="{prefix}-crop-shape" bind:value={draft.cropShape}>
        {#each SHAPES as shape (shape.value)}
          <option value={shape.value}>{shape.label}</option>
        {/each}
      </select>
    </div>
    <div class="field">
      <span class="label" id="{prefix}-focus-label">Crop around</span>
      <div class="row row--wrap" role="radiogroup" aria-labelledby="{prefix}-focus-label">
        <label class="check">
          <input type="radio" name="{prefix}-focus" value="subject" bind:group={draft.cropFocus} /> The subject
        </label>
        <label class="check">
          <input type="radio" name="{prefix}-focus" value="face" bind:group={draft.cropFocus} /> The faces
        </label>
      </div>
    </div>
    <label class="check">
      <input type="checkbox" bind:checked={draft.cropClose} /> Crop closely
    </label>
    {#if draft.cropClose}
      <div class="field">
        <label for="{prefix}-crop-room">Room around it (% of its longest side)</label>
        <input
          class="input"
          id="{prefix}-crop-room"
          type="number"
          min="0"
          max="200"
          step="1"
          placeholder="10"
          aria-describedby="{prefix}-crop-room-help"
          bind:value={draft.cropRoom}
        />
        <span class="help" id="{prefix}-crop-room-help">For a portrait, crop around the faces with about 60.</span>
      </div>
    {/if}
    <p class="help">
      {#if !draft.cropClose}The crop is the largest of that shape that holds what it is built around.{/if}
      When nothing is found to crop around, the crop is centred, and the result says so.
    </p>
  {:else if draft.kind === "expand"}
    <div class="field">
      <label for="{prefix}-expand-shape">Shape to reach (width:height)</label>
      <select class="select" id="{prefix}-expand-shape" bind:value={draft.expandShape}>
        {#each SHAPES as shape (shape.value)}
          <option value={shape.value}>{shape.label}</option>
        {/each}
      </select>
    </div>
    <div class="field">
      <span class="label" id="{prefix}-fill-label">New area</span>
      <div class="row row--wrap" role="radiogroup" aria-labelledby="{prefix}-fill-label">
        <label class="check">
          <input type="radio" name="{prefix}-fill" value="inpaint" bind:group={draft.expandFill} /> Continue the photo
        </label>
        <label class="check">
          <input type="radio" name="{prefix}-fill" value="white" bind:group={draft.expandFill} /> White
        </label>
      </div>
      {#if draft.expandFill === "inpaint"}
        <span class="help">
          Suits plain backgrounds such as a backdrop, a table or sky. Nothing is invented: a subject cut off at the
          edge stays cut off.
        </span>
      {/if}
    </div>
    <p class="help">A side can grow by the photo's own width or height at most. Transparency becomes white.</p>
  {:else if draft.kind === "watermark"}
    <div class="field">
      <label for="{prefix}-mark-text">Text</label>
      <input
        class="input"
        id="{prefix}-mark-text"
        type="text"
        maxlength={MAX_MARK_TEXT}
        autocomplete="off"
        placeholder="© Your shop"
        aria-describedby="{prefix}-mark-text-help"
        bind:value={draft.markText}
      />
      <span class="help" id="{prefix}-mark-text-help">
        Latin, Greek and Cyrillic letters. Other scripts come out as empty boxes.
      </span>
    </div>
    <div class="grid-2">
      <div class="field">
        <label for="{prefix}-mark-position">Position</label>
        <select class="select" id="{prefix}-mark-position" bind:value={draft.markPosition}>
          {#each POSITIONS as position (position.value)}
            <option value={position.value}>{position.label}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <label for="{prefix}-mark-colour">Colour</label>
        <select class="select" id="{prefix}-mark-colour" bind:value={draft.markColour}>
          <option value="white">White</option>
          <option value="black">Black</option>
        </select>
      </div>
    </div>
    <Slider
      id="{prefix}-mark-size"
      label="Size (% of the photo's width)"
      min={1}
      max={100}
      bind:value={draft.markSize}
    />
    <Slider id="{prefix}-mark-opacity" label="Opacity" min={1} max={100} bind:value={draft.markOpacity} />
  {:else if draft.kind === "vectorize"}
    <div class="field">
      <label for="{prefix}-vector-mode">Trace</label>
      <select class="select" id="{prefix}-vector-mode" bind:value={draft.vectorMode}>
        <option value="color">Each colour as its own shape</option>
        <option value="mono">One black shape</option>
      </select>
      {#if draft.vectorMode === "mono"}
        <span class="help">Whatever is darker than mid grey becomes the shape.</span>
      {/if}
    </div>
    <div class="field">
      <label for="{prefix}-curves">Outlines</label>
      <select class="select" id="{prefix}-curves" bind:value={draft.curves}>
        <option value="spline">Smooth curves</option>
        <option value="polygon">Straight segments</option>
        <option value="pixel">Every pixel step (for pixel art)</option>
      </select>
    </div>
    <p class="help">
      The result is an SVG file. A photograph becomes thousands of paths and a large file. Pictures over 2000
      pixels a side are traced from a smaller copy.
    </p>
  {:else if draft.kind === "fix"}
    <div class="field">
      <label for="{prefix}-marketplace">Marketplace</label>
      <select
        class="select"
        id="{prefix}-marketplace"
        bind:value={draft.marketplace}
        disabled={catalog === null}
        aria-describedby="{prefix}-marketplace-help"
      >
        <option value="">Choose…</option>
        {#each catalog?.marketplaces ?? [] as marketplace (marketplace.name)}
          <option value={marketplace.name}>{marketplace.title || marketplace.name}</option>
        {/each}
      </select>
      <span class="help" id="{prefix}-marketplace-help">
        {listProblem ??
          "Cuts the product out onto white where the marketplace wants it, removes added text, and resizes and compresses to its limits."}
      </span>
    </div>
  {:else if draft.kind === "run"}
    <div class="field">
      <label for="{prefix}-preset">Preset</label>
      <select
        class="select"
        id="{prefix}-preset"
        bind:value={draft.preset}
        disabled={catalog === null}
        aria-describedby="{prefix}-preset-help"
      >
        <option value="">Choose…</option>
        {#each catalog?.presets ?? [] as item (item.name)}
          <option value={item.name}>{item.name}</option>
        {/each}
      </select>
      <span class="help" id="{prefix}-preset-help">
        {listProblem ?? preset?.description ?? "Each preset is a ready-made chain of tools."}
      </span>
    </div>
  {/if}

  {#if usesOutput(draft.kind)}
    <div class="grid-2">
      <div class="field">
        <label for="{prefix}-format">Format</label>
        <select class="select" id="{prefix}-format" bind:value={draft.format}>
          <option value="">{draft.kind === "remove_background" ? "Automatic" : "Same as the photo"}</option>
          {#each FORMATS as format (format.value)}
            <option value={format.value}>{format.label}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <label for="{prefix}-quality">Quality (1–100)</label>
        <input
          class="input"
          id="{prefix}-quality"
          type="number"
          min="1"
          max="100"
          step="1"
          placeholder="automatic"
          disabled={draft.format === "png"}
          bind:value={draft.quality}
        />
      </div>
    </div>
  {/if}
</fieldset>

<style>
  .form {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }
  .caution {
    padding: 0.5rem 0.625rem;
    border: 1px solid color-mix(in srgb, var(--tone) 35%, transparent);
    border-radius: var(--ps-radius-sm);
    background: var(--tone-soft);
    color: var(--tone-text);
  }
</style>
