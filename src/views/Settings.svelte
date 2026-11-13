<script lang="ts">
  import ErrorNotice from "../components/ErrorNotice.svelte";
  import Notice from "../components/Notice.svelte";
  import { toAppError } from "../lib/ipc";
  import { FORMATS, OPERATIONS } from "../lib/operations";
  import { accountLine, type Session } from "../lib/session.svelte";
  import type { AppError, Format } from "../lib/types";

  interface Props {
    session: Session;
  }

  let { session }: Props = $props();

  const DEFAULT_BASE_URL = "https://api.photos.sh";
  /** Priced operations that are not in the batch's list. */
  const OTHER_PRICES: Record<string, string> = {
    erase: "Erase",
    select: "Select object",
    check: "Check",
    barcodes: "Read barcodes",
    ocr: "Read text",
    label: "Read a label",
    identify: "Catalogue matching",
  };

  // --- the key ---
  let pasted = $state("");
  let keyBusy = $state<"save" | "test" | "remove" | null>(null);
  let keyError = $state<AppError | null>(null);
  let tested = $state(false);

  async function saveKey(event: SubmitEvent) {
    event.preventDefault();
    if (pasted.trim() === "" || keyBusy !== null) return;
    keyBusy = "save";
    keyError = null;
    tested = false;
    try {
      await session.setKey(pasted);
      // The key now lives in the core only: forget it here.
      pasted = "";
      tested = true;
    } catch (e) {
      keyError = toAppError(e);
    } finally {
      keyBusy = null;
    }
  }

  async function testKey() {
    if (keyBusy !== null) return;
    keyBusy = "test";
    keyError = null;
    tested = false;
    await session.refreshAccount();
    tested = true;
    keyBusy = null;
  }

  async function removeKey() {
    if (keyBusy !== null) return;
    keyBusy = "remove";
    keyError = null;
    tested = false;
    try {
      await session.removeKey();
    } catch (e) {
      keyError = toAppError(e);
    } finally {
      keyBusy = null;
    }
  }

  // --- preferences ---
  let parallel = $state(3);
  let defaultFormat = $state<Format | "">("");
  let baseUrl = $state(DEFAULT_BASE_URL);
  let saving = $state(false);
  let saved = $state(false);
  let settingsError = $state<AppError | null>(null);

  // Show what is stored whenever it changes (at start, after a save).
  $effect(() => {
    const stored = session.settings;
    if (stored === null) return;
    parallel = stored.parallel;
    defaultFormat = stored.default_format ?? "";
    baseUrl = stored.base_url;
  });

  let changed = $derived(
    session.settings !== null &&
      (parallel !== session.settings.parallel ||
        defaultFormat !== (session.settings.default_format ?? "") ||
        baseUrl.trim() !== session.settings.base_url),
  );

  async function saveSettings(event: SubmitEvent) {
    event.preventDefault();
    if (saving) return;
    saving = true;
    saved = false;
    settingsError = null;
    try {
      await session.saveSettings({
        base_url: baseUrl.trim(),
        parallel,
        default_format: defaultFormat === "" ? null : defaultFormat,
      });
      saved = true;
    } catch (e) {
      settingsError = toAppError(e);
    } finally {
      saving = false;
    }
  }

  let prices = $derived(
    Object.entries(session.account?.prices ?? {})
      .map(([key, price]) => ({ label: OPERATIONS.find((o) => o.priceKey === key)?.label ?? OTHER_PRICES[key] ?? key, price }))
      .sort((a, b) => a.label.localeCompare(b.label)),
  );
</script>

