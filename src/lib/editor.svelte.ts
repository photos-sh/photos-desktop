// The editor view's state: the open photo, its steps, and what is running.

import { core as realCore, toAppError, type Core } from "./ipc";
import type {
  AppError,
  CheckReport,
  EditorSnapshot,
  Format,
  Operation,
  Point,
  SelectPick,
  Stroke,
} from "./types";

/** The most points "select" takes. */
export const MAX_SELECT_POINTS = 32;

/** What is said after a running tool is cancelled. The service does not
 *  count a request whose connection closed before it answered, but one it
 *  had just finished is counted. */
export const CANCELLED_TOOL =
  "Cancelled: the photo is unchanged. The tool is normally not charged, unless the service had just finished it.";

export class EditorStore {
  snapshot = $state<EditorSnapshot | null>(null);
  /** What is being done right now ("Removing the background…"), if anything. */
  busy = $state<string | null>(null);
  error = $state<AppError | null>(null);
  /** A short confirmation ("Saved as …"). */
  notice = $state<string | null>(null);
  report = $state<CheckReport | null>(null);
  /** The brush strokes painted for the eraser, in the photo's pixels. */
  strokes = $state<Stroke[]>([]);
  /** The points clicked for "select", in the photo's pixels. */
  points = $state<Point[]>([]);
  /** A save waiting for "yes, replace the original". */
  confirmReplace = $state<string | null>(null);

  readonly #core: Core;

  constructor(core: Core = realCore) {
    this.#core = core;
  }

  get isOpen(): boolean {
    return this.snapshot !== null;
  }

  async init(): Promise<void> {
    try {
      this.snapshot = await this.#core.editorSnapshot();
    } catch (e) {
      this.error = toAppError(e);
    }
  }

  #show(snapshot: EditorSnapshot | null) {
    const moved =
      snapshot?.session !== this.snapshot?.session || snapshot?.current.id !== this.snapshot?.current.id;
    this.snapshot = snapshot;
    if (moved) {
      // The picture changed: what was painted, clicked or checked was about
      // another one.
      this.strokes = [];
      this.points = [];
      this.report = null;
    }
  }

  async #do(label: string | null, action: () => Promise<void>): Promise<boolean> {
    if (this.busy !== null) return false;
    this.busy = label;
    this.error = null;
    this.notice = null;
    try {
      await action();
      return true;
    } catch (e) {
      const error = toAppError(e);
      // Cancelling is not a failure, but it has a cost worth knowing.
      if (error.code === "cancelled") this.notice = CANCELLED_TOOL;
      else this.error = error;
      return false;
    } finally {
      this.busy = null;
    }
  }

  open(path: string): Promise<boolean> {
    return this.#do("Opening…", async () => {
      this.#show(await this.#core.editorOpen(path));
    });
  }

  async pick(): Promise<void> {
    try {
      const path = await this.#core.pickImage();
      if (path !== null) await this.open(path);
    } catch (e) {
      this.error = toAppError(e);
    }
  }

  async close(): Promise<void> {
    await this.cancel();
    try {
      await this.#core.editorClose();
    } catch (e) {
      this.error = toAppError(e);
      return;
    }
    this.#show(null);
    this.error = null;
    this.notice = null;
  }

  /** Applies an operation to the current picture; its result is the next step. */
  apply(operation: Operation, doing: string): Promise<boolean> {
    return this.#do(doing, async () => {
      this.#show(await this.#core.editorApply({ tool: "operation", operation }));
    });
  }

  /** Erases what the brush painted. */
  erase(format: Format | null = null, quality: number | null = null): Promise<boolean> {
    const strokes = $state.snapshot(this.strokes);
    return this.#do("Erasing…", async () => {
      this.#show(
        await this.#core.editorApply({
          tool: "erase",
          strokes,
          ...(format ? { format } : {}),
          ...(quality != null ? { quality } : {}),
        }),
      );
    });
  }

  /** Cuts out the object at the points clicked. `auto` takes the surest of
   *  the model's answers and is the API's default, so it is not sent. */
  select(pick: SelectPick = "auto"): Promise<boolean> {
    const points = $state.snapshot(this.points);
    return this.#do("Cutting out the object…", async () => {
      this.#show(
        await this.#core.editorApply({
          tool: "select",
          points,
          ...(pick !== "auto" ? { pick } : {}),
        }),
      );
    });
  }

  check(marketplace: string | null, subject: boolean, text: boolean): Promise<boolean> {
    return this.#do("Checking…", async () => {
      this.report = await this.#core.editorCheck(marketplace, subject, text);
    });
  }

  /** Stops the tool that is running: its request is dropped and no step is added. */
  async cancel(): Promise<void> {
    if (this.busy === null) return;
    try {
      await this.#core.editorCancel();
    } catch (e) {
      this.error = toAppError(e);
    }
  }

  async #move(action: () => Promise<EditorSnapshot>): Promise<void> {
    if (this.busy !== null) return;
    this.error = null;
    this.notice = null;
    try {
      this.#show(await action());
    } catch (e) {
      this.error = toAppError(e);
    }
  }

  undo(): Promise<void> {
    return this.#move(() => this.#core.editorUndo());
  }

  redo(): Promise<void> {
    return this.#move(() => this.#core.editorRedo());
  }

  goTo(cursor: number): Promise<void> {
    return this.#move(() => this.#core.editorGoTo(cursor));
  }

  /** Asks where to save, then saves. The original is replaced only after a
   *  second, explicit yes (`confirmSave`). */
  async save(): Promise<void> {
    if (this.busy !== null || this.snapshot === null) return;
    this.error = null;
    this.notice = null;
    let path: string | null;
    try {
      path = await this.#core.editorPickSave();
    } catch (e) {
      this.error = toAppError(e);
      return;
    }
    if (path !== null) await this.#saveTo(path, false);
  }

  async #saveTo(path: string, replaceOriginal: boolean): Promise<void> {
    try {
      const written = await this.#core.editorSave(path, replaceOriginal);
      this.notice = `Saved as ${written}`;
    } catch (e) {
      const error = toAppError(e);
      if (error.code === "confirm_replace_original") this.confirmReplace = path;
      else this.error = error;
    }
  }

  async confirmSave(yes: boolean): Promise<void> {
    const path = this.confirmReplace;
    this.confirmReplace = null;
    if (yes && path !== null) await this.#saveTo(path, true);
  }

  // --- the brush ---

  addStroke(stroke: Stroke) {
    if (stroke.points.length > 0) this.strokes = [...this.strokes, stroke];
  }

  undoStroke() {
    this.strokes = this.strokes.slice(0, -1);
  }

  clearStrokes() {
    this.strokes = [];
  }

  // --- the points clicked ---

  /** Adds a point, up to as many as the API takes. */
  addPoint(point: Point) {
    if (this.points.length < MAX_SELECT_POINTS) this.points = [...this.points, point];
  }

  undoPoint() {
    this.points = this.points.slice(0, -1);
  }

  clearPoints() {
    this.points = [];
  }
}
