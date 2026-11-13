// What every view shares: the settings, whether a key is set, the account
// behind it, and the lists (presets, marketplaces) the forms offer.

import { core as realCore, toAppError, type Core } from "./ipc";
import type { Account, AppError, Catalog, KeyStatus, Settings } from "./types";

export class Session {
  settings = $state<Settings | null>(null);
  key = $state<KeyStatus | null>(null);
  account = $state<Account | null>(null);
  accountError = $state<AppError | null>(null);
  accountLoading = $state(false);
  catalog = $state<Catalog | null>(null);
  catalogError = $state<AppError | null>(null);
  /** Set when the app could not read its own state at start. */
  startError = $state<AppError | null>(null);

  readonly #core: Core;
  #accountRequest = 0;

  constructor(core: Core = realCore) {
    this.#core = core;
  }

  get hasKey(): boolean {
    return this.key?.set === true;
  }

  async init(): Promise<void> {
    try {
      [this.settings, this.key] = await Promise.all([this.#core.settingsGet(), this.#core.keyStatus()]);
    } catch (e) {
      this.startError = toAppError(e);
      return;
    }
    await this.refresh();
  }

  /** Reads the account and the lists again (after the key or the URL changed). */
  async refresh(): Promise<void> {
    if (!this.hasKey) {
      this.account = null;
      this.accountError = null;
      this.catalog = null;
      this.catalogError = null;
      return;
    }
    await Promise.all([this.refreshAccount(), this.loadCatalog()]);
  }

  async refreshAccount(): Promise<void> {
    if (!this.hasKey) return;
    const request = ++this.#accountRequest;
    this.accountLoading = true;
    try {
      const account = await this.#core.account();
      if (request !== this.#accountRequest) return;
      this.account = account;
      this.accountError = null;
    } catch (e) {
      if (request !== this.#accountRequest) return;
      this.account = null;
      this.accountError = toAppError(e);
    } finally {
      if (request === this.#accountRequest) this.accountLoading = false;
    }
  }

  async loadCatalog(): Promise<void> {
    if (!this.hasKey) return;
    try {
      this.catalog = await this.#core.catalog();
      this.catalogError = null;
    } catch (e) {
      this.catalog = null;
      this.catalogError = toAppError(e);
    }
  }

  /** Stores the key, then tries it. Throws when the key cannot be stored. */
  async setKey(key: string): Promise<void> {
    this.key = await this.#core.keySet(key);
    await this.refresh();
  }

  async removeKey(): Promise<void> {
    this.key = await this.#core.keyRemove();
    await this.refresh();
  }

  /** Saves the settings. Throws when a value does not hold. */
  async saveSettings(settings: Settings): Promise<void> {
    const before = this.settings?.base_url;
    this.settings = await this.#core.settingsSet(settings);
    if (this.settings.base_url !== before) await this.refresh();
  }
}

/** The account in a line: credits left, or that the key is unlimited. */
export function accountLine(account: Account): string {
  if (account.unlimited) return "Unlimited key";
  if (account.credits == null) return "Connected";
  return `${account.credits.toLocaleString("en")} ${account.credits === 1 ? "credit" : "credits"} left`;
}
