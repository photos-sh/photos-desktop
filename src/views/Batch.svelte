<script lang="ts">
  import { untrack } from "svelte";

  import ErrorNotice from "../components/ErrorNotice.svelte";
  import Notice from "../components/Notice.svelte";
  import OperationForm from "../components/OperationForm.svelte";
  import Thumb from "../components/Thumb.svelte";
  import {
    CANCELLED_ITEM,
    CANCEL_HINT,
    STATUS_LABEL,
    abortedNote,
    estimateText,
    retryLabel,
    summaryText,
  } from "../lib/batch";
  import type { BatchStore } from "../lib/batch.svelte";
  import { explain } from "../lib/errors";
  import { dimensions, fileSize, plural, splitPath } from "../lib/format";
  import { nav } from "../lib/nav.svelte";
  import { OPERATIONS, build, choice, newDraft, problem } from "../lib/operations";
  import { preview } from "../lib/preview";
  import type { Session } from "../lib/session.svelte";
  import type { BatchItem, ItemStatus } from "../lib/types";

  interface Props {
    batch: BatchStore;
    session: Session;
    /** Files are being dragged over the window. */
    dragging: boolean;
  }

  let { batch, session, dragging }: Props = $props();

  let draft = $state(newDraft("remove_background", untrack(() => session.settings?.default_format ?? null)));
  let issue = $derived(problem(draft));

  // A default format changed in Settings applies from then on.
  let defaultFormat = untrack(() => session.settings?.default_format ?? null);
  $effect(() => {
    const next = session.settings?.default_format ?? null;
    if (next !== defaultFormat) {
      defaultFormat = next;
      draft.format = next ?? "";
    }
  });
  let locked = $derived(batch.running || batch.starting);
  let empty = $derived(batch.items.length === 0);

  // What the batch will cost: asked again when the operation, the number of
  // files or the key changes (not on every keystroke in the options).
  $effect(() => {
    const ready = session.hasKey && issue === null && !batch.running;
    void draft.kind;
    void batch.pending;
    untrack(() => {
      void batch.refreshEstimate(ready ? build(draft) : null);
    });
  });

  // A finished run has spent credits: show the new balance.
  $effect(() => {
    if (batch.summary !== null) untrack(() => void session.refreshAccount());
  });

  let blocker = $derived.by(() => {
    if (empty) return "Add photos to start.";
    if (batch.pending === 0) return "Every file is done. Run again to process them once more, or add photos.";
    if (!session.hasKey) return "Add your API key in Settings to start.";
    if (issue !== null) return issue;
    if (batch.estimate !== null && !batch.estimate.enough) return "The account does not have enough credits for this batch.";
    return null;
  });

  const TONE: Record<ItemStatus, string> = {
    queued: "tone-neutral",
    running: "tone-info",
    done: "tone-success",
    failed: "tone-danger",
    skipped: "tone-warning",
    cancelled: "tone-warning",
  };

  function detail(item: BatchItem): string {
    if (item.status === "done") {
      return [item.output_name, dimensions(item.width, item.height), fileSize(item.bytes), item.note]
        .filter((part) => part)
        .join(" · ");
    }
    if (item.status === "failed" && item.error) return explain(item.error).text;
    if (item.status === "skipped") return "Not run: the batch was stopped first.";
    if (item.status === "cancelled") return CANCELLED_ITEM;
    return splitPath(item.source).folder;
  }

  function start() {
    if (blocker === null) void batch.start(build(draft));
  }
</script>

