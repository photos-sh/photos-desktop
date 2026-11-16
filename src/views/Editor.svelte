<script lang="ts">
  import { untrack } from "svelte";

  import ErrorNotice from "../components/ErrorNotice.svelte";
  import Notice from "../components/Notice.svelte";
  import OperationForm from "../components/OperationForm.svelte";
  import Stage from "../components/Stage.svelte";
  import { CANCELLED_TOOL, MAX_SELECT_POINTS, type EditorStore } from "../lib/editor.svelte";
  import { explain } from "../lib/errors";
  import { dimensions, fileSize, plural, typeName } from "../lib/format";
  import { nav } from "../lib/nav.svelte";
  import { FORMATS, build, newDraft, problem } from "../lib/operations";
  import { preview } from "../lib/preview";
  import type { Session } from "../lib/session.svelte";
  import type { Format, OperationKind, Readings, SelectPick, Severity } from "../lib/types";

  interface Props {
    editor: EditorStore;
    session: Session;
    dragging: boolean;
  }

  let { editor, session, dragging }: Props = $props();

  // A preset is for the batch, and so is vectorize: its result is an SVG
  // file, not a picture the next tool could work on.
  type Tool = Exclude<OperationKind, "run" | "vectorize"> | "erase" | "select" | "check";

  interface ToolChoice {
    tool: Tool;
    label: string;
    doing: string;
  }

  const FIRST_TOOL: ToolChoice = {
    tool: "remove_background",
    label: "Remove background",
    doing: "Removing the background…",
  };
  const TOOLS: readonly ToolChoice[] = [
    FIRST_TOOL,
    { tool: "upscale", label: "Upscale", doing: "Upscaling…" },
    { tool: "remove_text", label: "Remove text", doing: "Removing text…" },
    { tool: "erase", label: "Erase", doing: "Erasing…" },
    { tool: "select", label: "Select object", doing: "Cutting out the object…" },
    { tool: "enhance", label: "Enhance", doing: "Enhancing…" },
    { tool: "denoise", label: "Denoise", doing: "Removing noise…" },
    { tool: "colorize", label: "Colourise", doing: "Colourising…" },
    { tool: "restore_faces", label: "Restore faces", doing: "Restoring faces…" },
    { tool: "blur_faces", label: "Hide faces", doing: "Hiding faces…" },
    { tool: "crop", label: "Crop to a shape", doing: "Cropping…" },
    { tool: "expand", label: "Expand", doing: "Expanding…" },
    { tool: "resize", label: "Resize / convert", doing: "Resizing…" },
    { tool: "watermark", label: "Watermark", doing: "Adding the watermark…" },
    { tool: "fix", label: "Fix for a marketplace", doing: "Fixing for the marketplace…" },
    { tool: "recipe", label: "Recipe", doing: "Running the recipe…" },
    { tool: "check", label: "Check", doing: "Checking…" },
  ];

  let tool = $state<Tool>("remove_background");
  let draft = $state(newDraft("remove_background", untrack(() => session.settings?.default_format ?? null)));
  let brushPercent = $state(4);

  // A default format changed in Settings applies from then on.
  let defaultFormat = untrack(() => session.settings?.default_format ?? null);
  $effect(() => {
    const next = session.settings?.default_format ?? null;
    if (next !== defaultFormat) {
      defaultFormat = next;
      draft.format = next ?? "";
    }
  });
  let eraseFormat = $state<Format | "">("");
  let selectPick = $state<SelectPick>("auto");
  let checkMarketplace = $state("");
  let checkSubject = $state(false);
  let checkText = $state(false);
  let confirmDialog = $state<HTMLDialogElement | null>(null);

  let snapshot = $derived(editor.snapshot);
  let busy = $derived(editor.busy !== null);
  let active = $derived(TOOLS.find((t) => t.tool === tool) ?? FIRST_TOOL);

  function choose(next: Tool) {
    tool = next;
    if (next !== "erase" && next !== "select" && next !== "check") draft.kind = next;
  }

  let issue = $derived.by(() => {
    if (!session.hasKey) return "Add your API key in Settings to use the tools.";
    if (tool === "erase") return editor.strokes.length === 0 ? "Paint over what should go, then erase." : null;
    if (tool === "select") return editor.points.length === 0 ? "Click the object in the photo, then cut it out." : null;
    if (tool === "check") return null;
    return problem(draft);
  });

  function apply() {
    if (issue !== null || busy) return;
    if (tool === "erase") void editor.erase(eraseFormat === "" ? null : eraseFormat);
    else if (tool === "check") void editor.check(checkMarketplace === "" ? null : checkMarketplace, checkSubject, checkText);
    else if (tool === "select") {
      void editor.select(selectPick).then((ok) => {
        if (ok) void session.refreshAccount();
      });
    } else {
      void editor.apply(build(draft), active.doing).then((ok) => {
        if (ok) void session.refreshAccount();
      });
    }
  }

  let size = $derived(
    snapshot?.current.width != null && snapshot.current.height != null
      ? { width: snapshot.current.width, height: snapshot.current.height }
      : null,
  );

  $effect(() => {
    if (confirmDialog === null) return;
    if (editor.confirmReplace !== null && !confirmDialog.open) confirmDialog.showModal();
    if (editor.confirmReplace === null && confirmDialog.open) confirmDialog.close();
  });

  let copied = $state(false);

  /** The barcodes, then the text, one to a line. */
  async function copyReadings(readings: Readings) {
    const lines = [...(readings.barcodes ?? []).map((b) => b.value), ...(readings.text ?? [])];
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Selecting the text by hand still works.
    }
  }

  const SEVERITY: Record<Severity, { tone: string; label: string }> = {
    error: { tone: "tone-danger", label: "Breaks a rule" },
    warning: { tone: "tone-warning", label: "Warning" },
    info: { tone: "tone-info", label: "Note" },
  };
