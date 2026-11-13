// Small text helpers.

export function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
}

export function fileSize(bytes: number | null | undefined): string {
  if (bytes == null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function dimensions(width: number | null | undefined, height: number | null | undefined) {
  return width != null && height != null ? `${width} × ${height}` : "";
}

/** `image/png` → `PNG`. */
export function typeName(contentType: string): string {
  const sub = contentType.split("/")[1]?.split(";")[0]?.trim() ?? "";
  return sub === "jpeg" ? "JPEG" : sub.toUpperCase();
}

/** The folder part and the name of a path, with either kind of slash. */
export function splitPath(path: string): { folder: string; name: string } {
  const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return at < 0 ? { folder: "", name: path } : { folder: path.slice(0, at), name: path.slice(at + 1) };
}
