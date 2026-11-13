// The core's commands, typed. Every call the UI makes goes through here.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  Account,
  AppError,
  BatchItem,
  BatchSnapshot,
  BatchSummary,
  Catalog,
  CheckReport,
  EditorSnapshot,
  EditorTool,
  Estimate,
  KeyStatus,
  Operation,
  Settings,
} from "./types";

export const core = {
  settingsGet: () => invoke<Settings>("settings_get"),
  settingsSet: (settings: Settings) => invoke<Settings>("settings_set", { settings }),
  keyStatus: () => invoke<KeyStatus>("key_status"),
  keySet: (key: string) => invoke<KeyStatus>("key_set", { key }),
  keyRemove: () => invoke<KeyStatus>("key_remove"),
  account: () => invoke<Account>("account"),
  catalog: () => invoke<Catalog>("catalog"),

  pickImages: () => invoke<string[]>("pick_images"),
  pickImage: () => invoke<string | null>("pick_image"),
  pickFolder: (title?: string) => invoke<string | null>("pick_folder", { title }),

  batchSnapshot: () => invoke<BatchSnapshot>("batch_snapshot"),
  batchAdd: (paths: string[]) => invoke<BatchSnapshot>("batch_add", { paths }),
  batchRemove: (id: number) => invoke<BatchSnapshot>("batch_remove", { id }),
  batchClear: () => invoke<BatchSnapshot>("batch_clear"),
  batchReset: () => invoke<BatchSnapshot>("batch_reset"),
  batchEstimate: (operation: Operation) => invoke<Estimate>("batch_estimate", { operation }),
  batchStart: (operation: Operation, outputDir: string | null) =>
    invoke<Estimate>("batch_start", { operation, outputDir }),
  batchRetry: () => invoke<Estimate>("batch_retry"),
  batchCancel: () => invoke<boolean>("batch_cancel"),
  batchReveal: (id: number, source = false) => invoke<void>("batch_reveal", { id, source }),

  editorOpen: (path: string) => invoke<EditorSnapshot>("editor_open", { path }),
  editorClose: () => invoke<void>("editor_close"),
  editorSnapshot: () => invoke<EditorSnapshot | null>("editor_snapshot"),
  editorApply: (tool: EditorTool) => invoke<EditorSnapshot>("editor_apply", { tool }),
  editorCheck: (marketplace: string | null, subject: boolean, text: boolean) =>
    invoke<CheckReport>("editor_check", { marketplace, subject, text }),
  editorCancel: () => invoke<void>("editor_cancel"),
  editorUndo: () => invoke<EditorSnapshot>("editor_undo"),
  editorRedo: () => invoke<EditorSnapshot>("editor_redo"),
  editorGoTo: (cursor: number) => invoke<EditorSnapshot>("editor_go_to", { cursor }),
  editorPickSave: () => invoke<string | null>("editor_pick_save"),
  editorSave: (path: string, replaceOriginal: boolean) =>
    invoke<string>("editor_save", { path, replaceOriginal }),
};

export type Core = typeof core;

/** The events the core sends while a batch runs. */
export const events = {
  batchItem: (handler: (item: BatchItem) => void): Promise<UnlistenFn> =>
    listen<BatchItem>("batch:item", (e) => handler(e.payload)),
  batchFinished: (handler: (summary: BatchSummary) => void): Promise<UnlistenFn> =>
    listen<BatchSummary>("batch:finished", (e) => handler(e.payload)),
  batchError: (handler: (error: AppError) => void): Promise<UnlistenFn> =>
    listen<AppError>("batch:error", (e) => handler(e.payload)),
};

/** Whatever was thrown, as an `AppError`. */
export function toAppError(thrown: unknown): AppError {
  if (typeof thrown === "object" && thrown !== null && "code" in thrown && "message" in thrown) {
    const e = thrown as Partial<AppError>;
    return {
      code: String(e.code),
      message: String(e.message),
      status: typeof e.status === "number" ? e.status : null,
      retry_after: typeof e.retry_after === "number" ? e.retry_after : null,
    };
  }
  const message = thrown instanceof Error ? thrown.message : String(thrown);
  return { code: "internal", message, status: null, retry_after: null };
}
