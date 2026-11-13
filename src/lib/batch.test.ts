import { describe, expect, it } from "vitest";

import { abortedNote, applyItem, count, estimateText, progress, retryLabel, retryable, summaryText } from "./batch";
import { item } from "./testing";
import type { BatchSummary, Estimate } from "./types";

describe("applyItem", () => {
  it("replaces the item with its newer state", () => {
    const next = applyItem([item(1), item(2)], item(2, "running", 1));
    expect(next.map((i) => i.status)).toEqual(["queued", "running"]);
  });

  it("ignores an older state that arrives late", () => {
    const next = applyItem([item(1, "done", 3)], item(1, "running", 2));
    expect(next[0]?.status).toBe("done");
  });

  it("adds an item it has not seen", () => {
    expect(applyItem([item(1)], item(5, "running", 1))).toHaveLength(2);
  });
});

describe("count and progress", () => {
  const items = [
    item(1, "done"),
    item(2, "done"),
    item(3, "failed"),
    item(4, "running"),
    item(5),
    item(6, "skipped"),
    item(7, "cancelled"),
  ];

  it("names what a retry takes", () => {
    expect(retryable(count(items))).toBe(3);
    expect(retryLabel(count(items))).toBe("Retry failed, cancelled and not run (3)");
    expect(retryLabel(count([item(1, "failed"), item(2, "done")]))).toBe("Retry failed (1)");
    expect(retryLabel(count([item(1, "cancelled"), item(2, "skipped"), item(3, "skipped")]))).toBe(
      "Retry cancelled and not run (3)",
    );
  });

  it("counts by status", () => {
    expect(count(items)).toEqual({ total: 7, queued: 1, running: 1, done: 2, failed: 1, skipped: 1, cancelled: 1 });
  });

  it("measures the run, not what was done before it", () => {
    expect(progress(count(items), 0)).toEqual({ settled: 5, of: 7, ratio: 5 / 7 });
    // One file was already done when this run began.
    expect(progress(count(items), 1)).toEqual({ settled: 4, of: 6, ratio: 4 / 6 });
    expect(progress(count([]), 0)).toEqual({ settled: 0, of: 0, ratio: 0 });
  });
});

describe("estimateText", () => {
  const base: Estimate = { files: 12, unlimited: false, price: 4, cost: 48, credits: 100, enough: true };

  it("shows the sum and what is left", () => {
    expect(estimateText(base)).toBe("12 files × 4 credits = 48 credits. 100 credits left.");
    expect(estimateText({ ...base, files: 1, price: 1, cost: 1, credits: 1 })).toBe(
      "1 file × 1 credit = 1 credit. 1 credit left.",
    );
  });

  it("says when the balance is too low", () => {
    expect(estimateText({ ...base, credits: 10, enough: false })).toBe(
      "12 files × 4 credits = 48 credits, but only 10 credits left. Add credits in the Photos.sh console, or process fewer files.",
    );
  });

  it("says when nothing is counted", () => {
    expect(estimateText({ ...base, unlimited: true, price: 0, cost: 0, credits: null })).toBe(
      "12 files. This key is unlimited: no credits are counted.",
    );
    expect(estimateText({ ...base, price: 0, cost: 0 })).toBe("12 files: free. 100 credits left.");
    expect(estimateText({ ...base, credits: null })).toBe("12 files × 4 credits = 48 credits.");
  });
});

describe("abortedNote", () => {
  it("is honest about what a cancelled request may have cost", () => {
    expect(abortedNote(1)).toMatch(/normally not charged.*just finished.*may still have cost/);
    expect(abortedNote(3)).toMatch(/^The 3 photos that were in progress/);
  });
});

describe("summaryText", () => {
  const base: BatchSummary = {
    total: 5,
    done: 5,
    failed: 0,
    skipped: 0,
    aborted: 0,
    cancelled: false,
    stopped: null,
    output_dir: "/out",
  };

  it("reports a clean run", () => {
    expect(summaryText(base)).toBe("All 5 files done.");
  });

  it("reports failures", () => {
    expect(summaryText({ ...base, done: 3, failed: 2 })).toBe("3 of 5 files done, 2 failed.");
  });

  it("reports a cancelled and a stopped run", () => {
    expect(summaryText({ ...base, done: 2, aborted: 2, skipped: 1, cancelled: true })).toBe(
      "Cancelled. 2 of 5 files done, 2 cancelled while in progress, 1 not run.",
    );
    // A cancel that came too late to stop anything is still not "all done".
    expect(summaryText({ ...base, done: 4, aborted: 1, cancelled: true })).toMatch(/^Cancelled\. 4 of 5/);
    expect(
      summaryText({ ...base, done: 1, failed: 1, skipped: 3, stopped: { code: "payment_required", message: "" } }),
    ).toBe("Stopped. 1 of 5 files done, 1 failed, 3 not run.");
  });
});
