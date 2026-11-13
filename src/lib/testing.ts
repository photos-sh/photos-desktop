// Test support: a core whose commands are spies, and events fired by hand.

import { vi, type Mock } from "vitest";

import type { Core, events } from "./ipc";
import type {
  AppError,
  BatchItem,
  BatchSnapshot,
  BatchSummary,
  EditorSnapshot,
  ItemStatus,
  StepInfo,
} from "./types";

export function item(id: number, status: ItemStatus = "queued", version = 0): BatchItem {
  return {
    id,
    source: `/photos/${id}.jpg`,
    name: `${id}.jpg`,
    status,
    output: null,
    output_name: null,
    width: null,
    height: null,
    bytes: null,
    note: null,
    error: null,
    version,
  };
}

export function batchSnapshot(items: BatchItem[], running = false): BatchSnapshot {
  return { items, running, default_output_dir: items.length > 0 ? "/photos/photos-sh" : null };
}

export function appError(code: string, message = code): AppError {
  return { code, message, status: null, retry_after: null };
}

function step(id: number, label: string): StepInfo {
  return { id, label, content_type: "image/png", width: 100, height: 80, bytes: 1000, note: null };
}

/** An editor snapshot with `steps` steps applied and the cursor at `cursor`. */
export function editorSnapshot(steps = 0, cursor = steps, session = 1): EditorSnapshot {
  const all = Array.from({ length: steps }, (_, n) => step(n + 1, `Step ${n + 1}`));
  const original = step(0, "Original");
  return {
    session,
    name: "shoe.png",
    path: "/photos/shoe.png",
    original,
    steps: all,
    cursor,
    current: cursor === 0 ? original : (all[cursor - 1] ?? original),
    can_undo: cursor > 0,
    can_redo: cursor < steps,
    suggested_name: "shoe-edited.png",
  };
}

export type FakeCore = { [K in keyof Core]: Mock<Core[K]> };

/** Every command is a spy that rejects until a test gives it an answer. */
export function fakeCore(): FakeCore {
  return new Proxy({} as FakeCore, {
    get(target, name: string) {
      const key = name as keyof Core;
      target[key] ??= vi.fn(() => Promise.reject(appError("internal", `${name} was not expected`))) as never;
      return target[key];
    },
  });
}

type Events = typeof events;

/** Events a test fires itself. */
export function fakeEvents() {
  const handlers: {
    item?: (item: BatchItem) => void;
    finished?: (summary: BatchSummary) => void;
    error?: (error: AppError) => void;
  } = {};
  const stopped = vi.fn();
  const events: Events = {
    batchItem: (handler) => {
      handlers.item = handler;
      return Promise.resolve(stopped);
    },
    batchFinished: (handler) => {
      handlers.finished = handler;
      return Promise.resolve(stopped);
    },
    batchError: (handler) => {
      handlers.error = handler;
      return Promise.resolve(stopped);
    },
  };
  return {
    events,
    stopped,
    item: (item: BatchItem) => handlers.item?.(item),
    finished: (summary: BatchSummary) => handlers.finished?.(summary),
    error: (error: AppError) => handlers.error?.(error),
  };
}
