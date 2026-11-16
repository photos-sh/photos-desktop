import { describe, expect, it, vi } from "vitest";

import { appError, fakeCore } from "./testing";
import type { UpdateInfo, UpdateProgress } from "./types";
import { Updates } from "./updates.svelte";

const NEWER: UpdateInfo = { version: "1.2.0", current_version: "1.1.0", notes: "Faster." };

/** Progress events the test sends itself. */
function progressEvents() {
  let handler: ((progress: UpdateProgress) => void) | undefined;
  const stopped = vi.fn();
  return {
    subscribe: (h: (progress: UpdateProgress) => void) => {
      handler = h;
      return Promise.resolve(stopped);
    },
    send: (progress: UpdateProgress) => handler?.(progress),
    stopped,
  };
}

describe("Updates", () => {
  it("finds a newer version", async () => {
    const fake = fakeCore();
    fake.updateCheck.mockResolvedValue(NEWER);
    const updates = new Updates(fake);
    await updates.check(true);
    expect(updates.available).toEqual(NEWER);
    expect(updates.upToDate).toBe(false);
    expect(updates.checking).toBe(false);
  });

  it("says so when asked and there is nothing newer", async () => {
    const fake = fakeCore();
    fake.updateCheck.mockResolvedValue(null);
    const updates = new Updates(fake);
    await updates.check(true);
    expect(updates.upToDate).toBe(false);
    await updates.check();
    expect(updates.upToDate).toBe(true);
    expect(updates.available).toBeNull();
  });

  it("keeps quiet about a failed check at start, not about one asked for", async () => {
    const fake = fakeCore();
    fake.updateCheck.mockRejectedValue(appError("update_failed", "offline"));
    const updates = new Updates(fake);
    await updates.check(true);
    expect(updates.error).toBeNull();
    await updates.check();
    expect(updates.error?.code).toBe("update_failed");
  });

  it("shows the banner again when a later check finds a version", async () => {
    const fake = fakeCore();
    fake.updateCheck.mockResolvedValue(NEWER);
    const updates = new Updates(fake);
    await updates.check(true);
    updates.later();
    expect(updates.dismissed).toBe(true);
    await updates.check();
    expect(updates.dismissed).toBe(false);
  });

  it("installs with progress, and stops listening when it fails", async () => {
    const fake = fakeCore();
    fake.updateCheck.mockResolvedValue(NEWER);
    const events = progressEvents();
    const updates = new Updates(fake, events.subscribe);
    await updates.check(true);

    let fail: (reason: unknown) => void = () => {};
    fake.updateInstall.mockImplementation(() => new Promise((_, reject) => (fail = reject)));
    const installing = updates.install();
    await vi.waitFor(() => expect(fake.updateInstall).toHaveBeenCalled());
    expect(updates.installing).toBe(true);
    events.send({ downloaded: 250, total: 1000 });
    expect(updates.percent).toBe(25);
    events.send({ downloaded: 300, total: null });
    expect(updates.percent).toBeNull();

    fail(appError("batch_running", "A batch is running."));
    await installing;
    expect(updates.installing).toBe(false);
    expect(updates.error?.code).toBe("batch_running");
    expect(events.stopped).toHaveBeenCalled();
  });

  it("installs nothing without a version found", async () => {
    const fake = fakeCore();
    const updates = new Updates(fake);
    await updates.install();
    expect(fake.updateInstall).not.toHaveBeenCalled();
  });
});
