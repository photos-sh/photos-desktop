import { describe, expect, it } from "vitest";

import { BatchStore } from "./batch.svelte";
import { appError, batchSnapshot, fakeCore, fakeEvents, item } from "./testing";
import type { BatchSummary, Estimate, Operation } from "./types";

const UPSCALE: Operation = { kind: "upscale", scale: 2 };
const ESTIMATE: Estimate = { files: 2, unlimited: false, price: 4, cost: 8, credits: 100, enough: true };
const SUMMARY: BatchSummary = {
  total: 2,
  done: 1,
  failed: 1,
  skipped: 0,
  aborted: 0,
  cancelled: false,
  stopped: null,
  output_dir: "/photos/photos-sh",
};

async function ready(items = [item(1), item(2)]) {
  const core = fakeCore();
  const fired = fakeEvents();
  core.batchSnapshot.mockResolvedValue(batchSnapshot(items));
  const store = new BatchStore(core, fired.events);
  const stop = await store.init();
  return { core, fired, store, stop };
}

describe("BatchStore", () => {
  it("reads the list at start and stops listening when told", async () => {
    const { store, stop, fired } = await ready();
    expect(store.items).toHaveLength(2);
    expect(store.outputDir).toBe("/photos/photos-sh");
    expect(store.pending).toBe(2);
    stop();
    expect(fired.stopped).toHaveBeenCalledTimes(3);
  });

  it("adds dropped files and says how many were photos", async () => {
    const { store, core } = await ready([]);
    core.batchAdd.mockResolvedValue(batchSnapshot([item(1), item(2), item(3)]));
    await store.add(["/photos"]);
    expect(core.batchAdd).toHaveBeenCalledWith(["/photos"]);
    expect(store.added).toBe("3 photos added.");
    expect(store.adding).toBe(false);

    // The same drop again adds nothing new.
    await store.add(["/photos"]);
    expect(store.added).toMatch(/^No new photos/);
    // An empty drop does not bother the core.
    await store.add([]);
    expect(core.batchAdd).toHaveBeenCalledTimes(2);
  });

  it("picks files, a folder and an output folder through the core's dialogs", async () => {
    const { store, core } = await ready([]);
    core.pickImages.mockResolvedValue(["/photos/1.jpg"]);
    core.pickFolder.mockResolvedValueOnce("/photos/more").mockResolvedValueOnce("/results").mockResolvedValueOnce(null);
    core.batchAdd.mockResolvedValue(batchSnapshot([item(1)]));
    await store.pickFiles();
    await store.pickFolder();
    expect(core.batchAdd).toHaveBeenNthCalledWith(1, ["/photos/1.jpg"]);
    expect(core.batchAdd).toHaveBeenNthCalledWith(2, ["/photos/more"]);

    await store.pickOutputDir();
    expect(store.outputDir).toBe("/results");
    // A dismissed dialog changes nothing.
    await store.pickOutputDir();
    expect(store.chosenOutputDir).toBe("/results");
  });

  it("asks for the cost and ignores an answer that arrives too late", async () => {
    const { store, core } = await ready();
    let answerFirst: (estimate: Estimate) => void = () => {};
    core.batchEstimate
      .mockImplementationOnce(() => new Promise((resolve) => (answerFirst = resolve)))
      .mockResolvedValueOnce({ ...ESTIMATE, cost: 2, price: 1 });
    const first = store.refreshEstimate(UPSCALE);
    const second = store.refreshEstimate({ kind: "resize", width: 10 });
    await second;
    answerFirst(ESTIMATE);
    await first;
    expect(store.estimate?.cost).toBe(2);
    expect(store.estimating).toBe(false);

    core.batchEstimate.mockRejectedValueOnce(appError("unauthorized"));
    await store.refreshEstimate(UPSCALE);
    expect(store.estimate).toBeNull();
    expect(store.estimateError?.code).toBe("unauthorized");

    // Nothing to estimate without an operation.
    await store.refreshEstimate(null);
    expect(store.estimateError).toBeNull();
  });

  it("runs a batch: starts, follows the files, ends with the summary", async () => {
    const { store, core, fired } = await ready();
    core.batchStart.mockResolvedValue(ESTIMATE);
    await store.start(UPSCALE);
    expect(core.batchStart).toHaveBeenCalledWith(UPSCALE, null);
    expect(store.running).toBe(true);
    expect(store.progress).toEqual({ settled: 0, of: 2, ratio: 0 });

    fired.item(item(1, "running", 1));
    fired.item(item(1, "done", 2));
    expect(store.counts.done).toBe(1);
    expect(store.progress.settled).toBe(1);
    fired.item(item(2, "failed", 2));
    fired.finished(SUMMARY);
    expect(store.running).toBe(false);
    expect(store.summary).toEqual(SUMMARY);
    expect(store.canRetry).toBe(true);
    expect(store.pending).toBe(1);

    core.batchRetry.mockResolvedValue({ ...ESTIMATE, files: 1 });
    await store.retry();
    expect(store.running).toBe(true);
    expect(store.summary).toBeNull();
    // The file done in the first run is not part of this one.
    expect(store.progress.of).toBe(1);
  });

  it("uses the chosen output folder", async () => {
    const { store, core } = await ready();
    core.pickFolder.mockResolvedValue("/results");
    core.batchStart.mockResolvedValue(ESTIMATE);
    await store.pickOutputDir();
    await store.start(UPSCALE);
    expect(core.batchStart).toHaveBeenCalledWith(UPSCALE, "/results");
  });

  it("shows why a batch did not start", async () => {
    const { store, core } = await ready();
    core.batchStart.mockRejectedValue(appError("insufficient_credits", "This batch costs 8 credits."));
    await store.start(UPSCALE);
    expect(store.running).toBe(false);
    expect(store.starting).toBe(false);
    expect(store.error?.code).toBe("insufficient_credits");
  });

  it("cancels: waits for the core to say the run is over", async () => {
    const { store, core, fired } = await ready();
    core.batchStart.mockResolvedValue(ESTIMATE);
    core.batchCancel.mockResolvedValue(true);
    await store.cancel();
    expect(core.batchCancel).not.toHaveBeenCalled();

    await store.start(UPSCALE);
    await store.cancel();
    expect(store.cancelling).toBe(true);
    expect(store.running).toBe(true);
    // The core drops what is in flight: one file comes back cancelled, the
    // other was never started.
    fired.item(item(1, "running", 1));
    fired.item(item(1, "cancelled", 2));
    fired.item(item(2, "skipped", 1));
    fired.finished({ ...SUMMARY, done: 0, failed: 0, aborted: 1, skipped: 1, cancelled: true });
    expect(store.cancelling).toBe(false);
    expect(store.running).toBe(false);
    expect(store.counts.cancelled).toBe(1);
    expect(store.progress).toEqual({ settled: 2, of: 2, ratio: 1 });
    expect(store.summary?.aborted).toBe(1);
    // Both can be run again, with the same operation.
    expect(store.canRetry).toBe(true);
    expect(store.pending).toBe(2);
    core.batchRetry.mockResolvedValue(ESTIMATE);
    await store.retry();
    expect(core.batchRetry).toHaveBeenCalledTimes(1);
    expect(store.running).toBe(true);
  });

  it("reports a run the core could not go on with", async () => {
    const { store, core, fired } = await ready();
    core.batchStart.mockResolvedValue(ESTIMATE);
    await store.start(UPSCALE);
    fired.error(appError("io", "Could not create the output folder"));
    expect(store.running).toBe(false);
    expect(store.error?.code).toBe("io");
  });

  it("clears and resets the list", async () => {
    const { store, core, fired } = await ready([item(1, "done")]);
    fired.finished(SUMMARY);
    core.batchReset.mockResolvedValue(batchSnapshot([item(1)]));
    await store.reset();
    expect(store.items[0]?.status).toBe("queued");
    expect(store.summary).toBeNull();

    core.batchClear.mockRejectedValueOnce(appError("batch_running"));
    await store.clear();
    expect(store.items).toHaveLength(1);
    expect(store.error?.code).toBe("batch_running");

    core.batchClear.mockResolvedValue(batchSnapshot([]));
    await store.clear();
    expect(store.items).toHaveLength(0);
    expect(store.error).toBeNull();
    expect(store.outputDir).toBeNull();
  });

  it("reveals the result, or the source when there is none", async () => {
    const { store, core } = await ready();
    core.batchReveal.mockResolvedValue(undefined);
    await store.reveal(item(1));
    expect(core.batchReveal).toHaveBeenLastCalledWith(1, true);
    await store.reveal({ ...item(2, "done"), output: "/photos/photos-sh/2.png" });
    expect(core.batchReveal).toHaveBeenLastCalledWith(2, false);
  });
});
