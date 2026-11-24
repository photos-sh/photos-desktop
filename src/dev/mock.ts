// The core, faked, for looking at the UI in a plain browser:
//
//     npm run dev:mock        then open http://127.0.0.1:1420/
//
// Add `?key=none` to start without an API key, `?credits=3` for an account
// that cannot pay for much, `?keychain=none` for a system without a
// keychain, `?fail=unauthorized` to make every API call fail with that
// code, `?update=1.5.0` for a newer version to offer (`?update=fail` for a
// check that fails). `window.__mock.drop([...paths])` plays a file drop.
//
// Nothing here is in a normal build (see main.ts).

import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

import { choice } from "../lib/operations";
import { setPreviewResolver } from "../lib/preview";
import type {
  Account,
  AppError,
  BatchItem,
  BatchSnapshot,
  CheckReport,
  EditorSnapshot,
  EditorTool,
  Estimate,
  KeyStatus,
  Operation,
  Readings,
  Recipe,
  Settings,
  StepInfo,
} from "../lib/types";

const params = new URLSearchParams(location.search);
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function fail(code: string, message: string, status: number | null = null): never {
  const error: AppError = { code, message, status, retry_after: null };
  throw error;
}

const PRICES: Record<string, number> = {
  "remove-background": 2,
  upscale: 4,
  "remove-text": 3,
  resize: 1,
  fix: 5,
  run: 3,
  erase: 3,
  check: 1,
  // Some of the newer tools have a price; the rest are free, as on a real
  // account until its price list names them.
  denoise: 3,
  colorize: 3,
  "restore-faces": 3,
  "blur-faces": 2,
  crop: 2,
  expand: 3,
  select: 2,
};

/** What a step looks like in the mock's drawing, by the tool that made it. */
const LOOK: Record<Operation["kind"], string> = {
  remove_background: "cutout",
  upscale: "",
  remove_text: "text",
  resize: "",
  fix: "cutout fixed text",
  run: "cutout",
  recipe: "",
  enhance: "",
  denoise: "",
  colorize: "",
  restore_faces: "",
  blur_faces: "",
  crop: "",
  expand: "",
  watermark: "",
  vectorize: "",
};

/** What the core would say an operation reported. */
const NOTE: Partial<Record<Operation["kind"], string>> = {
  remove_text: "2 text lines removed",
  blur_faces: "2 faces hidden",
  restore_faces: "1 face restored",
  vectorize: "23 paths",
};

/** The step's name, as the core gives it. */
/** What the mock's photo of a ham gives a recipe that reads. */
function mockReadings(r: Recipe): Readings | null {
  if (!r.barcodes && !r.text) return null;
  return {
    barcodes: r.barcodes ? [{ format: "EAN-13", value: "3850104010017", gs1: "3850104010017" }] : null,
    text: r.text ? ["Dalmatinski pršut", "Zaštićena oznaka zemljopisnog podrijetla", "500 g"] : null,
    text_truncated: false,
  };
}

function stepLabel(op: Operation): string {
  switch (op.kind) {
    case "upscale":
      return `Upscale ${op.scale ?? 4}×`;
    case "fix":
      return `Fix for ${op.marketplace}`;
    case "run":
      return `Preset: ${op.preset}`;
    case "recipe":
      return "Recipe";
    case "crop":
      return `Crop to ${op.aspect}`;
    case "expand":
      return `Expand to ${op.aspect}`;
    default:
      return choice(op.kind).label;
  }
}

let settings: Settings = { base_url: "https://api.photos.sh", parallel: 3, default_format: null, check_for_updates: true };
const keychain = params.get("keychain") !== "none";
let key: string | null = params.get("key") === "none" ? null : "ps_live_0123456789abcdef";
let credits = Number(params.get("credits") ?? 240);
const failing = params.get("fail");

