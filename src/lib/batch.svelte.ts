// The batch view's state: the file list, the run in progress and its cost.

import { applyItem, count, progress, retryable } from "./batch";
import { core as realCore, events as realEvents, toAppError, type Core } from "./ipc";
import type { AppError, BatchItem, BatchSnapshot, BatchSummary, Estimate, Operation } from "./types";

type Events = typeof realEvents;

export class BatchStore {
  items = $state<BatchItem[]>([]);
  running = $state(false);
  /** Asked to stop; the core is dropping the requests in flight. */
  cancelling = $state(false);
  /** Reading dropped files and folders. */
  adding = $state(false);
  /** Checking the batch before it starts. */
  starting = $state(false);
  defaultOutputDir = $state<string | null>(null);
  /** The folder the user chose; none means the default. */
  chosenOutputDir = $state<string | null>(null);
  estimate = $state<Estimate | null>(null);
  estimateError = $state<AppError | null>(null);
  estimating = $state(false);
  summary = $state<BatchSummary | null>(null);
  error = $state<AppError | null>(null);
  /** What the last drop or pick added. */
  added = $state<string | null>(null);
  /** Files that were done before the current run began. */
  doneBefore = $state(0);

  counts = $derived(count(this.items));
  progress = $derived(progress(this.counts, this.doneBefore));

  readonly #core: Core;
  readonly #events: Events;
  #estimateRequest = 0;

  constructor(core: Core = realCore, events: Events = realEvents) {
    this.#core = core;
    this.#events = events;
  }

  get outputDir(): string | null {
    return this.chosenOutputDir ?? this.defaultOutputDir;
  }

  /** Files a run would take: everything not done. */
  get pending(): number {
    return this.counts.total - this.counts.done;
  }

  /** Whether "retry failed" has something to do. */
  get canRetry(): boolean {
    return !this.running && this.summary !== null && retryable(this.counts) > 0;
  }

  /** Listens for the core's progress and reads the list. Returns how to stop. */
  async init(): Promise<() => void> {
    const stops = await Promise.all([
      this.#events.batchItem((item) => {
        this.items = applyItem(this.items, item);
      }),
      this.#events.batchFinished((summary) => {
        this.summary = summary;
        this.running = false;
        this.cancelling = false;
      }),
      this.#events.batchError((error) => {
        this.error = error;
        this.running = false;
        this.cancelling = false;
      }),
    ]);
    try {
      this.#take(await this.#core.batchSnapshot());
    } catch (e) {
      this.error = toAppError(e);
    }
    return () => stops.forEach((stop) => stop());
  }

  #take(snapshot: BatchSnapshot) {
    this.items = snapshot.items;
    this.running = snapshot.running;
    this.defaultOutputDir = snapshot.default_output_dir;
  }

  async #change(action: () => Promise<BatchSnapshot>): Promise<boolean> {
    this.error = null;
    try {
      this.#take(await action());
      return true;
    } catch (e) {
      this.error = toAppError(e);
      return false;
    }
  }

  /** Adds dropped or picked files and folders. */
  async add(paths: string[]): Promise<void> {
    if (paths.length === 0) return;
    const before = this.items.length;
    this.adding = true;
    this.added = null;
    const ok = await this.#change(() => this.#core.batchAdd(paths));
    this.adding = false;
    if (!ok) return;
    const added = this.items.length - before;
    this.added =
      added === 0
        ? "No new photos in what was added. JPEG, PNG, WebP, GIF, TIFF, BMP, HEIC and AVIF files are taken."
        : `${added} ${added === 1 ? "photo" : "photos"} added.`;
    if (added > 0) this.summary = null;
  }

  async pickFiles(): Promise<void> {
    try {
      await this.add(await this.#core.pickImages());
    } catch (e) {
      this.error = toAppError(e);
    }
  }

  async pickFolder(): Promise<void> {
    try {
      const folder = await this.#core.pickFolder("Add a folder of photos");
      if (folder !== null) await this.add([folder]);
    } catch (e) {
      this.error = toAppError(e);
    }
  }

  async pickOutputDir(): Promise<void> {
    try {
      const folder = await this.#core.pickFolder("Save results in");
      if (folder !== null) this.chosenOutputDir = folder;
    } catch (e) {
      this.error = toAppError(e);
    }
  }

  async remove(id: number): Promise<void> {
    await this.#change(() => this.#core.batchRemove(id));
  }

  async clear(): Promise<void> {
    if (await this.#change(() => this.#core.batchClear())) {
      this.summary = null;
      this.added = null;
      this.estimate = null;
      this.chosenOutputDir = null;
    }
  }

  /** Puts every file back in the queue, to run something else on them. */
  async reset(): Promise<void> {
    if (await this.#change(() => this.#core.batchReset())) this.summary = null;
  }

  /** Asks what the operation would cost on the files not yet done. */
  async refreshEstimate(operation: Operation | null): Promise<void> {
    const request = ++this.#estimateRequest;
    if (operation === null || this.pending === 0) {
      this.estimate = null;
      this.estimateError = null;
      this.estimating = false;
      return;
    }
    this.estimating = true;
    try {
      const estimate = await this.#core.batchEstimate(operation);
      if (request !== this.#estimateRequest) return;
      this.estimate = estimate;
      this.estimateError = null;
    } catch (e) {
      if (request !== this.#estimateRequest) return;
      this.estimate = null;
      this.estimateError = toAppError(e);
    } finally {
      if (request === this.#estimateRequest) this.estimating = false;
    }
  }

  async #run(start: () => Promise<Estimate>): Promise<void> {
    if (this.running || this.starting) return;
    this.starting = true;
    this.error = null;
    this.summary = null;
    this.added = null;
    this.doneBefore = this.counts.done;
    try {
      this.estimate = await start();
      this.running = true;
    } catch (e) {
      this.error = toAppError(e);
    } finally {
      this.starting = false;
    }
  }

  /** Starts the batch. The core refuses it when the balance is too low. */
  start(operation: Operation): Promise<void> {
    return this.#run(() => this.#core.batchStart(operation, this.chosenOutputDir));
  }

  /** Runs the last operation again on what failed, was cancelled or was not run. */
  retry(): Promise<void> {
    return this.#run(() => this.#core.batchRetry());
  }

  async cancel(): Promise<void> {
    if (!this.running) return;
    this.cancelling = true;
    try {
      if (!(await this.#core.batchCancel())) this.cancelling = false;
    } catch (e) {
      this.cancelling = false;
      this.error = toAppError(e);
    }
  }

  async reveal(item: BatchItem): Promise<void> {
    try {
      await this.#core.batchReveal(item.id, item.output === null);
    } catch (e) {
      this.error = toAppError(e);
    }
  }
}