<section class="view" aria-labelledby="batch-title">
  <header class="view__header">
    <div class="grow">
      <h1 id="batch-title">Batch</h1>
      <p class="muted small">One operation on many photos. Originals are never changed.</p>
    </div>
    <button class="button" type="button" disabled={locked || batch.adding} onclick={() => batch.pickFiles()}>
      Add photos…
    </button>
    <button class="button" type="button" disabled={locked || batch.adding} onclick={() => batch.pickFolder()}>
      Add a folder…
    </button>
    <button class="button button--quiet" type="button" disabled={locked || empty} onclick={() => batch.clear()}>
      Clear list
    </button>
  </header>

  <div class="columns">
    <div class="list card" class:list--drop={dragging && !locked}>
      {#if empty}
        <div class="empty">
          <div class="empty__mark checker" aria-hidden="true"></div>
          <h2>{batch.adding ? "Reading…" : "Drop photos or folders here"}</h2>
          <p class="muted">
            JPEG, PNG, WebP, GIF, TIFF, BMP, HEIC and AVIF. Folders are searched for photos, subfolders
            included.
          </p>
          <div class="row">
            <button class="button button--primary" type="button" disabled={batch.adding} onclick={() => batch.pickFiles()}>
              Add photos…
            </button>
            <button class="button" type="button" disabled={batch.adding} onclick={() => batch.pickFolder()}>
              Add a folder…
            </button>
          </div>
        </div>
      {:else}
        <div class="list__head small muted">
          <span class="num">{plural(batch.counts.total, "photo", "photos")}</span>
          {#if batch.counts.done > 0}<span class="num">· {batch.counts.done} done</span>{/if}
          {#if batch.counts.failed > 0}<span class="num">· {batch.counts.failed} failed</span>{/if}
          {#if batch.counts.cancelled > 0}<span class="num">· {batch.counts.cancelled} cancelled</span>{/if}
          {#if batch.counts.skipped > 0}<span class="num">· {batch.counts.skipped} not run</span>{/if}
          {#if batch.adding}<span class="row"><span class="spinner"></span> Reading…</span>{/if}
        </div>
        <ul class="items" aria-label="Photos in the batch">
          {#each batch.items as item (item.id)}
            <li class="item">
              <Thumb
                src={item.status === "done" ? preview.batchResult(item.id, item.version) : preview.batchSource(item.id)}
              />
              <div class="item__text">
                <span class="item__name selectable" title={item.source}>{item.name}</span>
                <span
                  class="item__detail small"
                  class:item__detail--error={item.status === "failed"}
                  class:item__detail--wrap={item.status === "cancelled"}
                >
                  {detail(item)}
                </span>
              </div>
              <span class="pill {TONE[item.status]}">
                {#if item.status === "running"}<span class="spinner"></span>{/if}
                {STATUS_LABEL[item.status]}
              </span>
              <button
                class="button button--quiet button--small"
                type="button"
                title={item.output ? "Show the result in the file manager" : "Show the photo in the file manager"}
                onclick={() => batch.reveal(item)}
              >
                Show<span class="visually-hidden"> {item.output_name ?? item.name} in the file manager</span>
              </button>
              <button
                class="button button--quiet button--icon"
                type="button"
                disabled={locked}
                aria-label="Remove {item.name} from the list"
                title="Remove from the list"
                onclick={() => batch.remove(item.id)}
              >
                <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
                  <path d="M6 6l12 12M18 6L6 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                </svg>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <aside class="panel card" aria-label="Operation">
      <div class="panel__scroll stack">
        <div class="field">
          <label for="batch-kind">Operation</label>
          <select class="select" id="batch-kind" bind:value={draft.kind} disabled={locked}>
            {#each OPERATIONS as operation (operation.kind)}
              <option value={operation.kind}>{operation.label}</option>
            {/each}
          </select>
          <span class="help">{choice(draft.kind).hint}</span>
        </div>

        <OperationForm
          bind:draft
          catalog={session.catalog}
          catalogError={session.catalogError}
          disabled={locked}
          prefix="batch"
        />

        <div class="field">
          <span class="label">Save results in</span>
          <span class="path mono selectable" title={batch.outputDir ?? ""}>
            {batch.outputDir ?? "A “photos-sh” folder beside the first photo"}
          </span>
          <div class="row">
            <button class="button button--small" type="button" disabled={locked} onclick={() => batch.pickOutputDir()}>
              Choose a folder…
            </button>
            {#if batch.chosenOutputDir !== null}
              <button
                class="button button--quiet button--small"
                type="button"
                disabled={locked}
                onclick={() => (batch.chosenOutputDir = null)}
              >
                Use the default
              </button>
            {/if}
          </div>
          <span class="help">Names come from the photos. An existing file is never replaced: the new one gets a number.</span>
        </div>
      </div>

      <div class="panel__foot stack">
        <div class="cost small" aria-live="polite">
          {#if batch.estimate !== null}
            <span class:cost--short={!batch.estimate.enough}>{estimateText(batch.estimate)}</span>
          {:else if batch.estimating}
            <span class="muted row"><span class="spinner"></span> Working out the cost…</span>
          {:else if batch.estimateError !== null && !empty}
            <span class="cost--short">{explain(batch.estimateError).text}</span>
          {/if}
        </div>

        {#if batch.running}
          <button class="button button--danger button--wide" type="button" disabled={batch.cancelling} onclick={() => batch.cancel()}>
            {#if batch.cancelling}<span class="spinner"></span> Cancelling…{:else}Cancel{/if}
          </button>
          <p class="small muted">{CANCEL_HINT}</p>
        {:else}
          {#if blocker !== null && !batch.starting}
            <p class="small muted" id="batch-blocker">{blocker}</p>
          {/if}
          <button
            class="button button--primary button--wide"
            type="button"
            disabled={blocker !== null || batch.starting}
            aria-describedby={blocker !== null ? "batch-blocker" : undefined}
            onclick={start}
          >
            {#if batch.starting}
              <span class="spinner"></span> Checking…
            {:else}
              Start{batch.pending > 0 ? ` (${plural(batch.pending, "file", "files")})` : ""}
            {/if}
          </button>
          {#if batch.canRetry}
            <button class="button button--wide" type="button" disabled={batch.starting} onclick={() => batch.retry()}>
              {retryLabel(batch.counts)}
            </button>
          {/if}
          {#if batch.counts.done > 0}
            <button class="button button--quiet button--wide" type="button" disabled={batch.starting} onclick={() => batch.reset()}>
              Run again on all photos
            </button>
          {/if}
        {/if}
      </div>
    </aside>
  </div>

  <footer class="status" aria-live="polite">
    {#if batch.running}
      <div class="progress">
        <progress max={batch.progress.of} value={batch.progress.settled} aria-label="Batch progress"></progress>
        <span class="small num">
          {batch.progress.settled} of {batch.progress.of}
          {#if batch.counts.failed > 0}· {batch.counts.failed} failed{/if}
        </span>
      </div>
    {/if}
    {#if batch.error !== null}
      <ErrorNotice error={batch.error} ondismiss={() => (batch.error = null)} />
    {/if}
    {#if batch.summary !== null && !batch.running}
      {#if batch.summary.stopped}
        <ErrorNotice error={batch.summary.stopped} />
      {/if}
      <Notice
        tone={batch.summary.stopped || batch.summary.failed > 0 ? "warning" : batch.summary.cancelled ? "neutral" : "success"}
        ondismiss={() => (batch.summary = null)}
      >
        {summaryText(batch.summary)}
        {#if batch.summary.done > 0}
          Results are in <span class="mono">{batch.summary.output_dir}</span>.
        {/if}
        {#if batch.summary.aborted > 0}
          {abortedNote(batch.summary.aborted)}
        {/if}
      </Notice>
    {:else if batch.added !== null && !batch.running}
      <Notice tone="neutral" ondismiss={() => (batch.added = null)}>{batch.added}</Notice>
    {/if}
    {#if !session.hasKey && !empty}
      <Notice tone="info">
        Add your API key to process photos.
        {#snippet actions()}
          <button class="button button--small" type="button" onclick={() => (nav.view = "settings")}>Open Settings</button>
        {/snippet}
      </Notice>
    {/if}
  </footer>
</section>

<style>
  .view {
    display: flex;
    flex-direction: column;
    gap: var(--ps-space-4);
    height: 100%;
    min-height: 0;
    padding: var(--ps-space-5) var(--ps-space-6);
  }
  .view__header {
    display: flex;
    align-items: center;
    gap: var(--ps-space-2);
  }
  .columns {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) 21rem;
    gap: var(--ps-space-4);
    min-height: 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    transition: border-color var(--ps-duration) var(--ps-ease);
  }
  .list--drop {
    border-color: var(--ps-accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ps-accent) 30%, transparent);
  }
  .list__head {
    display: flex;
    gap: 0.4rem;
    padding: 0.5rem var(--ps-space-4);
    border-bottom: 1px solid var(--ps-border);
  }
  .empty {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--ps-space-3);
    max-width: 30rem;
    margin: auto;
    padding: var(--ps-space-8);
    text-align: center;
  }
  .empty__mark {
    width: 4.5rem;
    height: 4.5rem;
    border: 1px dashed var(--ps-border-strong);
    border-radius: var(--ps-radius-lg);
    background-size: 18px 18px;
  }
  .items {
    flex: 1;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--ps-space-3);
    padding: 0.5rem var(--ps-space-4);
    border-bottom: 1px solid var(--ps-border);
  }
  .item:last-child {
    border-bottom: 0;
  }
  .item__text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .item__name {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item__detail {
    overflow: hidden;
    color: var(--ps-text-3);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item__detail--error {
    color: var(--ps-danger-text);
    white-space: normal;
  }
  .item__detail--wrap {
    white-space: normal;
  }
  .panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .panel__scroll {
    flex: 1;
    min-height: 0;
    padding: var(--ps-space-4);
    overflow-y: auto;
  }
  .panel__foot {
    padding: var(--ps-space-4);
    border-top: 1px solid var(--ps-border);
    gap: var(--ps-space-2);
  }
  .path {
    padding: 0.375rem 0.5rem;
    overflow: hidden;
    border-radius: var(--ps-radius-sm);
    background: var(--ps-surface-2);
    color: var(--ps-text-2);
    font-size: 0.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cost {
    min-height: 1.25rem;
    color: var(--ps-text-2);
  }
  .cost--short {
    color: var(--ps-danger-text);
  }
  .status {
    display: flex;
    flex-direction: column;
    gap: var(--ps-space-2);
  }
  .status:empty {
    display: none;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: var(--ps-space-3);
  }
  progress {
    flex: 1;
    height: 0.5rem;
    overflow: hidden;
    border: 0;
    border-radius: var(--ps-radius-pill);
    background: var(--ps-surface-3);
    accent-color: var(--ps-brand);
    appearance: none;
  }
  progress::-webkit-progress-bar {
    background: var(--ps-surface-3);
  }
  progress::-webkit-progress-value {
    border-radius: var(--ps-radius-pill);
    background: var(--ps-brand);
    transition: width var(--ps-duration) var(--ps-ease);
  }
  progress::-moz-progress-bar {
    border-radius: var(--ps-radius-pill);
    background: var(--ps-brand);
  }
</style>
