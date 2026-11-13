import { describe, expect, it } from "vitest";

import { extend, radiusFor, toPhoto, toPixel } from "./brush";

describe("toPhoto", () => {
  const shown = { left: 100, top: 50, width: 400, height: 300 };
  const photo = { width: 2000, height: 1500 };

  it("maps the screen to the photo's pixels", () => {
    expect(toPhoto(100, 50, shown, photo)).toEqual([0, 0]);
    expect(toPhoto(300, 200, shown, photo)).toEqual([1000, 750]);
    expect(toPhoto(500, 350, shown, photo)).toEqual([2000, 1500]);
  });

  it("keeps points outside the picture (the core clips them)", () => {
    expect(toPhoto(90, 40, shown, photo)).toEqual([-50, -50]);
  });

  it("gives nothing before the picture is laid out", () => {
    expect(toPhoto(10, 10, { left: 0, top: 0, width: 0, height: 0 }, photo)).toBeNull();
  });
});

describe("toPixel", () => {
  const photo = { width: 640, height: 480 };

  it("gives the pixel a click fell in", () => {
    expect(toPixel([320.4, 240.9], photo)).toEqual([320, 240]);
    expect(toPixel([0, 0], photo)).toEqual([0, 0]);
  });

  it("keeps a click on the edge inside the photo", () => {
    // The right and bottom edges map to the photo's width and height,
    // which the API would refuse as outside.
    expect(toPixel([640, 480], photo)).toEqual([639, 479]);
    expect(toPixel([-3.2, 700], photo)).toEqual([0, 479]);
  });
});

describe("radiusFor", () => {
  it("is half the percentage of the longer side", () => {
    expect(radiusFor(4, { width: 2000, height: 1500 })).toBe(40);
    expect(radiusFor(10, { width: 300, height: 600 })).toBe(30);
  });

  it("never gets smaller than a pixel", () => {
    expect(radiusFor(1, { width: 20, height: 20 })).toBe(1);
  });
});

describe("extend", () => {
  it("adds points that moved, and skips those that barely did", () => {
    let stroke = { radius: 20, points: [[100, 100]] as [number, number][] };
    stroke = extend(stroke, [102, 101]);
    expect(stroke.points).toHaveLength(1);
    stroke = extend(stroke, [110, 100]);
    expect(stroke.points).toEqual([
      [100, 100],
      [110, 100],
    ]);
  });

  it("does not change the stroke it was given", () => {
    const stroke = { radius: 4, points: [[0, 0]] as [number, number][] };
    const longer = extend(stroke, [50, 50]);
    expect(stroke.points).toHaveLength(1);
    expect(longer.points).toHaveLength(2);
  });
});
