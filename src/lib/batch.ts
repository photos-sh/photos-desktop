// The batch view's logic, apart from the store that holds its state.

import { plural } from "./format";
import type { BatchItem, BatchSummary, Estimate, ItemStatus } from "./types";

/** The list with one item replaced by its newer state. */
export function applyItem(items: readonly BatchItem[], changed: BatchItem): BatchItem[] {
  let found = false;
  const next = items.map((item) => {
    if (item.id !== changed.id) return item;
    found = true;
    // Events may overtake each other: the higher version wins.
    return changed.version >= item.version ? changed : item;
  });
  return found ? next : [...items, changed];
}

export type Counts = Record<ItemStatus, number> & { total: number };

export function count(items: readonly BatchItem[]): Counts {
  const counts: Counts = {
    total: items.length,
    queued: 0,
    running: 0,
    done: 0,
    failed: 0,
    skipped: 0,
    cancelled: 0,
  };
  for (const item of items) counts[item.status] += 1;
  return counts;
}

/** How far a run is: files settled (done, failed, cancelled or skipped) of the files in
 *  the run. Files done before the run started are not part of it. */
export function progress(counts: Counts, doneBefore: number): { settled: number; of: number; ratio: number } {
  const of = Math.max(counts.total - doneBefore, 0);
  const settled = Math.min(Math.max(counts.done - doneBefore, 0) + counts.failed + counts.skipped + counts.cancelled,
    of,
  );
  return { settled, of, ratio: of === 0 ? 0 : settled / of };
}

/** The line under the operation: what the batch costs and what is left. */
export function estimateText(estimate: Estimate): string {
  const files = plural(estimate.files, "file", "files");
  if (estimate.unlimited) {
    return `${files}. This key is unlimited: no credits are counted.`;
  }
  const cost =
    estimate.cost === 0
      ? `${files}: free`
      : `${files} × ${plural(estimate.price, "credit", "credits")} = ${plural(estimate.cost, "credit", "credits")}`;
  if (estimate.credits == null) return `${cost}.`;
  const left = `${plural(estimate.credits, "credit", "credits")} left`;
  if (!estimate.enough) {
    return `${cost}, but only ${left}. Add credits in the Photos.sh console, or process fewer files.`;
  }
  return `${cost}. ${left}.`;
}

/** What to say when a run ends. */
export function summaryText(summary: BatchSummary): string {
  const parts = [`${summary.done} of ${plural(summary.total, "file", "files")} done`];
  if (summary.failed > 0) parts.push(`${summary.failed} failed`);
  if (summary.aborted > 0) parts.push(`${summary.aborted} cancelled while in progress`);
  if (summary.skipped > 0) parts.push(`${summary.skipped} not run`);
  const line = `${parts.join(", ")}.`;
  if (summary.stopped) return `Stopped. ${line}`;
  if (summary.cancelled) return `Cancelled. ${line}`;
  return summary.failed === 0 && summary.skipped === 0 && summary.aborted === 0
    ? `All ${plural(summary.total, "file", "files")} done.`
    : line;
}

export const STATUS_LABEL: Record<ItemStatus, string> = {
  queued: "Waiting",
  running: "Working",
  done: "Done",
  failed: "Failed",
  skipped: "Not run",
  cancelled: "Cancelled",
};

/** Files "retry failed" would run again: failed, cancelled or not run. */
export function retryable(counts: Counts): number {
  return counts.failed + counts.cancelled + counts.skipped;
}

/** The retry button's words: it says what it takes. */
export function retryLabel(counts: Counts): string {
  const kinds = [
    counts.failed > 0 ? "failed" : null,
    counts.cancelled > 0 ? "cancelled" : null,
    counts.skipped > 0 ? "not run" : null,
  ].filter((kind) => kind !== null);
  return `Retry ${kinds.join(kinds.length > 2 ? ", " : " and ").replace(/, ([^,]*)$/, " and $1")} (${retryable(counts)})`;
}

// What cancelling costs. The service does not count a request whose
// connection closed before it answered, but it cannot stop work it has
// begun, and an answer that was already on its way is counted.
export const CANCEL_HINT =
  "Cancel stops at once. Photos being processed are given up and nothing is saved for them. They are normally not charged, but one the service had just finished may still cost its credits.";
export const CANCELLED_ITEM =
  "Cancelled while in progress: nothing was saved. Normally not charged, unless the service had just finished it.";
export function abortedNote(aborted: number): string {
  return aborted === 1
    ? "The photo that was in progress is normally not charged; if the service had just finished it, it may still have cost its credits. Check the balance."
    : `The ${aborted} photos that were in progress are normally not charged; any the service had just finished may still have cost their credits. Check the balance.`;
}
