// What crosses the bridge between the UI and the Rust core. These mirror the
// core's types (src-tauri/src): keep them in step.

export type Format = "png" | "jpg" | "webp" | "avif";
export type Fit = "contain" | "cover" | "fill";
export type TextRegion = "background" | "all";
export type Shadow = "none" | "contact" | "drop";
/** The model remove background finds the subject with; the API uses `atlas` when none is sent. */
export type Model = "atlas" | "lumen" | "muse";
export type FaceMode = "blur" | "pixelate" | "fill";
export type CropFocus = "subject" | "face";
export type Position =
  | "top-left"
  | "top"
  | "top-right"
  | "left"
  | "center"
  | "right"
  | "bottom-left"
  | "bottom"
  | "bottom-right";
export type VectorMode = "color" | "mono";
export type Curves = "spline" | "polygon" | "pixel";
/** Which of select's answers: the surest, the part clicked, or the whole thing. */
export type SelectPick = "auto" | "smallest" | "largest";

export interface Settings {
  base_url: string;
  parallel: number;
  default_format: Format | null;
  /** Ask for a newer version when the app starts. */
  check_for_updates: boolean;
}

/** A newer version of the app. */
export interface UpdateInfo {
  version: string;
  current_version: string;
  notes: string | null;
}

/** How far an update's download is. */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export type KeyStorage = "none" | "keychain" | "session";

export interface KeyStatus {
  set: boolean;
  last4: string | null;
  storage: KeyStorage;
  keychain_available: boolean;
  keychain_error: string | null;
}

export interface Account {
  account?: string;
  key?: string;
  credits?: number;
  prices: Record<string, number>;
  unlimited: boolean;
}

export interface Preset {
  name: string;
  description: string;
}

export interface Marketplace {
  name: string;
  title: string;
}

export interface Catalog {
  presets: Preset[];
  marketplaces: Marketplace[];
}

/** Several steps on one upload. The service runs them in a fixed order:
 *  read barcodes and text (on the photo as it came), remove text, upscale,
 *  remove background, frame or crop, shadow, background, output. */
export interface Recipe {
  barcodes?: boolean;
  /** Every barcode format, not only retail codes. */
  all_formats?: boolean;
  text?: boolean;
  remove_text?: TextRegion;
  /** Upscale a photo whose longest side is under this many pixels. */
  upscale_to?: number;
  remove_background?: boolean;
  /** Left out, the service uses `atlas`. */
  model?: Model;
  studio?: boolean;
  layout?:
    | { kind: "frame"; width: number; height: number; /** Percent. */ fill?: number; bottom?: boolean }
    | { kind: "crop"; /** Percent of the subject's longest side. */ padding?: number };
  shadow?: Shadow;
  background?: { kind: "color"; color: string } | { kind: "gradient"; from: string; to: string } | { kind: "blur" };
  format?: Format;
  quality?: number;
  max_side?: number;
  max_bytes?: number;
}

/** An operation as the core takes it. Options left out keep the API's defaults. */
export type Operation =
  | {
      kind: "remove_background";
      background?: string;
      crop?: boolean;
      padding?: number;
      shadow?: Shadow;
      /** The model that finds the subject; left out, the API uses `atlas`. */
      model?: Model;
      /** A product on a plain, light backdrop: keep its box or lid too. */
      studio?: boolean;
      format?: Format;
      quality?: number;
    }
  | { kind: "upscale"; scale?: number; format?: Format; quality?: number }
  | {
      kind: "remove_text";
      region?: TextRegion;
      matching?: string;
      format?: Format;
      quality?: number;
    }
  | {
      kind: "resize";
      width?: number;
      height?: number;
      fit?: Fit;
      enlarge?: boolean;
      format?: Format;
      quality?: number;
      max_bytes?: number;
      background?: string;
    }
  | { kind: "fix"; marketplace: string }
  | { kind: "run"; preset: string }
  | { kind: "recipe"; recipe: Recipe }
  | {
      kind: "enhance";
      /** Keep the automatic fix beside the settings given. */
      auto?: boolean;
      brightness?: number;
      contrast?: number;
      saturation?: number;
      sharpen?: number;
      format?: Format;
      quality?: number;
    }
  | { kind: "denoise"; strength?: number; format?: Format; quality?: number }
  | { kind: "colorize"; strength?: number; format?: Format; quality?: number }
  | { kind: "restore_faces"; strength?: number; format?: Format; quality?: number }
  | { kind: "blur_faces"; mode?: FaceMode; strength?: number; format?: Format; quality?: number }
  | {
      kind: "crop";
      aspect: string;
      focus?: CropFocus;
      zoom?: boolean;
      /** Room around the subject with `zoom`, as a fraction of its longest side. */
      padding?: number;
      format?: Format;
      quality?: number;
    }
  | {
      kind: "expand";
      aspect: string;
      /** A colour for the new area; left out, the photo is continued by inpainting. */
      fill?: string;
      format?: Format;
      quality?: number;
    }
  | {
      kind: "watermark";
      text: string;
      color?: string;
      position?: Position;
      scale?: number;
      opacity?: number;
      format?: Format;
      quality?: number;
    }
  | { kind: "vectorize"; mode?: VectorMode; curves?: Curves };

