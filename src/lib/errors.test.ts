import { describe, expect, it } from "vitest";

import { explain } from "./errors";
import type { AppError } from "./types";

function error(code: string, message = "", extra: Partial<AppError> = {}): AppError {
  return { code, message, status: null, retry_after: null, ...extra };
}

describe("explain", () => {
  it("sends key problems to Settings", () => {
    expect(explain(error("no_key"))).toEqual({ text: "Add your API key in Settings first.", settings: true });
    const refused = explain(error("unauthorized", "Unknown API key", { status: 401 }));
    expect(refused.text).toMatch(/key was not accepted/);
    expect(refused.settings).toBe(true);
    // The server's wording is not what the user reads.
    expect(refused.text).not.toContain("Unknown API key");
  });

  it("says what to do about credits and suspension", () => {
    expect(explain(error("payment_required", "Payment Required")).text).toMatch(/out of credits.*console/i);
    expect(explain(error("account_suspended")).text).toMatch(/suspended/);
    const short = explain(error("insufficient_credits", "This batch costs 12 credits and the account has 10 left."));
    expect(short.text).toBe("This batch costs 12 credits and the account has 10 left.");
  });

  it("keeps the server's remark where it helps", () => {
    expect(explain(error("bad_request", "width must be at least 1")).text).toBe(
      "The server did not accept this photo or these options. The server said: “width must be at least 1”",
    );
    expect(explain(error("unprocessable", "no subject found")).text).toContain("“no subject found”");
    expect(explain(error("bad_request", "  ")).text).toBe("The server did not accept this photo or these options.");
  });

  it("tells a busy server from a missing tool", () => {
    expect(explain(error("busy", "", { retry_after: 5 })).text).toBe(
      "The service is busy right now. Try again in 5 seconds.",
    );
    expect(explain(error("busy", "", { retry_after: 1 })).text).toContain("in 1 second.");
    expect(explain(error("busy")).text).toContain("in a moment");
    expect(explain(error("model_unavailable")).text).toMatch(/not available on the server/);
  });

  it("covers the other API codes", () => {
    expect(explain(error("payload_too_large")).text).toMatch(/too large/);
    expect(explain(error("unsupported_format")).text).toMatch(/cannot read this file/);
    expect(explain(error("internal")).text).toMatch(/went wrong on the server/);
    expect(explain(error("unknown", "<html>", { status: 502 })).text).toContain("HTTP 502");
    expect(explain(error("not_found", "", { status: 404 })).settings).toBe(true);
  });

  it("covers the network", () => {
    expect(explain(error("connection", "could not reach the server: refused")).text).toMatch(/internet connection/);
    expect(explain(error("timeout")).text).toMatch(/too long/);
    expect(explain(error("cancelled")).text).toBe("Cancelled.");
  });

  it("falls back to the core's own message", () => {
    expect(explain(error("batch_running", "A batch is running.")).text).toBe("A batch is running.");
    expect(explain(error("whatever", "")).text).toBe("Something went wrong.");
  });

  it("explains a batch file's error, which has no status", () => {
    expect(explain({ code: "unsupported_format", message: "x" }).text).toMatch(/cannot read this file/);
  });
});
