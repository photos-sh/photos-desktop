// Errors in plain language, chosen by the error's code.

import type { AppError, ItemError } from "./types";

export interface Explained {
  /** What happened and what to do, in a sentence or two. */
  text: string;
  /** Whether the Settings view is where to fix it. */
  settings: boolean;
}

function seconds(n: number): string {
  return n === 1 ? "1 second" : `${n} seconds`;
}

/** The server's own remark, when it adds something to ours. */
function withDetail(text: string, message: string): string {
  const detail = message.trim();
  return detail === "" ? text : `${text} The server said: “${detail}”`;
}

export function explain(error: AppError | ItemError): Explained {
  const status = "status" in error ? error.status : null;
  const retryAfter = "retry_after" in error ? error.retry_after : null;
  switch (error.code) {
    case "no_key":
      return { text: "Add your API key in Settings first.", settings: true };
    case "unauthorized":
      return {
        text: "The API key was not accepted. Check it in Settings, or create a new one in the Photos.sh console.",
        settings: true,
      };
    case "payment_required":
      return {
        text: "The account is out of credits. Add credits in the Photos.sh console, then try again.",
        settings: false,
      };
    case "account_suspended":
      return {
        text: "This account is suspended, so nothing can be processed. Contact Photos.sh support.",
        settings: false,
      };
    case "insufficient_credits":
      return { text: error.message, settings: false };
    case "bad_request":
      return {
        text: withDetail("The server did not accept this photo or these options.", error.message),
        settings: false,
      };
    case "payload_too_large":
      return {
        text: "This photo is too large for the service. Resize it first, or use a smaller file.",
        settings: false,
      };
    case "unsupported_format":
      return {
        text: "The service cannot read this file. It may be damaged, or in a format the server does not handle (HEIC and AVIF need server support).",
        settings: false,
      };
    case "unprocessable":
      return {
        text: withDetail("This photo could not be processed with these options.", error.message),
        settings: false,
      };
    case "busy":
      return {
        text:
          retryAfter != null && retryAfter > 0
            ? `The service is busy right now. Try again in ${seconds(retryAfter)}.`
            : "The service is busy right now. Try again in a moment.",
        settings: false,
      };
    case "model_unavailable":
      return {
        text: "This tool is not available on the server, so trying again will not help.",
        settings: false,
      };
    case "internal":
      return {
        text: "Something went wrong on the server. Try again; if it keeps happening, contact Photos.sh support.",
        settings: false,
      };
    case "not_found":
    case "unknown":
      return {
        text:
          status != null
            ? `The server answered in a way the app does not understand (HTTP ${status}). Check the base URL in Settings.`
            : "The server answered in a way the app does not understand. Check the base URL in Settings.",
        settings: true,
      };
    case "connection":
      return {
        text: "Could not reach the server. Check your internet connection, and the base URL in Settings if you changed it.",
        settings: false,
      };
    case "timeout":
      return {
        text: "The server took too long to answer. Try again; large photos take longer.",
        settings: false,
      };
    case "cancelled":
      return { text: "Cancelled.", settings: false };
    case "keychain":
      return {
        text: `The system keychain refused to store the key. ${error.message}`,
        settings: true,
      };
    default:
      // The core's own errors (bad options, files, folders) already read plainly.
      return {
        text: error.message.trim() === "" ? "Something went wrong." : error.message,
        settings: false,
      };
  }
}
