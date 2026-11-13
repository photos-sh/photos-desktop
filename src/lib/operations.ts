// The operation forms: what the user fills in (a draft) and the operation
// the core is sent. Only options the user set are sent, so the API's own
// defaults apply to the rest.

import type {
  CropFocus,
  Curves,
  FaceMode,
  Fit,
  Format,
  Operation,
  OperationKind,
  Position,
  Shadow,
  TextRegion,
  VectorMode,
} from "./types";

export interface OperationChoice {
  kind: OperationKind;
  label: string;
  hint: string;
  /** The name the operation has in the account's price list. */
  priceKey: string;
  /** Where the tool can mislead, in one sentence. Shown with its options. */
  warning?: string;
}

export const OPERATIONS: readonly OperationChoice[] = [
  {
    kind: "remove_background",
    label: "Remove background",
    hint: "Cut the product out, onto transparency or a colour.",
    priceKey: "remove-background",
  },
  {
    kind: "upscale",
    label: "Upscale",
    hint: "Enlarge up to 4× with AI, keeping edges sharp.",
    priceKey: "upscale",
  },
  {
    kind: "remove_text",
    label: "Remove text",
    hint: "Paint out watermarks, captions and prices.",
    priceKey: "remove-text",
  },
  {
    kind: "enhance",
    label: "Enhance",
    hint: "Fix light and colour, by itself or with your own settings.",
    priceKey: "enhance",
  },
  {
    kind: "denoise",
    label: "Denoise",
    hint: "Remove camera noise and the blocks of heavy JPEG compression.",
    priceKey: "denoise",
    warning:
      "Slow when the server has no GPU (about half a minute per megapixel), and only for photos up to 12 megapixels.",
  },
  {
    kind: "colorize",
    label: "Colourise",
    hint: "Add colour to a black-and-white photo.",
    priceKey: "colorize",
    warning:
      "The colours are guesses: skin, sky and grass come out believable, but the colour of clothes, cars or walls is invented.",
  },
  {
    kind: "restore_faces",
    label: "Restore faces",
    hint: "Redraw the faces in an old, blurred or small photo. The rest of the photo is not touched.",
    priceKey: "restore-faces",
    warning:
      "The detail is invented: the face resembles the person but is not exactly theirs, so do not use it for identity photos or evidence.",
  },
  {
    kind: "blur_faces",
    label: "Hide faces",
    hint: "Find every face and blur, pixelate or cover it.",
    priceKey: "blur-faces",
  },
  {
    kind: "crop",
    label: "Crop to a shape",
    hint: "Crop to a shape around the subject or the faces. The background stays.",
    priceKey: "crop",
  },
  {
    kind: "expand",
    label: "Expand",
    hint: "Add room around the photo until it has a shape.",
    priceKey: "expand",
  },
  {
    kind: "resize",
    label: "Resize / convert",
    hint: "Change the size or format, or compress to a file size.",
    priceKey: "resize",
  },
  {
    kind: "watermark",
    label: "Watermark",
    hint: "Put a line of text on the photo.",
    priceKey: "watermark",
  },
  {
    kind: "vectorize",
    label: "Vectorize",
    hint: "Trace a logo, icon or flat drawing into an SVG file.",
    priceKey: "vectorize",
  },
  {
    kind: "fix",
    label: "Fix for a marketplace",
    hint: "Make the photo meet a marketplace's image rules.",
    priceKey: "fix",
  },
  {
    kind: "run",
    label: "Run a preset",
    hint: "Apply a ready-made recipe.",
    priceKey: "run",
  },
];

export const FORMATS: readonly { value: Format; label: string }[] = [
  { value: "png", label: "PNG" },
  { value: "jpg", label: "JPEG" },
  { value: "webp", label: "WebP" },
  { value: "avif", label: "AVIF" },
];

/** The shapes crop and expand offer, as width:height. */
export const SHAPES: readonly { value: string; label: string }[] = [
  { value: "1:1", label: "1:1 (square)" },
  { value: "4:5", label: "4:5 (portrait)" },
  { value: "3:4", label: "3:4" },
  { value: "2:3", label: "2:3" },
  { value: "9:16", label: "9:16 (tall)" },
  { value: "5:4", label: "5:4" },
  { value: "4:3", label: "4:3" },
  { value: "3:2", label: "3:2" },
  { value: "16:9", label: "16:9 (wide)" },
];

export const POSITIONS: readonly { value: Position; label: string }[] = [
  { value: "top-left", label: "Top left" },
  { value: "top", label: "Top" },
  { value: "top-right", label: "Top right" },
  { value: "left", label: "Left" },
  { value: "center", label: "Centre" },
  { value: "right", label: "Right" },
  { value: "bottom-left", label: "Bottom left" },
  { value: "bottom", label: "Bottom" },
  { value: "bottom-right", label: "Bottom right" },
];

