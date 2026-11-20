import { describe, expect, it } from "vitest";

import {
  MAX_MARK_TEXT,
  MODELS,
  OPERATIONS,
  POSITIONS,
  SHAPES,
  build,
  choice,
  givesPixels,
  newDraft,
  problem,
  usesOutput,
} from "./operations";

describe("build", () => {
  it("sends only what was set", () => {
    expect(build(newDraft("remove_background"))).toEqual({ kind: "remove_background" });
    expect(build(newDraft("remove_text"))).toEqual({ kind: "remove_text" });
    expect(build(newDraft("upscale"))).toEqual({ kind: "upscale", scale: 2 });
  });

  it("starts from the default output format", () => {
    expect(build(newDraft("resize", "webp"))).toEqual({ kind: "resize", format: "webp" });
  });

  it("builds remove background", () => {
    const draft = { ...newDraft(), background: "colour" as const, colour: "#FFEEDD", crop: true, padding: 24 };
    expect(build(draft)).toEqual({ kind: "remove_background", background: "#ffeedd", crop: true, padding: 24 });
    expect(build({ ...newDraft(), background: "white" })).toEqual({ kind: "remove_background", background: "white" });
    // A margin without cropping means nothing.
    expect(build({ ...newDraft(), padding: 24 })).toEqual({ kind: "remove_background" });
  });

  it("adds a shadow to a cutout only when one is chosen", () => {
    expect(build({ ...newDraft(), shadow: "contact" })).toEqual({ kind: "remove_background", shadow: "contact" });
    expect(build({ ...newDraft(), background: "white", shadow: "drop" })).toEqual({
      kind: "remove_background",
      background: "white",
      shadow: "drop",
    });
    expect(build({ ...newDraft(), shadow: "none" })).toEqual({ kind: "remove_background" });
  });

  it("asks for a studio shot only when it is chosen", () => {
    expect(newDraft().studio).toBe(false);
    expect(build({ ...newDraft(), studio: true })).toEqual({ kind: "remove_background", studio: true });
    expect(build({ ...newDraft(), background: "white", studio: true, shadow: "contact" })).toEqual({
      kind: "remove_background",
      background: "white",
      shadow: "contact",
      studio: true,
    });
    expect(build({ ...newDraft(), studio: false })).toEqual({ kind: "remove_background" });
    // Another tool does not take it.
    expect(build({ ...newDraft("upscale"), studio: true })).toEqual({ kind: "upscale", scale: 2 });
  });

  it("sends a model only when one other than the API's default is chosen", () => {
    expect(newDraft().model).toBe("atlas");
    expect(MODELS.map((m) => m.value)).toEqual(["atlas", "lumen", "muse"]);
    expect(MODELS.map((m) => m.label)).toEqual(["Atlas (default)", "Lumen", "Muse"]);
    expect(build({ ...newDraft(), model: "atlas" })).toEqual({ kind: "remove_background" });
    expect(build({ ...newDraft(), model: "lumen" })).toEqual({ kind: "remove_background", model: "lumen" });
    expect(build({ ...newDraft(), model: "muse", studio: true, background: "white" })).toEqual({
      kind: "remove_background",
      background: "white",
      model: "muse",
      studio: true,
    });
    // Another tool does not take it.
    expect(build({ ...newDraft("upscale"), model: "muse" })).toEqual({ kind: "upscale", scale: 2 });
  });

  it("builds enhance: nothing for automatic, else only the sliders that moved", () => {
    expect(build(newDraft("enhance"))).toEqual({ kind: "enhance" });
    // Sliders moved while "automatic" is chosen are not sent.
    expect(build({ ...newDraft("enhance"), brightness: 20, sharpen: 40 })).toEqual({ kind: "enhance" });
    expect(
      build({ ...newDraft("enhance"), enhanceMode: "manual", brightness: 20, saturation: -100, sharpen: 40 }),
    ).toEqual({ kind: "enhance", brightness: 20, saturation: -100, sharpen: 40 });
    expect(
      build({ ...newDraft("enhance", "jpg"), enhanceMode: "manual", keepAuto: true, contrast: -15, quality: 90 }),
    ).toEqual({ kind: "enhance", auto: true, contrast: -15, format: "jpg", quality: 90 });
  });

  it("sends a strength only when the slider left the API's default", () => {
    expect(build(newDraft("denoise"))).toEqual({ kind: "denoise" });
    expect(build(newDraft("colorize"))).toEqual({ kind: "colorize" });
    expect(build(newDraft("restore_faces"))).toEqual({ kind: "restore_faces" });
    expect(build({ ...newDraft("denoise"), denoiseStrength: 60 })).toEqual({ kind: "denoise", strength: 60 });
    expect(build({ ...newDraft("colorize", "webp"), colorizeStrength: 75 })).toEqual({
      kind: "colorize",
      strength: 75,
      format: "webp",
    });
    expect(build({ ...newDraft("restore_faces"), restoreStrength: 50 })).toEqual({ kind: "restore_faces", strength: 50 });
    // Each tool keeps its own strength.
    expect(build({ ...newDraft("colorize"), denoiseStrength: 60 })).toEqual({ kind: "colorize" });
  });

  it("builds hide faces", () => {
    expect(build(newDraft("blur_faces"))).toEqual({ kind: "blur_faces" });
    expect(build({ ...newDraft("blur_faces"), faceMode: "pixelate", faceStrength: 80 })).toEqual({
      kind: "blur_faces",
      mode: "pixelate",
      strength: 80,
    });
    // A fill has no strength.
    expect(build({ ...newDraft("blur_faces"), faceMode: "fill", faceStrength: 80 })).toEqual({
      kind: "blur_faces",
      mode: "fill",
    });
  });

  it("builds crop, with the room as a fraction", () => {
    expect(build(newDraft("crop"))).toEqual({ kind: "crop", aspect: "1:1" });
    expect(
      build({ ...newDraft("crop"), cropShape: "4:5", cropFocus: "face", cropClose: true, cropRoom: 60 }),
    ).toEqual({ kind: "crop", aspect: "4:5", focus: "face", zoom: true, padding: 0.6 });
    expect(build({ ...newDraft("crop"), cropShape: "16:9", cropClose: true })).toEqual({
      kind: "crop",
      aspect: "16:9",
      zoom: true,
    });
    // Room without cropping closely means nothing.
    expect(build({ ...newDraft("crop"), cropRoom: 60 })).toEqual({ kind: "crop", aspect: "1:1" });
  });

  it("builds expand: inpainting is the API's own, white is said", () => {
    expect(build({ ...newDraft("expand"), expandShape: "16:9" })).toEqual({ kind: "expand", aspect: "16:9" });
    expect(build({ ...newDraft("expand", "jpg"), expandFill: "white" })).toEqual({
      kind: "expand",
      aspect: "1:1",
      fill: "white",
      format: "jpg",
    });
  });

  it("builds a text watermark", () => {
    expect(build({ ...newDraft("watermark"), markText: "  © Shop  " })).toEqual({ kind: "watermark", text: "© Shop" });
    expect(
      build({
        ...newDraft("watermark"),
        markText: "shop.example",
        markColour: "black",
        markPosition: "top-left",
        markSize: 40,
        markOpacity: 35,
      }),
    ).toEqual({
      kind: "watermark",
      text: "shop.example",
      color: "black",
      position: "top-left",
      scale: 40,
      opacity: 35,
    });
  });

  it("builds vectorize, which has no output format", () => {
    expect(build(newDraft("vectorize", "webp"))).toEqual({ kind: "vectorize" });
    expect(build({ ...newDraft("vectorize"), vectorMode: "mono", curves: "pixel", quality: 80 })).toEqual({
      kind: "vectorize",
      mode: "mono",
      curves: "pixel",
    });
  });

  it("builds resize, with the size limit in bytes", () => {
    const draft = {
      ...newDraft("resize"),
      width: 800,
      height: 600,
      fit: "cover" as const,
      enlarge: true,
      format: "jpg" as const,
      quality: 82,
      maxKb: 300,
      flatten: true,
      flattenColour: "#FFFFFF",
    };
    expect(build(draft)).toEqual({
      kind: "resize",
      width: 800,
      height: 600,
      fit: "cover",
      enlarge: true,
      format: "jpg",
      quality: 82,
      max_bytes: 307200,
      background: "#ffffff",
    });
  });

  it("leaves quality out for PNG", () => {
    expect(build({ ...newDraft("upscale"), format: "png", quality: 80, scale: 4 })).toEqual({
      kind: "upscale",
      scale: 4,
      format: "png",
    });
  });

  it("builds the rest", () => {
    expect(build({ ...newDraft("remove_text"), region: "all", matching: " sale|€ " })).toEqual({
      kind: "remove_text",
      region: "all",
      matching: "sale|€",
    });
    expect(build({ ...newDraft("fix"), marketplace: "amazon" })).toEqual({ kind: "fix", marketplace: "amazon" });
    expect(build({ ...newDraft("run"), preset: "studio" })).toEqual({ kind: "run", preset: "studio" });
  });
});