function keyStatus(): KeyStatus {
  return {
    set: key !== null,
    last4: key?.slice(-4) ?? null,
    storage: key === null ? "none" : keychain ? "keychain" : "session",
    keychain_available: keychain,
    keychain_error: keychain ? null : "no secret service was found on the session bus",
  };
}

function needKey() {
  if (key === null) fail("no_key", "no API key is set");
  if (failing === "unauthorized") fail("unauthorized", "Unknown API key", 401);
  if (failing === "connection") fail("connection", "could not reach the server: connection refused");
}

function account(): Account {
  needKey();
  return { account: "acc_k3j9x2pq", key: "key_7fd2", credits, prices: PRICES, unlimited: false };
}

// --- batch ---

let items: BatchItem[] = [];
let nextId = 1;
let running = false;
let cancelled = false;
let lastOperation: Operation | null = null;

function snapshot(): BatchSnapshot {
  const first = items[0]?.source;
  return {
    items,
    running,
    default_output_dir: first ? `${first.slice(0, first.lastIndexOf("/"))}/photos-sh` : null,
  };
}

function addPaths(paths: string[]) {
  for (const path of paths) {
    const list = /\.[a-z]+$/i.test(path)
      ? [path]
      : ["boots.jpg", "lamp.png", "mug.webp", "broken.jpg", "scarf.heic"].map((name) => `${path}/${name}`);
    for (const source of list) {
      if (items.some((item) => item.source === source)) continue;
      items = [
        ...items,
        {
          id: nextId++,
          source,
          name: source.slice(source.lastIndexOf("/") + 1),
          status: "queued",
          output: null,
          output_name: null,
          width: null,
          height: null,
          bytes: null,
          note: null,
          error: null,
          version: 0,
        },
      ];
    }
  }
}

function change(id: number, patch: Partial<BatchItem>) {
  items = items.map((item) => (item.id === id ? { ...item, ...patch, version: item.version + 1 } : item));
  const item = items.find((i) => i.id === id);
  if (item) void emit("batch:item", item);
}

function estimate(operation: Operation, files: number): Estimate {
  account();
  const price = PRICES[choice(operation.kind).priceKey] ?? 0;
  return { files, unlimited: false, price, cost: price * files, credits, enough: price * files <= credits };
}

async function runBatch(operation: Operation, ids: number[]) {
  const price = PRICES[choice(operation.kind).priceKey] ?? 0;
  const extension =
    operation.kind === "vectorize" ? "svg" : "format" in operation && operation.format ? operation.format : "png";
  let failed = 0;
  let skipped = 0;
  let aborted = 0;
  for (const id of ids) {
    const item = items.find((i) => i.id === id);
    if (!item) continue;
    if (cancelled) {
      skipped += 1;
      change(id, { status: "skipped" });
      continue;
    }
    change(id, { status: "running" });
    await wait(700);
    if (cancelled) {
      aborted += 1;
      change(id, { status: "cancelled" });
      continue;
    }
    if (item.name.startsWith("broken")) {
      failed += 1;
      change(id, {
        status: "failed",
        error: { code: "unsupported_format", message: "The image could not be decoded" },
      });
      continue;
    }
    credits -= price;
    const stem = item.name.replace(/\.[^.]+$/, "");
    change(id, {
      status: "done",
      output: `${snapshot().default_output_dir}/${stem}.${extension}`,
      output_name: `${stem}.${extension}`,
      width: 1200,
      height: 1200,
      bytes: 184_320,
      note: NOTE[operation.kind] ?? null,
    });
  }
  running = false;
  void emit("batch:finished", {
    total: ids.length,
    done: ids.length - failed - skipped - aborted,
    failed,
    skipped,
    aborted,
    cancelled,
    stopped: null,
    output_dir: snapshot().default_output_dir ?? "",
  });
}