/** Where the API's sliders rest. A slider left there is not sent. */
export const API_DEFAULT = {
  /** Denoise, colourise and restore faces: all of the result. */
  strength: 100,
  /** Hide faces: how strong the blur or how coarse the mosaic is. */
  faceStrength: 50,
  /** A watermark's width, in percent of the photo's width. */
  markSize: 25,
  markOpacity: 50,
} as const;

/** The longest text a watermark takes, in characters. */
export const MAX_MARK_TEXT = 200;

export type BackgroundChoice = "transparent" | "white" | "colour";

/** Everything the forms can hold, for every kind at once, so switching the
 *  operation keeps what was typed. Empty number fields are `null`. */
export interface Draft {
  kind: OperationKind;
  format: Format | "";
  quality: number | null;
  // remove background
  background: BackgroundChoice;
  colour: string;
  crop: boolean;
  padding: number | null;
  shadow: Shadow;
  // upscale
  scale: number;
  // remove text
  region: TextRegion;
  matching: string;
  // resize
  width: number | null;
  height: number | null;
  fit: Fit;
  enlarge: boolean;
  maxKb: number | null;
  flatten: boolean;
  flattenColour: string;
  // fix, run
  marketplace: string;
  preset: string;
  // enhance: by itself, or by the settings below
  enhanceMode: "auto" | "manual";
  /** With own settings: keep the automatic fix too. */
  keepAuto: boolean;
  brightness: number;
  contrast: number;
  saturation: number;
  sharpen: number;
  // denoise, colourise, restore faces: each remembers its own strength
  denoiseStrength: number;
  colorizeStrength: number;
  restoreStrength: number;
  // hide faces
  faceMode: FaceMode;
  faceStrength: number;
  // crop to a shape
  cropShape: string;
  cropFocus: CropFocus;
  /** Crop closely around the subject, instead of as large as the shape allows. */
  cropClose: boolean;
  /** Room around the subject when cropping closely, in percent of its longest side. */
  cropRoom: number | null;
  // expand
  expandShape: string;
  expandFill: "inpaint" | "white";
  // watermark
  markText: string;
  markColour: "white" | "black";
  markPosition: Position;
  markSize: number;
  markOpacity: number;
  // vectorize
  vectorMode: VectorMode;
  curves: Curves;
}

export function newDraft(kind: OperationKind = "remove_background", format: Format | null = null): Draft {
  return {
    kind,
    format: format ?? "",
    quality: null,
    background: "transparent",
    colour: "#ffffff",
    crop: false,
    padding: null,
    shadow: "none",
    scale: 2,
    region: "background",
    matching: "",
    width: null,
    height: null,
    fit: "contain",
    enlarge: false,
    maxKb: null,
    flatten: false,
    flattenColour: "#ffffff",
    marketplace: "",
    preset: "",
    enhanceMode: "auto",
    keepAuto: false,
    brightness: 0,
    contrast: 0,
    saturation: 0,
    sharpen: 0,
    denoiseStrength: API_DEFAULT.strength,
    colorizeStrength: API_DEFAULT.strength,
    restoreStrength: API_DEFAULT.strength,
    faceMode: "blur",
    faceStrength: API_DEFAULT.faceStrength,
    cropShape: "1:1",
    cropFocus: "subject",
    cropClose: false,
    cropRoom: null,
    expandShape: "1:1",
    expandFill: "inpaint",
    markText: "",
    markColour: "white",
    markPosition: "bottom-right",
    markSize: API_DEFAULT.markSize,
    markOpacity: API_DEFAULT.markOpacity,
    vectorMode: "color",
    curves: "spline",
  };
}

function isWhole(n: number | null, min: number, max = Number.MAX_SAFE_INTEGER): boolean {
  return n == null || (Number.isInteger(n) && n >= min && n <= max);
}

function within(n: number, min: number, max: number): boolean {
  return Number.isFinite(n) && n >= min && n <= max;
}

const COLOUR = /^#[0-9a-f]{6}$/i;
/** A shape as width:height in whole numbers. */
const SHAPE = /^[1-9]\d{0,3}:[1-9]\d{0,3}$/;
const STRENGTH = "The strength goes from 1 to 100.";