describe("problem", () => {
  it("passes the defaults that need no choice", () => {
    expect(problem(newDraft("remove_background"))).toBeNull();
    expect(problem(newDraft("upscale"))).toBeNull();
    expect(problem(newDraft("remove_text"))).toBeNull();
  });

  it("asks for a marketplace and a preset", () => {
    expect(problem(newDraft("fix"))).toBe("Choose a marketplace.");
    expect(problem(newDraft("run"))).toBe("Choose a preset.");
    expect(problem({ ...newDraft("fix"), marketplace: "etsy" })).toBeNull();
  });

  it("checks resize", () => {
    expect(problem(newDraft("resize"))).toMatch(/Set a size/);
    expect(problem({ ...newDraft("resize"), width: 800 })).toBeNull();
    expect(problem({ ...newDraft("resize"), format: "webp" })).toBeNull();
    expect(problem({ ...newDraft("resize"), width: 0 })).toMatch(/whole numbers/);
    expect(problem({ ...newDraft("resize"), width: 10.5 })).toMatch(/whole numbers/);
    expect(problem({ ...newDraft("resize"), width: 800, fit: "cover" })).toMatch(/both a width and a height/);
    expect(problem({ ...newDraft("resize"), maxKb: 0 })).toMatch(/more than 0 KB/);
  });

  it("checks quality, scale, margin and the pattern", () => {
    expect(problem({ ...newDraft("upscale"), quality: 101 })).toMatch(/1 to 100/);
    expect(problem({ ...newDraft("upscale"), quality: 0 })).toMatch(/1 to 100/);
    // PNG ignores quality, so a stray value does not block.
    expect(problem({ ...newDraft("upscale"), format: "png", quality: 101 })).toBeNull();
    expect(problem({ ...newDraft("upscale"), scale: 5 })).toMatch(/1 to 4/);
    expect(problem({ ...newDraft(), crop: true, padding: -1 })).toMatch(/margin/);
    expect(problem({ ...newDraft("remove_text"), matching: "(" })).toMatch(/regular expression/);
    expect(problem({ ...newDraft(), background: "colour", colour: "red" })).toMatch(/colour/);
  });

  it("passes the new tools as they start, except a watermark without text", () => {
    for (const kind of ["enhance", "denoise", "colorize", "restore_faces", "blur_faces", "crop", "expand", "vectorize"] as const) {
      expect(problem(newDraft(kind)), kind).toBeNull();
    }
    expect(problem(newDraft("watermark"))).toBe("Type the watermark's text.");
  });

  it("checks enhance: own settings need a setting", () => {
    const manual = { ...newDraft("enhance"), enhanceMode: "manual" as const };
    expect(problem(manual)).toBe("Move a slider, or choose Automatic.");
    expect(problem({ ...manual, sharpen: 10 })).toBeNull();
    expect(problem({ ...manual, keepAuto: true })).toBeNull();
    expect(problem({ ...manual, brightness: 120 })).toMatch(/-100 to 100/);
    expect(problem({ ...manual, sharpen: -5 })).toMatch(/0 to 100/);
    // Out of range sliders do not matter while "automatic" is chosen.
    expect(problem({ ...newDraft("enhance"), brightness: 120 })).toBeNull();
  });

  it("checks the strengths", () => {
    expect(problem({ ...newDraft("denoise"), denoiseStrength: 0 })).toMatch(/1 to 100/);
    expect(problem({ ...newDraft("colorize"), colorizeStrength: 101 })).toMatch(/1 to 100/);
    expect(problem({ ...newDraft("restore_faces"), restoreStrength: Number.NaN })).toMatch(/1 to 100/);
    expect(problem({ ...newDraft("blur_faces"), faceStrength: 0 })).toMatch(/1 to 100/);
    // A fill does not use it.
    expect(problem({ ...newDraft("blur_faces"), faceMode: "fill", faceStrength: 0 })).toBeNull();
  });

  it("checks crop, expand and the watermark", () => {
    expect(problem({ ...newDraft("crop"), cropShape: "" })).toBe("Choose a shape.");
    expect(problem({ ...newDraft("crop"), cropShape: "4:0" })).toBe("Choose a shape.");
    expect(problem({ ...newDraft("crop"), cropClose: true, cropRoom: 250 })).toMatch(/0 to 200 percent/);
    expect(problem({ ...newDraft("crop"), cropClose: true, cropRoom: 0 })).toBeNull();
    // The room is only looked at when cropping closely.
    expect(problem({ ...newDraft("crop"), cropRoom: 250 })).toBeNull();
    expect(problem({ ...newDraft("expand"), expandShape: "wide" })).toBe("Choose a shape.");

    const mark = { ...newDraft("watermark"), markText: "© Shop" };
    expect(problem(mark)).toBeNull();
    expect(problem({ ...mark, markText: "   " })).toMatch(/text/);
    // 200 characters, counted as the API counts them.
    expect(problem({ ...mark, markText: "😀".repeat(MAX_MARK_TEXT) })).toBeNull();
    expect(problem({ ...mark, markText: "😀".repeat(MAX_MARK_TEXT + 1) })).toMatch(/200 characters/);
    expect(problem({ ...mark, markSize: 0 })).toMatch(/size/);
    expect(problem({ ...mark, markOpacity: 0 })).toMatch(/opacity/);
    expect(problem({ ...mark, quality: 500 })).toMatch(/Quality/);
    // Vectorize has no quality to get wrong.
    expect(problem({ ...newDraft("vectorize"), quality: 500 })).toBeNull();
  });
});

