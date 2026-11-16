// Updates: whether a newer version is out, and installing it when asked.
// The core does the work (see src-tauri/src/updates.rs); this keeps what the
// banner and the Settings view show.

import type { UnlistenFn } from "@tauri-apps/api/event";

import { core as realCore, onUpdateProgress, toAppError, type Core } from "./ipc";
import type { AppError, UpdateInfo, UpdateProgress } from "./types";

type Subscribe = (handler: (progress: UpdateProgress) => void) => Promise<UnlistenFn>;

export class Updates {
  /** The newer version the last check found. */
  available = $state<UpdateInfo | null>(null);
  checking = $state(false);
  installing = $state(false);
  progress = $state<UpdateProgress | null>(null);
  error = $state<AppError | null>(null);
  /** A check the user asked for found nothing newer. */
  upToDate = $state(false);
  /** The banner was put away until the next check. */
  dismissed = $state(false);

  readonly #core: Core;
  readonly #subscribe: Subscribe;

  constructor(core: Core = realCore, subscribe: Subscribe = onUpdateProgress) {
    this.#core = core;
    this.#subscribe = subscribe;
  }

  /**
   * Asks whether there is a newer version. `quiet` is the check at start:
   * nobody asked for it, so a failure (offline, say) is not shown.
   */
  async check(quiet = false): Promise<void> {
    if (this.checking || this.installing) return;
    this.checking = true;
    this.upToDate = false;
    if (!quiet) this.error = null;
    try {
      this.available = await this.#core.updateCheck();
      this.upToDate = !quiet && this.available === null;
      if (this.available !== null) this.dismissed = false;
    } catch (e) {
      if (!quiet) this.error = toAppError(e);
    } finally {
      this.checking = false;
    }
  }

  /** Downloads and installs it. The app restarts when it is done, so this only returns when it failed. */
  async install(): Promise<void> {
    if (this.available === null || this.installing) return;
    this.installing = true;
    this.error = null;
    this.progress = null;
    const stop = await this.#subscribe((progress) => (this.progress = progress));
    try {
      await this.#core.updateInstall();
    } catch (e) {
      this.error = toAppError(e);
    } finally {
      stop();
      this.installing = false;
    }
  }

  later(): void {
    this.dismissed = true;
    this.error = null;
  }

  /** The release page, for an update the app could not install itself. */
  async openReleases(): Promise<void> {
    try {
      await this.#core.updateOpenReleases();
    } catch (e) {
      this.error = toAppError(e);
    }
  }

  /** How much is downloaded, 0 to 100, when the size is known. */
  get percent(): number | null {
    const p = this.progress;
    if (p === null || p.total === null || p.total <= 0) return null;
    return Math.min(100, Math.round((p.downloaded / p.total) * 100));
  }
}
