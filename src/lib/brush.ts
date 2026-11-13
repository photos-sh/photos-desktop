// The erase brush: turning pointer positions over the displayed photo into
// strokes in the photo's own pixels, which is what the core draws the mask from.
// The same positions give the points clicked for "select".

import type { Point, Stroke } from "./types";

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Size {
  width: number;
  height: number;
}

/** A point on screen as a point in the photo. `null` when the photo is not laid out. */
export function toPhoto(clientX: number, clientY: number, shown: Box, photo: Size): [number, number] | null {
  if (shown.width <= 0 || shown.height <= 0) return null;
  const x = ((clientX - shown.left) / shown.width) * photo.width;
  const y = ((clientY - shown.top) / shown.height) * photo.height;
  return [round(x), round(y)];
}

function round(n: number): number {
  return Math.round(n * 10) / 10;
}

/** The brush's radius in photo pixels: `percent` of the photo's longer side
 *  is its diameter. */
export function radiusFor(percent: number, photo: Size): number {
  return Math.max(1, round((Math.max(photo.width, photo.height) * percent) / 200));
}

/** Adds a point to a stroke, unless it has barely moved (keeps strokes small). */
export function extend(stroke: Stroke, point: [number, number]): Stroke {
  const last = stroke.points.at(-1);
  if (last) {
    const moved = Math.hypot(point[0] - last[0], point[1] - last[1]);
    if (moved < Math.max(1, stroke.radius / 4)) return stroke;
  }
  return { radius: stroke.radius, points: [...stroke.points, point] };
}

/** The pixel of the photo a click fell in. A click on the photo's very edge
 *  counts as its last pixel: the API refuses a point outside the photo. */
export function toPixel(point: Point, photo: Size): Point {
  const inside = (n: number, side: number) => Math.min(Math.max(Math.floor(n), 0), Math.max(side - 1, 0));
  return [inside(point[0], photo.width), inside(point[1], photo.height)];
}

/** Draws the points clicked on a canvas laid over the photo, each as a dot
 *  of `radius` canvas pixels. `scale` is canvas pixels per photo pixel. */
export function mark(
  context: CanvasRenderingContext2D,
  points: readonly Point[],
  scale: number,
  colour: string,
  radius: number,
) {
  context.clearRect(0, 0, context.canvas.width, context.canvas.height);
  for (const [x, y] of points) {
    // The middle of the pixel that was clicked.
    const at = [(x + 0.5) * scale, (y + 0.5) * scale] as const;
    // A dark rim, then a white one, so the dot shows on any photo.
    context.beginPath();
    context.arc(at[0], at[1], radius * 1.35, 0, Math.PI * 2);
    context.fillStyle = "rgb(0 0 0 / 60%)";
    context.fill();
    context.beginPath();
    context.arc(at[0], at[1], radius * 1.2, 0, Math.PI * 2);
    context.fillStyle = "#fff";
    context.fill();
    context.beginPath();
    context.arc(at[0], at[1], radius * 0.8, 0, Math.PI * 2);
    context.fillStyle = colour;
    context.fill();
  }
}

/** Draws strokes on a canvas laid over the photo. `scale` is canvas pixels
 *  per photo pixel. */
export function paint(context: CanvasRenderingContext2D, strokes: readonly Stroke[], scale: number, colour: string) {
  context.clearRect(0, 0, context.canvas.width, context.canvas.height);
  context.strokeStyle = colour;
  context.fillStyle = colour;
  context.lineCap = "round";
  context.lineJoin = "round";
  for (const stroke of strokes) {
    const [first, ...rest] = stroke.points;
    if (!first) continue;
    if (rest.length === 0) {
      context.beginPath();
      context.arc(first[0] * scale, first[1] * scale, stroke.radius * scale, 0, Math.PI * 2);
      context.fill();
      continue;
    }
    context.lineWidth = stroke.radius * 2 * scale;
    context.beginPath();
    context.moveTo(first[0] * scale, first[1] * scale);
    for (const point of rest) context.lineTo(point[0] * scale, point[1] * scale);
    context.stroke();
  }
}