function startBatch(operation: Operation, statuses: BatchItem["status"][]): Estimate {
  if (running) fail("batch_running", "A batch is running. Wait for it to finish, or cancel it.");
  const ids = items.filter((item) => statuses.includes(item.status)).map((item) => item.id);
  if (ids.length === 0) fail("batch_empty", "There is nothing to process.");
  const cost = estimate(operation, ids.length);
  if (!cost.enough) {
    fail(
      "insufficient_credits",
      `This batch costs ${cost.cost} credits and the account has ${credits} left. Add credits in the Photos.sh console, or process fewer files.`,
    );
  }
  for (const id of ids) change(id, { status: "queued", error: null, output: null, output_name: null });
  running = true;
  cancelled = false;
  lastOperation = operation;
  void runBatch(operation, ids);
  return cost;
}

// --- editor ---

let editor: (EditorSnapshot & { looks: Map<number, string> }) | null = null;
let session = 0;

function picture(look: string): string {
  const backdrop = look.includes("cutout") ? "" : `<rect width="640" height="480" fill="#d9d4c7"/>`;
  const caption = look.includes("text") ? "" : `<text x="24" y="452" font-family="sans-serif" font-size="34" font-weight="700" fill="#b02525">SALE −30%</text>`;
  const speck = look.includes("erased") ? "" : `<circle cx="520" cy="110" r="22" fill="#3b5bdb"/>`;
  const white = look.includes("fixed") ? `<rect width="640" height="480" fill="#fff"/>` : "";
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480" width="640" height="480">${backdrop}${white}<ellipse cx="320" cy="400" rx="150" ry="18" fill="rgba(0,0,0,.18)"/><rect x="230" y="120" width="180" height="270" rx="28" fill="#c2410c"/><rect x="262" y="70" width="116" height="64" rx="12" fill="#7c2d12"/><rect x="250" y="210" width="140" height="90" rx="6" fill="#fef3c7"/>${speck}${caption}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

function editorSnapshot(): EditorSnapshot | null {
  if (editor === null) return null;
  const current = editor.cursor === 0 ? editor.original : editor.steps[editor.cursor - 1]!;
  const extension = current.content_type.split("/")[1]?.replace("jpeg", "jpg") ?? "png";
  return {
    session: editor.session,
    name: editor.name,
    path: editor.path,
    original: editor.original,
    steps: editor.steps,
    cursor: editor.cursor,
    current,
    can_undo: editor.cursor > 0,
    can_redo: editor.cursor < editor.steps.length,
    suggested_name: `${editor.name.replace(/\.[^.]+$/, "")}-edited.${extension}`,
  };
}

function openEditor(path: string): EditorSnapshot {
  if (!/\.(jpe?g|png|webp|gif|tiff?|bmp|heic|heif|avif)$/i.test(path)) {
    fail("not_an_image", "That file is not a photo this app can open.");
  }
  session += 1;
  const original: StepInfo = {
    id: 0,
    label: "Original",
    content_type: "image/jpeg",
    width: 640,
    height: 480,
    bytes: 412_000,
    note: null,
    readings: null,
  };
  editor = {
    session,
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    original,
    steps: [],
    cursor: 0,
    current: original,
    can_undo: false,
    can_redo: false,
    suggested_name: "",
    looks: new Map([[0, "original"]]),
  };
  return editorSnapshot()!;
}

let editorCancelled = false;

async function applyTool(tool: EditorTool): Promise<EditorSnapshot> {
  needKey();
  if (editor === null) fail("nothing_open", "Open a photo first.");
  editorCancelled = false;
  await wait(900);
  if (editorCancelled) fail("cancelled", "cancelled");
  if (failing) fail(failing, "The mock was asked to fail", 402);
  const state = editor!;
  const before = state.looks.get(state.cursor === 0 ? 0 : state.steps[state.cursor - 1]!.id) ?? "";
  let label = "Erase";
  let look = `${before} erased`;
  let type = "image/png";
  let note: string | null = null;
  let readings: Readings | null = null;
  if (tool.tool === "select") {
    if (tool.points.length === 0) fail("invalid", "Click the object to cut out first.");
    label = "Select object";
    look = `${before} cutout`;
    credits -= PRICES.select ?? 0;
  } else if (tool.tool === "operation") {
    const op = tool.operation;
    if (op.kind === "vectorize") {
      fail("invalid", "This tool gives an SVG file, which the editor cannot work on. Use it in Batch.");
    }
    label = stepLabel(op);
    look = `${before} ${LOOK[op.kind]}`;
    note = NOTE[op.kind] ?? null;
    if ("format" in op && op.format) type = `image/${op.format === "jpg" ? "jpeg" : op.format}`;
    if (op.kind === "recipe") {
      const r = op.recipe;
      if (r.remove_background) look = `${before} cutout`;
      if (r.format) type = `image/${r.format === "jpg" ? "jpeg" : r.format}`;
      readings = mockReadings(r);
      note = readings ? "Barcode 3850104010017; 3 lines of text read" : null;
    }
    credits -= PRICES[choice(op.kind).priceKey] ?? 0;
  }
  const id = Math.max(0, ...state.steps.map((s) => s.id)) + 1;
  state.steps = [
    ...state.steps.slice(0, state.cursor),
    { id, label, content_type: type, width: 640, height: 480, bytes: 268_000, note, readings },
  ];
  state.cursor = state.steps.length;
  state.looks.set(id, look);
  return editorSnapshot()!;
}

async function check(marketplace: string | null): Promise<CheckReport> {
  needKey();
  await wait(600);
  const findings: CheckReport["findings"] = [
    { code: "soft", severity: "warning", message: "The photo is a little soft.", detail: "sharpness 0.42" },
  ];
  if (marketplace === null) return { metrics: { width: 640, height: 480, sharpness: 0.42 }, findings };
  return {
    metrics: { width: 640, height: 480 },
    findings,
    marketplace: {
      marketplace,
      title: "Amazon (main image)",
      passed: false,
      findings: [
        {
          code: "background_not_white",
          severity: "error",
          message: "The main image needs a pure white background (RGB 255, 255, 255).",
          detail: "62% of the border is white",
        },
        { code: "too_small", severity: "warning", message: "Under 1000 px on the longest side, so Amazon cannot offer zoom." },
      ],
      not_checked: [],
      notes: ["No props that are not included in the sale."],
    },
  };
}

// --- the bridge ---

type Args = Record<string, unknown>;

async function handle(command: string, args: Args): Promise<unknown> {
  switch (command) {
    case "settings_get":
      return settings;
    case "settings_set": {
      const next = args.settings as Settings;
      if (!/^https?:\/\//.test(next.base_url)) fail("invalid", "the base URL must start with http:// or https://");
      settings = next;
      return settings;
    }
    case "key_status":
      return keyStatus();
    case "key_set": {
      const pasted = String(args.key).trim();
      if (pasted === "") fail("invalid", "Paste an API key first.");
      key = pasted;
      return keyStatus();
    }
    case "key_remove":
      key = null;
      return keyStatus();
    case "account":
      await wait(250);
      return account();
    case "catalog":
      needKey();
      return {
        presets: [
          { name: "marketplace", description: "Marketplace main image: pure white, square, product fills 85%, JPEG under 2 MB." },
          { name: "studio", description: "Studio look for shop pages: soft grey gradient, contact shadow, WebP." },
          { name: "web", description: "Web-ready photo: longest side 1600 px, WebP under 300 KB." },
        ],
        marketplaces: [
          { name: "amazon", title: "Amazon (main image)" },
          { name: "ebay", title: "eBay" },
          { name: "etsy", title: "Etsy" },
        ],
      };
    case "pick_images":
      return ["/home/you/Shop/teapot.jpg", "/home/you/Shop/chair.png"];
    case "pick_image":
      return "/home/you/Shop/bottle.jpg";
    case "pick_folder":
      return String(args.title).startsWith("Save") ? "/home/you/Results" : "/home/you/Shop/autumn";
    case "batch_snapshot":
      return snapshot();
    case "batch_add":
      await wait(150);
      addPaths(args.paths as string[]);
      return snapshot();
    case "batch_remove":
      items = items.filter((item) => item.id !== args.id);
      return snapshot();
    case "batch_clear":
      items = [];
      return snapshot();
    case "batch_reset":
      items = items.map((item) => ({ ...item, status: "queued", output: null, output_name: null, error: null }));
      return snapshot();
    case "batch_estimate":
      await wait(200);
      return estimate(args.operation as Operation, items.filter((i) => i.status !== "done").length);
    case "batch_start":
      await wait(300);
      return startBatch(args.operation as Operation, ["queued", "failed", "skipped", "cancelled"]);
    case "batch_retry":
      if (lastOperation === null) fail("batch_no_earlier_run", "There is no earlier run to retry.");
      return startBatch(lastOperation!, ["failed", "skipped", "cancelled"]);
    case "batch_cancel":
      cancelled = running;
      return running;
    case "batch_reveal":
      return null;
    case "editor_open":
      return openEditor(String(args.path));
    case "editor_close":
      editor = null;
      return null;
    case "editor_snapshot":
      return editorSnapshot();
    case "editor_apply":
      return applyTool(args.tool as EditorTool);
    case "editor_check":
      return check((args.marketplace as string | null) ?? null);
    case "editor_cancel":
      editorCancelled = true;
      return null;
    case "editor_undo":
      if (editor && editor.cursor > 0) editor.cursor -= 1;
      return editorSnapshot();
    case "editor_redo":
      if (editor && editor.cursor < editor.steps.length) editor.cursor += 1;
      return editorSnapshot();
    case "editor_go_to":
      if (editor) editor.cursor = Math.min(Number(args.cursor), editor.steps.length);
      return editorSnapshot();
    case "editor_pick_save":
      return params.get("save") === "original" ? editor?.path ?? null : `/home/you/Results/${editorSnapshot()?.suggested_name}`;
    case "editor_save":
      if (args.path === editor?.path && !args.replaceOriginal) {
        fail("confirm_replace_original", "That would replace the original photo.");
      }
      return args.path;
    case "update_check": {
      const version = params.get("update");
      if (version === "fail") fail("update_failed", "Could not check for updates: error sending request");
      if (version === null) return null;
      return { version, current_version: __APP_VERSION__, notes: "- Edge shift, hardness and smoothing for remove background.\n- A choice of model for remove background." };
    }
    case "update_install":
      // A download in ten steps; a real install then restarts the app.
      for (let step = 1; step <= 10; step += 1) {
        await new Promise((resolve) => setTimeout(resolve, 300));
        await emit("update:progress", { downloaded: step * 1_200_000, total: 12_000_000 });
      }
      return fail("update_failed", "The mock cannot restart the app.");
    case "update_open_releases":
      return null;
    default:
      throw new Error(`the mock does not know the command ${command}`);
  }
}

export function installMock() {
  mockWindows("main");
  mockIPC((command, args) => handle(command, (args ?? {}) as Args), { shouldMockEvents: true });
  setPreviewResolver((key) => {
    const [kind, a, b] = key.split("/");
    if (kind === "editor") return picture(editor?.looks.get(Number(b)) ?? "original");
    const item = items.find((i) => i.id === Number(a));
    if (item?.name.endsWith(".heic")) return "data:image/unsupported,";
    return picture(b === "result" ? "cutout text" : "original");
  });
  Object.assign(window, {
    __mock: {
      drop: (paths: string[]) => emit("tauri://drag-drop", { paths, position: { x: 0, y: 0 } }),
      dragEnter: (paths: string[]) => emit("tauri://drag-enter", { paths, position: { x: 0, y: 0 } }),
      dragLeave: () => emit("tauri://drag-leave"),
    },
  });
}