</script>

<section class="view" aria-labelledby="editor-title">
  <header class="view__header">
    <div class="grow title">
      <h1 id="editor-title">Editor</h1>
      {#if snapshot}
        <p class="muted small file selectable" title={snapshot.path}>{snapshot.name}</p>
      {:else}
        <p class="muted small">One photo, tool after tool. The original is never changed.</p>
      {/if}
    </div>
    {#if snapshot}
      <button class="button button--quiet" type="button" disabled={busy || !snapshot.can_undo} title="Undo (Ctrl+Z)" onclick={() => editor.undo()}>
        Undo
      </button>
      <button class="button button--quiet" type="button" disabled={busy || !snapshot.can_redo} title="Redo (Ctrl+Shift+Z)" onclick={() => editor.redo()}>
        Redo
      </button>
      <button class="button" type="button" disabled={busy} title="Open another photo (Ctrl+O)" onclick={() => editor.pick()}>Open…</button>
      <button class="button button--primary" type="button" disabled={busy} title="Save as (Ctrl+S)" onclick={() => editor.save()}>
        Save as…
      </button>
      <button class="button button--quiet" type="button" onclick={() => editor.close()}>Close</button>
    {/if}
  </header>

  {#if !snapshot}
    <div class="empty card" class:empty--drop={dragging}>
      <div class="empty__mark checker" aria-hidden="true"></div>
      <h2>{editor.busy ?? "Drop a photo here"}</h2>
      <p class="muted">
        Remove the background, cut out or erase an object, fix light and colour, hide faces, crop, resize, add a
        watermark, fix for a marketplace, or check it.
      </p>
      <button class="button button--primary" type="button" disabled={busy} onclick={() => editor.pick()}>Open a photo…</button>
    </div>
  {:else}
    <div class="columns">
      <div class="canvas card" class:canvas--drop={dragging}>
        <Stage
          current={preview.editorStep(snapshot.session, snapshot.current.id)}
          original={snapshot.cursor > 0 ? preview.editorStep(snapshot.session, 0) : null}
          {size}
          painting={tool === "erase"}
          strokes={editor.strokes}
          {brushPercent}
          onstroke={(stroke) => editor.addStroke(stroke)}
          pointing={tool === "select"}
          points={editor.points}
          onpoint={(point) => editor.addPoint(point)}
          disabled={busy}
        />
        {#if editor.busy !== null}
          <div class="working" role="status">
            <span class="spinner"></span>
            <span>{editor.busy}</span>
            <button class="button button--small" type="button" onclick={() => editor.cancel()}>Cancel</button>
          </div>
        {/if}
        <p class="meta small muted num">
          {[typeName(snapshot.current.content_type), dimensions(snapshot.current.width, snapshot.current.height), fileSize(snapshot.current.bytes)]
            .filter((part) => part)
            .join(" · ")}
          {#if snapshot.current.note}· {snapshot.current.note}{/if}
        </p>
      </div>

      <aside class="panel card" aria-label="Tools">
        <div class="panel__scroll stack stack--loose">
          <div class="tools" role="group" aria-label="Tool">
            {#each TOOLS as item (item.tool)}
              <button
                class="tool"
                type="button"
                aria-pressed={tool === item.tool}
                disabled={busy}
                onclick={() => choose(item.tool)}
              >
                {item.label}
              </button>
            {/each}
          </div>

          <div class="stack">
            {#if tool === "erase"}
              <p class="help">
                Paint over what should go: an object, a blemish, a shadow. It is filled in from its
                surroundings. Painting needs a mouse, a pen or a finger.
              </p>
              <div class="field">
                <label for="editor-brush">Brush size</label>
                <input id="editor-brush" type="range" min="1" max="20" step="0.5" bind:value={brushPercent} disabled={busy} />
              </div>
              <div class="row">
                <button class="button button--small" type="button" disabled={busy || editor.strokes.length === 0} onclick={() => editor.undoStroke()}>
                  Undo stroke
                </button>
                <button class="button button--small" type="button" disabled={busy || editor.strokes.length === 0} onclick={() => editor.clearStrokes()}>
                  Clear
                </button>
              </div>
              <div class="field">
                <label for="editor-erase-format">Format</label>
                <select class="select" id="editor-erase-format" bind:value={eraseFormat} disabled={busy}>
                  <option value="">Same as the photo</option>
                  {#each FORMATS as format (format.value)}
                    <option value={format.value}>{format.label}</option>
                  {/each}
                </select>
              </div>
            {:else if tool === "select"}
              <p class="help">
                Click the object to cut out. The rest of the photo becomes transparent. Click again to add another
                part of it. Clicking needs a mouse, a pen or a finger.
              </p>
              <div class="row">
                <span class="grow small muted num">
                  {editor.points.length >= MAX_SELECT_POINTS
                    ? `${MAX_SELECT_POINTS} points: no more can be added.`
                    : plural(editor.points.length, "point", "points")}
                </span>
                <button class="button button--small" type="button" disabled={busy || editor.points.length === 0} onclick={() => editor.undoPoint()}>
                  Undo point
                </button>
                <button class="button button--small" type="button" disabled={busy || editor.points.length === 0} onclick={() => editor.clearPoints()}>
                  Clear
                </button>
              </div>
              <div class="field">
                <label for="editor-select-pick">When a click could mean a part or the whole</label>
                <select class="select" id="editor-select-pick" bind:value={selectPick} disabled={busy}>
                  <option value="auto">Take the surest answer</option>
                  <option value="smallest">Take the part clicked</option>
                  <option value="largest">Take the whole it belongs to</option>
                </select>
              </div>
              <p class="help">On large photos the edges are softer than Remove background's, and hair is not followed.</p>
            {:else if tool === "check"}
              <div class="field">
                <label for="editor-check-marketplace">Check against</label>
                <select class="select" id="editor-check-marketplace" bind:value={checkMarketplace} disabled={busy}>
                  <option value="">Photo quality only</option>
                  {#each session.catalog?.marketplaces ?? [] as marketplace (marketplace.name)}
                    <option value={marketplace.name}>{marketplace.title || marketplace.name}</option>
                  {/each}
                </select>
                {#if session.catalogError}
                  <span class="help">{explain(session.catalogError).text}</span>
                {/if}
              </div>
              {#if checkMarketplace === ""}
                <label class="check">
                  <input type="checkbox" bind:checked={checkSubject} disabled={busy} /> Also check how the product is framed
                </label>
                <label class="check">
                  <input type="checkbox" bind:checked={checkText} disabled={busy} /> Also look for text
                </label>
              {/if}
            {:else}
              <OperationForm bind:draft catalog={session.catalog} catalogError={session.catalogError} disabled={busy} prefix="editor" />
            {/if}

            {#if issue !== null}
              <p class="small muted" id="editor-issue">
                {issue}
                {#if !session.hasKey}
                  <button class="link" type="button" onclick={() => (nav.view = "settings")}>Open Settings</button>
                {/if}
              </p>
            {/if}
            <button
              class="button button--primary button--wide"
              type="button"
              disabled={issue !== null || busy}
              aria-describedby={issue !== null ? "editor-issue" : undefined}
              onclick={apply}
            >
              {#if busy}<span class="spinner"></span>{/if}
              {tool === "check" ? "Check" : tool === "erase" ? "Erase" : tool === "select" ? "Cut out" : "Apply"}
            </button>
          </div>

          {#if editor.report !== null}
            {@const report = editor.report}
            <div class="report stack" aria-live="polite">
              <h2>Check</h2>
              {#if report.marketplace}
                <p>
                  <span class="pill {report.marketplace.passed ? 'tone-success' : 'tone-danger'}">
                    {report.marketplace.passed ? "Passes" : "Does not pass"}
                  </span>
                  {report.marketplace.title ?? report.marketplace.marketplace}
                </p>
              {/if}
              {#each [...(report.marketplace?.findings ?? []), ...report.findings] as finding, n (n)}
                <div class="finding">
                  <span class="pill {SEVERITY[finding.severity]?.tone ?? 'tone-neutral'}">
                    {SEVERITY[finding.severity]?.label ?? finding.severity}
                  </span>
                  <p class="selectable">{finding.message}</p>
                  {#if finding.detail}<p class="small muted selectable">{finding.detail}</p>{/if}
                </div>
              {:else}
                <p class="muted small">Nothing to remark: the photo looks fine.</p>
              {/each}
              {#if report.marketplace?.not_checked.length}
                <p class="small muted">Not checked: {report.marketplace.not_checked.join(", ")}.</p>
              {/if}
              {#each report.marketplace?.notes ?? [] as note, n (n)}
                <p class="small muted selectable">To check by eye: {note}</p>
              {/each}
            </div>
          {/if}

          {#if snapshot.current.readings !== null}
            {@const readings = snapshot.current.readings}
            <div class="report stack" aria-live="polite">
              <div class="row">
                <h2 class="grow">Read from the photo</h2>
                <button class="button button--small button--quiet" type="button" onclick={() => copyReadings(readings)}>
                  {copied ? "Copied" : "Copy"}
                </button>
              </div>
              {#if readings.barcodes !== null}
                {#each readings.barcodes as code, n (n)}
                  <p class="selectable">
                    <span class="pill tone-neutral">{code.format}</span>
                    <span class="mono">{code.value}</span>
                  </p>
                {:else}
                  <p class="muted small">No barcode found.</p>
                {/each}
              {/if}
              {#if readings.text !== null}
                {#if readings.text.length === 0}
                  <p class="muted small">No text found.</p>
                {:else}
                  <p class="lines selectable">{readings.text.join("\n")}</p>
                {/if}
                {#if readings.text_truncated}
                  <p class="muted small">Only the first lines came back: there was more text than the service sends with a picture.</p>
                {/if}
              {/if}
            </div>
          {/if}

          <div class="stack">
            <h2>Steps</h2>
            <ol class="steps">
              <li>
                <button class="step" type="button" aria-current={snapshot.cursor === 0 ? "step" : undefined} disabled={busy} onclick={() => editor.goTo(0)}>
                  <span>Original</span>
                  <span class="small muted num">{dimensions(snapshot.original.width, snapshot.original.height)}</span>
                </button>
              </li>
              {#each snapshot.steps as step, index (step.id)}
                <li>
                  <button
                    class="step"
                    class:step--undone={index + 1 > snapshot.cursor}
                    type="button"
                    aria-current={snapshot.cursor === index + 1 ? "step" : undefined}
                    disabled={busy}
                    onclick={() => editor.goTo(index + 1)}
                  >
                    <span>{step.label}</span>
                    <span class="small muted num">{dimensions(step.width, step.height)}</span>
                  </button>
                </li>
              {/each}
            </ol>
          </div>
        </div>
      </aside>
    </div>
  {/if}

  <footer class="status" aria-live="polite">
    {#if editor.error !== null}
      <ErrorNotice error={editor.error} ondismiss={() => (editor.error = null)} />
    {/if}
    {#if editor.notice !== null}
      <Notice tone={editor.notice === CANCELLED_TOOL ? "neutral" : "success"} ondismiss={() => (editor.notice = null)}>
        {editor.notice}
      </Notice>
    {/if}
  </footer>

  <dialog
    bind:this={confirmDialog}
    aria-labelledby="replace-title"
    oncancel={(e) => {
      e.preventDefault();
      void editor.confirmSave(false);
    }}
  >
    <h2 id="replace-title">Replace the original photo?</h2>
    <p class="selectable">
      <span class="mono">{editor.confirmReplace}</span> is the photo you opened. Saving here replaces it with the
      edited version, and the original cannot be brought back.
    </p>
    <div class="row dialog__actions">
      <!-- svelte-ignore a11y_autofocus -->
      <button class="button" type="button" autofocus onclick={() => editor.confirmSave(false)}>Keep the original</button>
      <button class="button button--danger" type="button" onclick={() => editor.confirmSave(true)}>Replace it</button>
    </div>
  </dialog>
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
  .title {
    min-width: 0;
  }
  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .columns {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) 21rem;
    gap: var(--ps-space-4);
    min-height: 0;
  }
  .empty {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--ps-space-3);
    padding: var(--ps-space-8);
    text-align: center;
  }
  .empty p {
    max-width: 30rem;
  }
  .empty__mark {
    width: 4.5rem;
    height: 4.5rem;
    border: 1px dashed var(--ps-border-strong);
    border-radius: var(--ps-radius-lg);
    background-size: 18px 18px;
  }
  .empty--drop,
  .canvas--drop {
    border-color: var(--ps-accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ps-accent) 30%, transparent);
  }
  .canvas {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--ps-space-2);
    min-height: 0;
    padding: var(--ps-space-4) var(--ps-space-4) var(--ps-space-2);
    background: var(--ps-surface-2);
  }
  .meta {
    min-height: 1.2rem;
    text-align: center;
  }
  .working {
    position: absolute;
    top: 50%;
    left: 50%;
    display: flex;
    align-items: center;
    gap: var(--ps-space-3);
    padding: 0.625rem 0.875rem;
    border: 1px solid var(--ps-border);
    border-radius: var(--ps-radius);
    background: var(--ps-surface);
    box-shadow: var(--ps-shadow-lg);
    font-weight: 500;
    transform: translate(-50%, -50%);
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
  .tools {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.375rem;
  }
  .tool {
    min-height: 2.25rem;
    padding: 0.375rem 0.5rem;
    border: 1px solid var(--ps-border);
    border-radius: var(--ps-radius-sm);
    background: var(--ps-surface);
    color: var(--ps-text-2);
    font: inherit;
    font-size: 0.8125rem;
    font-weight: 500;
    line-height: 1.2;
    cursor: pointer;
  }
  .tool:hover:not(:disabled) {
    background: var(--ps-surface-2);
  }
  .tool[aria-pressed="true"] {
    border-color: var(--ps-brand);
    background: var(--ps-brand-soft);
    color: var(--ps-text);
    box-shadow: inset 0 0 0 1px var(--ps-brand);
  }
  .tool:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .steps {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .step {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ps-space-2);
    width: 100%;
    padding: 0.375rem 0.625rem;
    border: 1px solid transparent;
    border-radius: var(--ps-radius-sm);
    background: transparent;
    color: var(--ps-text);
    font: inherit;
    font-size: 0.875rem;
    text-align: left;
    cursor: pointer;
  }
  .step:hover:not(:disabled) {
    background: var(--ps-surface-2);
  }
  .step[aria-current="step"] {
    border-color: var(--ps-border-strong);
    background: var(--ps-surface-2);
    font-weight: 600;
  }
  .step--undone {
    color: var(--ps-text-3);
    text-decoration: line-through;
  }
  .report {
    padding: var(--ps-space-3);
    border: 1px solid var(--ps-border);
    border-radius: var(--ps-radius-sm);
    gap: var(--ps-space-2);
  }
  .lines {
    white-space: pre-line;
  }
  .finding {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.2rem;
    font-size: 0.875rem;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--ps-text);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
  .status {
    display: flex;
    flex-direction: column;
    gap: var(--ps-space-2);
  }
  .status:empty {
    display: none;
  }
  dialog {
    max-width: 30rem;
    padding: var(--ps-space-5);
    border: 1px solid var(--ps-border);
    border-radius: var(--ps-radius-lg);
    background: var(--ps-surface);
    color: var(--ps-text);
    box-shadow: var(--ps-shadow-lg);
  }
  dialog::backdrop {
    background: var(--ps-overlay);
  }
  dialog h2 {
    margin-bottom: var(--ps-space-2);
  }
  .dialog__actions {
    justify-content: flex-end;
    margin-top: var(--ps-space-4);
  }
</style>