describe("the list of operations", () => {
  it("has what a batch can run, with the API's price names", () => {
    expect(OPERATIONS.map((o) => o.priceKey)).toEqual([
      "remove-background",
      "upscale",
      "remove-text",
      "enhance",
      "denoise",
      "colorize",
      "restore-faces",
      "blur-faces",
      "crop",
      "expand",
      "resize",
      "watermark",
      "vectorize",
      "fix",
      "run",
      "run",
    ]);
    // Each kind once, each buildable from a fresh draft.
    expect(new Set(OPERATIONS.map((o) => o.kind)).size).toBe(OPERATIONS.length);
    for (const operation of OPERATIONS) {
      expect(build(newDraft(operation.kind)).kind).toBe(operation.kind);
    }
    expect(choice("fix").label).toBe("Fix for a marketplace");
    expect(choice("colorize").label).toBe("Colourise");
    expect(usesOutput("resize")).toBe(true);
    expect(usesOutput("crop")).toBe(true);
    expect(usesOutput("fix")).toBe(false);
    expect(usesOutput("run")).toBe(false);
    expect(usesOutput("vectorize")).toBe(false);
  });

  it("leaves out the tools that answer with JSON, and identity photos", () => {
    const kinds: string[] = OPERATIONS.map((o) => o.kind);
    for (const unwanted of ["caption", "palette", "faces", "id_photo"]) {
      expect(kinds).not.toContain(unwanted);
    }
  });

  it("warns, in a sentence, where a tool can mislead", () => {
    expect(choice("colorize").warning).toMatch(/guesses.*invented/);
    expect(choice("restore_faces").warning).toMatch(/invented.*identity photos or evidence/);
    expect(choice("denoise").warning).toMatch(/no GPU.*12 megapixels/);
    for (const operation of OPERATIONS) {
      // One sentence: a single full stop, at the end.
      if (operation.warning) expect(operation.warning).toMatch(/^[^.]+\.$/);
    }
    expect(choice("resize").warning).toBeUndefined();
    expect(choice("blur_faces").warning).toBeUndefined();
  });

  it("knows only vectorize gives something other than pixels", () => {
    expect(OPERATIONS.filter((o) => !givesPixels(o.kind)).map((o) => o.kind)).toEqual(["vectorize"]);
  });

  it("offers shapes the API takes and all nine positions", () => {
    for (const shape of SHAPES) expect(shape.value).toMatch(/^[1-9]\d*:[1-9]\d*$/);
    expect(SHAPES.map((s) => s.value)).toEqual(expect.arrayContaining(["1:1", "4:5", "16:9"]));
    expect(POSITIONS.map((p) => p.value)).toEqual([
      "top-left",
      "top",
      "top-right",
      "left",
      "center",
      "right",
      "bottom-left",
      "bottom",
      "bottom-right",
    ]);
  });
});