<div class="scroll">
<section class="view" aria-labelledby="settings-title">
  <header>
    <h1 id="settings-title">Settings</h1>
  </header>

  <div class="card card--padded stack" aria-labelledby="key-title" role="group">
    <h2 id="key-title">API key</h2>

    {#if session.key?.set}
      <p>
        A key ending in <span class="mono">…{session.key.last4}</span> is
        {#if session.key.storage === "keychain"}
          stored in your system keychain.
        {:else}
          kept for this session only.
        {/if}
      </p>
    {:else}
      <p class="muted">
        No key yet. Create one in the Photos.sh console (your organisation's API keys page) and paste it here.
      </p>
    {/if}

    {#if session.key && !session.key.keychain_available}
      <Notice tone="warning">
        No system keychain could be used{session.key.keychain_error ? ` (${session.key.keychain_error})` : ""}. The
        app never writes the key to a file, so
        {session.key.set ? "this key is" : "a key you paste will be"} kept in memory only and has to be pasted again
        after you quit. On Linux, a Secret Service such as GNOME Keyring or KWallet provides the keychain.
      </Notice>
    {/if}

    <form class="row row--wrap" onsubmit={saveKey}>
      <div class="field grow">
        <label for="key-input">{session.key?.set ? "Replace it with another key" : "Paste your key"}</label>
        <input
          class="input mono"
          id="key-input"
          type="password"
          autocomplete="off"
          spellcheck="false"
          placeholder="Paste the key"
          bind:value={pasted}
          disabled={keyBusy !== null}
        />
      </div>
      <div class="row actions">
        <button class="button button--primary" type="submit" disabled={pasted.trim() === "" || keyBusy !== null}>
          {#if keyBusy === "save"}<span class="spinner"></span>{/if}
          Save and test
        </button>
        {#if session.key?.set}
          <button class="button" type="button" disabled={keyBusy !== null} onclick={testKey}>
            {#if keyBusy === "test"}<span class="spinner"></span>{/if}
            Test
          </button>
          <button class="button button--danger" type="button" disabled={keyBusy !== null} onclick={removeKey}>Remove</button>
        {/if}
      </div>
    </form>
    <p class="help">
      {session.key?.keychain_available === false
        ? "The key is never written to a file,"
        : "The key is kept in the system keychain, never in a file,"}
      and is only ever sent to the API. Once saved, it is not shown again: only its last characters.
    </p>

    <div aria-live="polite" class="stack">
      {#if keyError !== null}
        <ErrorNotice error={keyError} ondismiss={() => (keyError = null)} />
      {:else if session.key?.set && session.accountError !== null}
        <ErrorNotice error={session.accountError} />
      {:else if tested && session.account !== null}
        <Notice tone="success" ondismiss={() => (tested = false)}>The key works. {accountLine(session.account)}.</Notice>
      {/if}
    </div>
  </div>

  <div class="card card--padded stack" aria-labelledby="account-title" role="group">
    <div class="row">
      <h2 id="account-title" class="grow">Account</h2>
      <button
        class="button button--small"
        type="button"
        disabled={!session.hasKey || session.accountLoading}
        onclick={() => session.refreshAccount()}
      >
        {#if session.accountLoading}<span class="spinner"></span>{/if}
        Refresh
      </button>
    </div>
    {#if !session.hasKey}
      <p class="muted">Add a key to see the account and its credits.</p>
    {:else if session.account !== null}
      <p class="credits">
        <strong class="num">{accountLine(session.account)}</strong>
        {#if session.account.account}
          <span class="muted">· account <span class="mono selectable">{session.account.account}</span></span>
        {/if}
      </p>
      {#if session.account.unlimited}
        <p class="muted small">This key is not metered: no credits are counted for it.</p>
      {:else if prices.length > 0}
        <table class="prices small">
          <caption class="visually-hidden">Credits per photo, by operation</caption>
          <thead>
            <tr><th scope="col">Operation</th><th scope="col" class="num">Credits per photo</th></tr>
          </thead>
          <tbody>
            {#each prices as price (price.label)}
              <tr><td>{price.label}</td><td class="num">{price.price}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {:else if session.accountLoading}
      <p class="muted row"><span class="spinner"></span> Asking the server…</p>
    {:else if session.accountError !== null}
      <p class="muted">The account could not be read; see the message above.</p>
    {/if}
  </div>

  <form class="card card--padded stack" aria-labelledby="prefs-title" onsubmit={saveSettings}>
    <h2 id="prefs-title">Preferences</h2>
    <div class="grid-2">
      <div class="field">
        <label for="parallel">Files processed at once</label>
        <input class="input" id="parallel" type="number" min="1" max="8" step="1" required bind:value={parallel} aria-describedby="parallel-help" />
        <span class="help" id="parallel-help">1 to 8. More is faster until the service is busy; 3 suits most.</span>
      </div>
      <div class="field">
        <label for="default-format">Default output format</label>
        <select class="select" id="default-format" bind:value={defaultFormat} aria-describedby="format-help">
          <option value="">Each tool's own</option>
          {#each FORMATS as format (format.value)}
            <option value={format.value}>{format.label}</option>
          {/each}
        </select>
        <span class="help" id="format-help">What the format fields start with. It can be changed per operation.</span>
      </div>
    </div>

    <details>
      <summary>Advanced</summary>
      <div class="field advanced">
        <label for="base-url">API base URL</label>
        <div class="row">
          <input
            class="input mono grow"
            id="base-url"
            type="url"
            spellcheck="false"
            autocomplete="off"
            bind:value={baseUrl}
            aria-describedby="base-url-help"
          />
          <button class="button button--small" type="button" disabled={baseUrl === DEFAULT_BASE_URL} onclick={() => (baseUrl = DEFAULT_BASE_URL)}>
            Use the default
          </button>
        </div>
        <span class="help" id="base-url-help">
          Where the API is. Change it only for a private or test engine: your key and photos are sent there.
        </span>
      </div>
    </details>

    <div class="row">
      <button class="button button--primary" type="submit" disabled={!changed || saving}>
        {#if saving}<span class="spinner"></span>{/if}
        Save preferences
      </button>
      <div class="grow" aria-live="polite">
        {#if settingsError !== null}
          <ErrorNotice error={settingsError} ondismiss={() => (settingsError = null)} />
        {:else if saved && !changed}
          <span class="muted small">Saved.</span>
        {/if}
      </div>
    </div>
  </form>

  <p class="muted small about">
    Photos.sh desktop {__APP_VERSION__}. No telemetry: the app talks only to the API above. Photos are sent there to
    be processed; results are written where you choose.
  </p>
</section>
</div>

<style>
  .scroll {
    height: 100%;
    overflow-y: auto;
  }
  .view {
    display: flex;
    flex-direction: column;
    gap: var(--ps-space-4);
    max-width: 52rem;
    padding: var(--ps-space-5) var(--ps-space-6);
  }
  .actions {
    align-self: flex-end;
  }
  .credits {
    font-size: 1rem;
  }
  .prices {
    width: 100%;
    max-width: 24rem;
    border-collapse: collapse;
  }
  .prices th,
  .prices td {
    padding: 0.3rem 0;
    border-bottom: 1px solid var(--ps-border);
    text-align: left;
  }
  .prices th {
    color: var(--ps-text-3);
    font-weight: 600;
  }
  .prices .num {
    text-align: right;
  }
  summary {
    width: fit-content;
    color: var(--ps-text-2);
    font-size: 0.875rem;
    font-weight: 600;
    cursor: pointer;
  }
  .advanced {
    margin-top: var(--ps-space-3);
  }
  .about {
    padding-bottom: var(--ps-space-4);
  }
</style>