export type OperationKind = Operation["kind"];

/** `skipped`: never sent. `cancelled`: given up while it was being processed. */
export type ItemStatus = "queued" | "running" | "done" | "failed" | "skipped" | "cancelled";

export interface ItemError {
  code: string;
  message: string;
}

export interface BatchItem {
  id: number;
  source: string;
  name: string;
  status: ItemStatus;
  output: string | null;
  output_name: string | null;
  width: number | null;
  height: number | null;
  bytes: number | null;
  note: string | null;
  error: ItemError | null;
  version: number;
}

export interface BatchSnapshot {
  items: BatchItem[];
  running: boolean;
  default_output_dir: string | null;
}

export interface BatchSummary {
  total: number;
  done: number;
  failed: number;
  skipped: number;
  /** Files given up while they were being processed, because of a cancel. */
  aborted: number;
  cancelled: boolean;
  stopped: ItemError | null;
  output_dir: string;
}

export interface Estimate {
  files: number;
  unlimited: boolean;
  price: number;
  cost: number;
  credits: number | null;
  enough: boolean;
}

export interface StepInfo {
  id: number;
  label: string;
  content_type: string;
  width: number | null;
  height: number | null;
  bytes: number | null;
  note: string | null;
  /** What a recipe read off the photo. */
  readings: Readings | null;
}

export interface Barcode {
  /** `EAN-13`, `UPC-A`, `QR`, … */
  format: string;
  value: string;
  /** The GS1-valid number of a retail code. */
  gs1: string | null;
}

/** A field is `null` when its step was not in the recipe, empty when it found nothing. */
export interface Readings {
  barcodes: Barcode[] | null;
  text: string[] | null;
  /** Lines were left out: the service sends at most 8 KB of them. */
  text_truncated: boolean;
}

export interface EditorSnapshot {
  session: number;
  name: string;
  path: string;
  original: StepInfo;
  steps: StepInfo[];
  cursor: number;
  current: StepInfo;
  can_undo: boolean;
  can_redo: boolean;
  suggested_name: string;
}

/** A drag of the brush, in the photo's own pixels. */
export interface Stroke {
  radius: number;
  points: [number, number][];
}

/** A click on the photo, in the photo's own pixels. */
export type Point = [number, number];

export type EditorTool =
  | { tool: "operation"; operation: Operation }
  | { tool: "erase"; strokes: Stroke[]; format?: Format; quality?: number }
  | { tool: "select"; points: Point[]; pick?: SelectPick };

export type Severity = "info" | "warning" | "error";

export interface Finding {
  code: string;
  severity: Severity;
  message: string;
  detail?: string;
}

export interface Verdict {
  marketplace: string;
  title?: string;
  passed: boolean;
  findings: Finding[];
  not_checked: string[];
  notes?: string[];
}

export interface CheckReport {
  metrics: Record<string, unknown>;
  findings: Finding[];
  marketplace?: Verdict;
}

/** An error from the core: `code` chooses the wording, `message` is its own. */
export interface AppError {
  code: string;
  message: string;
  status: number | null;
  retry_after: number | null;
}
