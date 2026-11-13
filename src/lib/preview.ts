// Pictures come from the core's `preview` protocol, by key, not through IPC:
// `batch/<id>/source`, `batch/<id>/result`, `editor/<session>/<step>`.

import { convertFileSrc } from "@tauri-apps/api/core";

type Resolver = (key: string) => string;

let resolver: Resolver = (key) => convertFileSrc(key, "preview");

/** Replaces how keys become URLs (the browser mock serves its own pictures). */
export function setPreviewResolver(next: Resolver) {
  resolver = next;
}

export function previewUrl(key: string, version?: number): string {
  const url = resolver(key);
  return version == null || url.startsWith("data:") ? url : `${url}?v=${version}`;
}

export const preview = {
  batchSource: (id: number) => previewUrl(`batch/${id}/source`),
  batchResult: (id: number, version: number) => previewUrl(`batch/${id}/result`, version),
  editorStep: (session: number, step: number) => previewUrl(`editor/${session}/${step}`),
};
