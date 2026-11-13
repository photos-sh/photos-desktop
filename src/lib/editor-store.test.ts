import { describe, expect, it } from "vitest";

import { CANCELLED_TOOL, EditorStore, MAX_SELECT_POINTS } from "./editor.svelte";
import { appError, editorSnapshot, fakeCore } from "./testing";
import type { CheckReport, Operation } from "./types";

const UPSCALE: Operation = { kind: "upscale", scale: 2 };
const REPORT: CheckReport = { metrics: {}, findings: [] };

async function opened() {
  const core = fakeCore();
  core.editorOpen.mockResolvedValue(editorSnapshot(0));
  const store = new EditorStore(core);
  await store.open("/photos/shoe.png");
  return { core, store };
}

describe("EditorStore", () => {
  it("starts with whatever the core has open", async () => {
    const core = fakeCore();
    core.editorSnapshot.mockResolvedValue(editorSnapshot(2));
    const store = new EditorStore(core);
    expect(store.isOpen).toBe(false);
    await store.init();
    expect(store.snapshot?.cursor).toBe(2);
  });

  it("opens a photo, by path or through the dialog", async () => {
    const { core, store } = await opened();
    expect(core.editorOpen).toHaveBeenCalledWith("/photos/shoe.png");
    expect(store.snapshot?.name).toBe("shoe.png");
    expect(store.busy).toBeNull();

    core.pickImage.mockResolvedValueOnce(null).mockResolvedValueOnce("/photos/other.png");
    await store.pick();
    expect(core.editorOpen).toHaveBeenCalledTimes(1);
    core.editorOpen.mockResolvedValue(editorSnapshot(0, 0, 2));
    await store.pick();
    expect(core.editorOpen).toHaveBeenLastCalledWith("/photos/other.png");
  });

  it("says why a file did not open", async () => {
    const core = fakeCore();
    core.editorOpen.mockRejectedValue(appError("not_an_image", "That file is not a photo this app can open."));
    const store = new EditorStore(core);
    expect(await store.open("/notes.txt")).toBe(false);
    expect(store.error?.code).toBe("not_an_image");
    expect(store.isOpen).toBe(false);
  });

  it("applies a tool: busy while it runs, a step when it is back", async () => {
    const { core, store } = await opened();
    let finish: (snapshot: ReturnType<typeof editorSnapshot>) => void = () => {};
    core.editorApply.mockImplementation(() => new Promise((resolve) => (finish = resolve)));
    const applying = store.apply(UPSCALE, "Upscaling…");
    expect(store.busy).toBe("Upscaling…");
    // Nothing else starts while a tool runs.
    expect(await store.apply(UPSCALE, "Again")).toBe(false);
    await store.undo();
    expect(core.editorUndo).not.toHaveBeenCalled();

    finish(editorSnapshot(1));
    expect(await applying).toBe(true);
    expect(store.busy).toBeNull();
    expect(store.snapshot?.cursor).toBe(1);
    expect(core.editorApply).toHaveBeenCalledTimes(1);
    expect(core.editorApply).toHaveBeenCalledWith({ tool: "operation", operation: UPSCALE });
  });

  it("keeps the picture and shows the error when a tool fails", async () => {
    const { core, store } = await opened();
    core.editorApply.mockRejectedValue(appError("payment_required", "Out of credits"));
    expect(await store.apply(UPSCALE, "Upscaling…")).toBe(false);
    expect(store.error?.code).toBe("payment_required");
    expect(store.snapshot?.cursor).toBe(0);
    expect(store.busy).toBeNull();
  });

  it("cancels a running tool: no error, no new step, and a word about the cost", async () => {
    const { core, store } = await opened();
    let fail: (error: unknown) => void = () => {};
    core.editorApply.mockImplementation(() => new Promise((_, reject) => (fail = reject)));
    core.editorCancel.mockResolvedValue(undefined);
    const applying = store.apply(UPSCALE, "Upscaling…");
    await store.cancel();
    expect(core.editorCancel).toHaveBeenCalled();
    fail(appError("cancelled"));
    expect(await applying).toBe(false);
    expect(store.error).toBeNull();
    expect(store.busy).toBeNull();
    expect(store.snapshot?.cursor).toBe(0);
    expect(store.snapshot?.steps).toHaveLength(0);
    expect(store.notice).toBe(CANCELLED_TOOL);
    expect(CANCELLED_TOOL).toMatch(/unchanged.*normally not charged.*just finished/);
    // With nothing running, cancel asks the core nothing.
    await store.cancel();
    expect(core.editorCancel).toHaveBeenCalledTimes(1);
  });

  it("erases what was painted, then forgets the strokes", async () => {
    const { core, store } = await opened();
    store.addStroke({ radius: 10, points: [] });
    expect(store.strokes).toHaveLength(0);
    store.addStroke({ radius: 10, points: [[5, 5]] });
    store.addStroke({ radius: 10, points: [[9, 9], [20, 20]] });
    store.addStroke({ radius: 10, points: [[1, 1]] });
    store.undoStroke();
    expect(store.strokes).toHaveLength(2);

    core.editorApply.mockResolvedValue(editorSnapshot(1));
    await store.erase("png");
    expect(core.editorApply).toHaveBeenCalledWith({
      tool: "erase",
      strokes: [
        { radius: 10, points: [[5, 5]] },
        { radius: 10, points: [[9, 9], [20, 20]] },
      ],
      format: "png",
    });
    expect(store.strokes).toHaveLength(0);

    store.addStroke({ radius: 3, points: [[1, 1]] });
    store.clearStrokes();
    expect(store.strokes).toHaveLength(0);
  });

  it("keeps the strokes when erasing fails", async () => {
    const { core, store } = await opened();
    store.addStroke({ radius: 10, points: [[5, 5]] });
    core.editorApply.mockRejectedValue(appError("busy"));
    await store.erase();
    expect(store.strokes).toHaveLength(1);
    expect(core.editorApply).toHaveBeenCalledWith({ tool: "erase", strokes: [{ radius: 10, points: [[5, 5]] }] });
  });

  it("cuts out the object at the points clicked, then forgets the points", async () => {
    const { core, store } = await opened();
    store.addPoint([320, 240]);
    store.addPoint([100, 50]);
    store.addPoint([5, 5]);
    store.undoPoint();
    expect(store.points).toEqual([
      [320, 240],
      [100, 50],
    ]);

    core.editorApply.mockResolvedValue(editorSnapshot(1));
    expect(await store.select()).toBe(true);
    // The surest answer is the API's default: it is not sent.
    expect(core.editorApply).toHaveBeenCalledWith({
      tool: "select",
      points: [
        [320, 240],
        [100, 50],
      ],
    });
    expect(store.snapshot?.cursor).toBe(1);
    // The points were on the picture before; the cutout is another one.
    expect(store.points).toHaveLength(0);

    store.addPoint([10, 10]);
    core.editorApply.mockResolvedValue(editorSnapshot(2));
    await store.select("largest");
    expect(core.editorApply).toHaveBeenLastCalledWith({ tool: "select", points: [[10, 10]], pick: "largest" });
  });

  it("keeps the points when nothing is found there, and takes no more than the API does", async () => {
    const { core, store } = await opened();
    store.addPoint([7, 7]);
    core.editorApply.mockRejectedValue(appError("unprocessable", "nothing was found at those points"));
    expect(await store.select("smallest")).toBe(false);
    expect(store.error?.code).toBe("unprocessable");
    expect(store.points).toEqual([[7, 7]]);
    expect(store.snapshot?.cursor).toBe(0);

    for (let n = 0; n < MAX_SELECT_POINTS + 5; n += 1) store.addPoint([n, n]);
    expect(store.points).toHaveLength(MAX_SELECT_POINTS);
    store.clearPoints();
    expect(store.points).toHaveLength(0);

    // Moving to another step drops them too.
    store.addPoint([1, 1]);
    core.editorGoTo.mockResolvedValue(editorSnapshot(2, 1));
    await store.goTo(1);
    expect(store.points).toHaveLength(0);
  });

  it("checks without adding a step, and drops the report when the picture changes", async () => {
    const { core, store } = await opened();
    core.editorCheck.mockResolvedValue(REPORT);
    await store.check("amazon", false, false);
    expect(core.editorCheck).toHaveBeenCalledWith("amazon", false, false);
    expect(store.report).toEqual(REPORT);
    expect(store.snapshot?.cursor).toBe(0);

    core.editorApply.mockResolvedValue(editorSnapshot(1));
    await store.apply(UPSCALE, "Upscaling…");
    expect(store.report).toBeNull();
  });

  it("moves through the steps", async () => {
    const { core, store } = await opened();
    core.editorUndo.mockResolvedValue(editorSnapshot(2, 1));
    core.editorRedo.mockResolvedValue(editorSnapshot(2, 2));
    core.editorGoTo.mockResolvedValue(editorSnapshot(2, 0));
    await store.undo();
    expect(store.snapshot?.cursor).toBe(1);
    await store.redo();
    expect(store.snapshot?.cursor).toBe(2);
    await store.goTo(0);
    expect(core.editorGoTo).toHaveBeenCalledWith(0);
    expect(store.snapshot?.cursor).toBe(0);
  });

  it("saves where the dialog says", async () => {
    const { core, store } = await opened();
    core.editorPickSave.mockResolvedValueOnce(null);
    await store.save();
    expect(core.editorSave).not.toHaveBeenCalled();

    core.editorPickSave.mockResolvedValue("/out/shoe-edited");
    core.editorSave.mockResolvedValue("/out/shoe-edited.png");
    await store.save();
    expect(core.editorSave).toHaveBeenCalledWith("/out/shoe-edited", false);
    expect(store.notice).toBe("Saved as /out/shoe-edited.png");
  });

  it("replaces the original only after a second yes", async () => {
    const { core, store } = await opened();
    core.editorPickSave.mockResolvedValue("/photos/shoe.png");
    core.editorSave.mockRejectedValueOnce(appError("confirm_replace_original"));
    await store.save();
    expect(store.confirmReplace).toBe("/photos/shoe.png");
    expect(store.error).toBeNull();
    expect(store.notice).toBeNull();

    // No: nothing more is sent.
    await store.confirmSave(false);
    expect(store.confirmReplace).toBeNull();
    expect(core.editorSave).toHaveBeenCalledTimes(1);

    // Asked again, and yes this time.
    core.editorSave.mockRejectedValueOnce(appError("confirm_replace_original"));
    await store.save();
    core.editorSave.mockResolvedValue("/photos/shoe.png");
    await store.confirmSave(true);
    expect(core.editorSave).toHaveBeenLastCalledWith("/photos/shoe.png", true);
    expect(store.notice).toBe("Saved as /photos/shoe.png");
  });

  it("closes the photo", async () => {
    const { core, store } = await opened();
    core.editorClose.mockResolvedValue(undefined);
    store.addStroke({ radius: 3, points: [[1, 1]] });
    await store.close();
    expect(store.isOpen).toBe(false);
    expect(store.strokes).toHaveLength(0);
  });
});
