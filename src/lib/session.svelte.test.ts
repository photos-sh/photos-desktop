import { describe, expect, it } from "vitest";

import { Session, accountLine } from "./session.svelte";
import { appError, fakeCore } from "./testing";
import type { Account, Catalog, KeyStatus, Settings } from "./types";

const SETTINGS: Settings = { base_url: "https://api.photos.sh", parallel: 3, default_format: null, check_for_updates: true };
const NO_KEY: KeyStatus = { set: false, last4: null, storage: "none", keychain_available: true, keychain_error: null };
const KEY: KeyStatus = { ...NO_KEY, set: true, last4: "cdef", storage: "keychain" };
const ACCOUNT: Account = { account: "acc_1", credits: 42, prices: { resize: 1 }, unlimited: false };
const CATALOG: Catalog = { presets: [{ name: "web", description: "" }], marketplaces: [{ name: "amazon", title: "Amazon" }] };

function core(key = KEY) {
  const fake = fakeCore();
  fake.settingsGet.mockResolvedValue(SETTINGS);
  fake.keyStatus.mockResolvedValue(key);
  fake.account.mockResolvedValue(ACCOUNT);
  fake.catalog.mockResolvedValue(CATALOG);
  return fake;
}

describe("Session", () => {
  it("without a key asks the API nothing", async () => {
    const fake = core(NO_KEY);
    const session = new Session(fake);
    await session.init();
    expect(session.hasKey).toBe(false);
    expect(session.settings).toEqual(SETTINGS);
    expect(fake.account).not.toHaveBeenCalled();
    expect(fake.catalog).not.toHaveBeenCalled();
  });

  it("with a key reads the account and the lists", async () => {
    const session = new Session(core());
    await session.init();
    expect(session.account).toEqual(ACCOUNT);
    expect(session.catalog).toEqual(CATALOG);
    expect(session.accountLoading).toBe(false);
  });

  it("keeps the error when the key is refused", async () => {
    const fake = core();
    fake.account.mockRejectedValue(appError("unauthorized"));
    fake.catalog.mockRejectedValue(appError("unauthorized"));
    const session = new Session(fake);
    await session.init();
    expect(session.account).toBeNull();
    expect(session.accountError?.code).toBe("unauthorized");
    expect(session.catalogError?.code).toBe("unauthorized");
  });

  it("saving a key tries it, and never keeps it", async () => {
    const fake = core(NO_KEY);
    fake.keySet.mockResolvedValue(KEY);
    const session = new Session(fake);
    await session.init();
    await session.setKey("ps_live_0123456789abcdef");
    expect(fake.keySet).toHaveBeenCalledWith("ps_live_0123456789abcdef");
    expect(session.key?.last4).toBe("cdef");
    expect(session.account).toEqual(ACCOUNT);
    expect(JSON.stringify($state.snapshot(session))).not.toContain("0123456789");
  });

  it("removing the key forgets the account", async () => {
    const fake = core();
    fake.keyRemove.mockResolvedValue(NO_KEY);
    const session = new Session(fake);
    await session.init();
    await session.removeKey();
    expect(session.hasKey).toBe(false);
    expect(session.account).toBeNull();
    expect(session.catalog).toBeNull();
  });

  it("a new base URL means asking the account again", async () => {
    const fake = core();
    const session = new Session(fake);
    await session.init();
    fake.settingsSet.mockImplementation((settings) => Promise.resolve(settings));

    await session.saveSettings({ ...SETTINGS, parallel: 5 });
    expect(session.settings?.parallel).toBe(5);
    expect(fake.account).toHaveBeenCalledTimes(1);

    await session.saveSettings({ ...SETTINGS, base_url: "http://127.0.0.1:8098/api" });
    expect(fake.account).toHaveBeenCalledTimes(2);

    fake.settingsSet.mockRejectedValue(appError("invalid", "the base URL must start with http:// or https://"));
    await expect(session.saveSettings({ ...SETTINGS, base_url: "nope" })).rejects.toMatchObject({ code: "invalid" });
    expect(session.settings?.base_url).toBe("http://127.0.0.1:8098/api");
  });

  it("reports a start that failed", async () => {
    const fake = fakeCore();
    fake.settingsGet.mockRejectedValue(appError("io", "could not read the settings"));
    fake.keyStatus.mockResolvedValue(NO_KEY);
    const session = new Session(fake);
    await session.init();
    expect(session.startError?.code).toBe("io");
  });
});

describe("accountLine", () => {
  it("shows credits, or that nothing is counted", () => {
    expect(accountLine(ACCOUNT)).toBe("42 credits left");
    expect(accountLine({ ...ACCOUNT, credits: 1 })).toBe("1 credit left");
    expect(accountLine({ prices: {}, unlimited: true, account: "sdk" })).toBe("Unlimited key");
    expect(accountLine({ prices: {}, unlimited: false })).toBe("Connected");
  });
});