/** What is wrong with a draft, in plain words; `null` when it can be run. */
export function problem(draft: Draft): string | null {
  const lossy = draft.format !== "png";
  if (usesOutput(draft.kind) && lossy && !isWhole(draft.quality, 1, 100)) {
    return "Quality is a whole number from 1 to 100.";
  }
  switch (draft.kind) {
    case "remove_background":
      if (draft.background === "colour" && !COLOUR.test(draft.colour)) {
        return "Choose a background colour.";
      }
      if (draft.crop && !isWhole(draft.padding, 0, 10_000)) {
        return "The margin is a whole number of pixels, 0 or more.";
      }
      return null;
    case "upscale":
      if (!(draft.scale >= 1 && draft.scale <= 4)) return "The scale goes from 1 to 4.";
      return null;
    case "remove_text":
      if (draft.matching.trim() !== "") {
        try {
          new RegExp(draft.matching);
        } catch {
          return "The “only text matching” pattern is not a valid regular expression.";
        }
      }
      return null;
    case "resize":
      if (!isWhole(draft.width, 1, 100_000) || !isWhole(draft.height, 1, 100_000)) {
        return "Width and height are whole numbers of pixels, 1 or more.";
      }
      if (draft.fit !== "contain" && (draft.width == null || draft.height == null)) {
        return "Cover and fill need both a width and a height.";
      }
      if (draft.maxKb != null && !(draft.maxKb > 0)) {
        return "The file size limit must be more than 0 KB.";
      }
      if (draft.flatten && !COLOUR.test(draft.flattenColour)) {
        return "Choose a colour to flatten transparency onto.";
      }
      if (
        draft.width == null &&
        draft.height == null &&
        draft.format === "" &&
        draft.quality == null &&
        draft.maxKb == null &&
        !draft.flatten
      ) {
        return "Set a size, a format, a quality or a file size limit.";
      }
      return null;
    case "fix":
      return draft.marketplace === "" ? "Choose a marketplace." : null;
    case "run":
      return draft.preset === "" ? "Choose a preset." : null;
    case "enhance": {
      if (draft.enhanceMode === "auto") return null;
      const { brightness, contrast, saturation, sharpen } = draft;
      if (![brightness, contrast, saturation].every((n) => within(n, -100, 100))) {
        return "Brightness, contrast and saturation go from -100 to 100.";
      }
      if (!within(sharpen, 0, 100)) return "Sharpening goes from 0 to 100.";
      // Nothing set would be sent as nothing, which the API takes as "automatic".
      if (!draft.keepAuto && brightness === 0 && contrast === 0 && saturation === 0 && sharpen === 0) {
        return "Move a slider, or choose Automatic.";
      }
      return null;
    }
    case "denoise":
      return within(draft.denoiseStrength, 1, 100) ? null : STRENGTH;
    case "colorize":
      return within(draft.colorizeStrength, 1, 100) ? null : STRENGTH;
    case "restore_faces":
      return within(draft.restoreStrength, 1, 100) ? null : STRENGTH;
    case "blur_faces":
      return draft.faceMode === "fill" || within(draft.faceStrength, 1, 100) ? null : STRENGTH;
    case "crop":
      if (!SHAPE.test(draft.cropShape)) return "Choose a shape.";
      if (draft.cropClose && draft.cropRoom != null && !within(draft.cropRoom, 0, 200)) {
        return "The room around the subject goes from 0 to 200 percent.";
      }
      return null;
    case "expand":
      return SHAPE.test(draft.expandShape) ? null : "Choose a shape.";
    case "watermark": {
      const text = draft.markText.trim();
      if (text === "") return "Type the watermark's text.";
      // Characters, not UTF-16 units: that is how the API counts.
      if ([...text].length > MAX_MARK_TEXT) return `The text can have ${MAX_MARK_TEXT} characters at most.`;
      if (!within(draft.markSize, 1, 100)) return "The size goes from 1 to 100 percent of the photo's width.";
      if (!within(draft.markOpacity, 1, 100)) return "The opacity goes from 1 to 100.";
      return null;
    }
    case "vectorize":
      return null;
  }
}

/** Whether the kind has the format and quality options. Vectorize always
 *  gives an SVG file. */
export function usesOutput(kind: OperationKind): boolean {
  return kind !== "fix" && kind !== "run" && kind !== "vectorize";
}

/** Whether the result is a picture in pixels, which the editor can keep as a
 *  step for the next tool. Vectorize gives an SVG file. */
export function givesPixels(kind: OperationKind): boolean {
  return kind !== "vectorize";
}

function output(draft: Draft): { format?: Format; quality?: number } {
  const out: { format?: Format; quality?: number } = {};
  if (draft.format !== "") out.format = draft.format;
  // PNG has no quality.
  if (draft.quality != null && draft.format !== "png") out.quality = draft.quality;
  return out;
}

