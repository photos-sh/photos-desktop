import { describe, expect, it } from "vitest";

import { dimensions, fileSize, plural, splitPath, typeName } from "./format";

describe("format", () => {
  it("counts things", () => {
    expect(plural(1, "file", "files")).toBe("1 file");
    expect(plural(0, "file", "files")).toBe("0 files");
    expect(plural(1200, "credit", "credits")).toBe("1,200 credits");
  });

  it("writes file sizes", () => {
    expect(fileSize(null)).toBe("");
    expect(fileSize(512)).toBe("512 B");
    expect(fileSize(2048)).toBe("2.0 KB");
    expect(fileSize(184_320)).toBe("180 KB");
    expect(fileSize(3 * 1024 * 1024)).toBe("3.0 MB");
  });

  it("writes sizes and types", () => {
    expect(dimensions(800, 600)).toBe("800 × 600");
    expect(dimensions(null, 600)).toBe("");
    expect(typeName("image/jpeg")).toBe("JPEG");
    expect(typeName("image/webp")).toBe("WEBP");
  });

  it("splits paths with either slash", () => {
    expect(splitPath("/home/you/a.jpg")).toEqual({ folder: "/home/you", name: "a.jpg" });
    expect(splitPath("C:\\Photos\\a.jpg")).toEqual({ folder: "C:\\Photos", name: "a.jpg" });
    expect(splitPath("a.jpg")).toEqual({ folder: "", name: "a.jpg" });
  });
});