describe("recipe", () => {
  it("starts as a cutout on white, and sends only the steps ticked", () => {
    expect(problem(newDraft("recipe"))).toBeNull();
    expect(build(newDraft("recipe"))).toEqual({
      kind: "recipe",
      recipe: { remove_background: true, background: { kind: "color", color: "white" } },
    });
  });

  it("sends every step it is given", () => {
    const draft = {
      ...newDraft("recipe", "webp"),
      quality: 85,
      rBarcodes: true,
      rAllFormats: true,
      rText: true,
      rRemoveText: true,
      rTextRegion: "all" as const,
      rUpscale: true,
      rModel: "lumen" as const,
      rStudio: true,
      rLayout: "frame" as const,
      rBottom: true,
      rShadow: "contact" as const,
      rBackground: "gradient" as const,
      rColour: "#FFFFFF",
      rMaxSide: 1600,
      maxKb: 300,
    };
    expect(problem(draft)).toBeNull();
    expect(build(draft)).toEqual({
      kind: "recipe",
      recipe: {
        format: "webp",
        quality: 85,
        barcodes: true,
        all_formats: true,
        text: true,
        remove_text: "all",
        upscale_to: 1000,
        remove_background: true,
        model: "lumen",
        studio: true,
        layout: { kind: "frame", width: 2000, height: 2000, fill: 85, bottom: true },
        shadow: "contact",
        background: { kind: "gradient", from: "#ffffff", to: "#e6e9ed" },
        max_side: 1600,
        max_bytes: 300 * 1024,
      },
    });
  });

  it("leaves the model out at the default, and without remove background", () => {
    expect(build({ ...newDraft("recipe"), rModel: "atlas" })).toEqual(build(newDraft("recipe")));
    expect(build({ ...newDraft("recipe"), rModel: "muse" })).toEqual({
      kind: "recipe",
      recipe: { remove_background: true, model: "muse", background: { kind: "color", color: "white" } },
    });
    const readOnly = { ...newDraft("recipe"), rCutout: false, rBackground: "transparent" as const, rBarcodes: true };
    expect(build({ ...readOnly, rModel: "muse" })).toEqual({ kind: "recipe", recipe: { barcodes: true } });
  });

  it("can only read, and then sends nothing else", () => {
    const draft = { ...newDraft("recipe"), rCutout: false, rBackground: "transparent" as const, rBarcodes: true };
    expect(problem(draft)).toBeNull();
    expect(build(draft)).toEqual({ kind: "recipe", recipe: { barcodes: true } });
  });

  it("refuses what the service would refuse", () => {
    const base = newDraft("recipe");
    const refused: [Partial<typeof base>, string][] = [
      [{ rCutout: false, rBackground: "transparent" }, "at least one step"],
      [{ rCutout: false }, "need “Remove background”"],
      [{ rBackground: "transparent", format: "jpg" }, "JPEG has no transparency"],
      [{ rUpscale: true, rUpscaleTo: 8 }, "Upscale to"],
      [{ rLayout: "frame", rWidth: null }, "canvas"],
      [{ rLayout: "frame", rFill: 2 }, "fills between"],
      [{ rLayout: "crop", rPadding: 150 }, "room around"],
      [{ rBackground: "gradient", rGradientTo: "" }, "two colours"],
      [{ rMaxSide: 0 }, "longest side"],
    ];
    for (const [change, expected] of refused) {
      expect(problem({ ...base, ...change }), JSON.stringify(change)).toContain(expected);
    }
  });
});