/** The operation to send. Call `problem` first: this does not check. */
export function build(draft: Draft): Operation {
  switch (draft.kind) {
    case "remove_background": {
      const op: Operation = { kind: "remove_background", ...output(draft) };
      if (draft.background === "white") op.background = "white";
      if (draft.background === "colour") op.background = draft.colour.toLowerCase();
      if (draft.crop) {
        op.crop = true;
        if (draft.padding != null && draft.padding > 0) op.padding = draft.padding;
      }
      if (draft.shadow !== "none") op.shadow = draft.shadow;
      return op;
    }
    case "upscale":
      return { kind: "upscale", scale: draft.scale, ...output(draft) };
    case "remove_text": {
      const op: Operation = { kind: "remove_text", ...output(draft) };
      if (draft.region === "all") op.region = "all";
      if (draft.matching.trim() !== "") op.matching = draft.matching.trim();
      return op;
    }
    case "resize": {
      const op: Operation = { kind: "resize", ...output(draft) };
      if (draft.width != null) op.width = draft.width;
      if (draft.height != null) op.height = draft.height;
      if (draft.fit !== "contain") op.fit = draft.fit;
      if (draft.enlarge) op.enlarge = true;
      if (draft.maxKb != null) op.max_bytes = Math.round(draft.maxKb * 1024);
      if (draft.flatten) op.background = draft.flattenColour.toLowerCase();
      return op;
    }
    case "fix":
      return { kind: "fix", marketplace: draft.marketplace };
    case "run":
      return { kind: "run", preset: draft.preset };
    case "enhance": {
      const op: Operation = { kind: "enhance", ...output(draft) };
      // Automatic is the API's own behaviour when no setting is sent.
      if (draft.enhanceMode === "manual") {
        if (draft.keepAuto) op.auto = true;
        if (draft.brightness !== 0) op.brightness = draft.brightness;
        if (draft.contrast !== 0) op.contrast = draft.contrast;
        if (draft.saturation !== 0) op.saturation = draft.saturation;
        if (draft.sharpen !== 0) op.sharpen = draft.sharpen;
      }
      return op;
    }
    case "denoise": {
      const op: Operation = { kind: "denoise", ...output(draft) };
      if (draft.denoiseStrength !== API_DEFAULT.strength) op.strength = draft.denoiseStrength;
      return op;
    }
    case "colorize": {
      const op: Operation = { kind: "colorize", ...output(draft) };
      if (draft.colorizeStrength !== API_DEFAULT.strength) op.strength = draft.colorizeStrength;
      return op;
    }
    case "restore_faces": {
      const op: Operation = { kind: "restore_faces", ...output(draft) };
      if (draft.restoreStrength !== API_DEFAULT.strength) op.strength = draft.restoreStrength;
      return op;
    }
    case "blur_faces": {
      const op: Operation = { kind: "blur_faces", ...output(draft) };
      if (draft.faceMode !== "blur") op.mode = draft.faceMode;
      // A fill covers the face whole: it has no strength.
      if (draft.faceMode !== "fill" && draft.faceStrength !== API_DEFAULT.faceStrength) {
        op.strength = draft.faceStrength;
      }
      return op;
    }
    case "crop": {
      const op: Operation = { kind: "crop", aspect: draft.cropShape, ...output(draft) };
      if (draft.cropFocus === "face") op.focus = "face";
      if (draft.cropClose) {
        op.zoom = true;
        // The API takes the room as a fraction of the subject's longest side.
        if (draft.cropRoom != null) op.padding = draft.cropRoom / 100;
      }
      return op;
    }
    case "expand": {
      const op: Operation = { kind: "expand", aspect: draft.expandShape, ...output(draft) };
      if (draft.expandFill === "white") op.fill = "white";
      return op;
    }
    case "watermark": {
      const op: Operation = { kind: "watermark", text: draft.markText.trim(), ...output(draft) };
      if (draft.markColour !== "white") op.color = draft.markColour;
      if (draft.markPosition !== "bottom-right") op.position = draft.markPosition;
      if (draft.markSize !== API_DEFAULT.markSize) op.scale = draft.markSize;
      if (draft.markOpacity !== API_DEFAULT.markOpacity) op.opacity = draft.markOpacity;
      return op;
    }
    case "vectorize": {
      const op: Operation = { kind: "vectorize" };
      if (draft.vectorMode !== "color") op.mode = draft.vectorMode;
      if (draft.curves !== "spline") op.curves = draft.curves;
      return op;
    }
  }
}

export function choice(kind: OperationKind): OperationChoice {
  const found = OPERATIONS.find((o) => o.kind === kind);
  if (!found) throw new Error(`unknown operation ${kind}`);
  return found;
}
