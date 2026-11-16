<script lang="ts">
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";

  import ErrorNotice from "./components/ErrorNotice.svelte";
  import UpdateBanner from "./components/UpdateBanner.svelte";
  import { BatchStore } from "./lib/batch.svelte";
  import { EditorStore } from "./lib/editor.svelte";
  import { nav, type View } from "./lib/nav.svelte";
  import { Session, accountLine } from "./lib/session.svelte";
  import { Updates } from "./lib/updates.svelte";
  import Batch from "./views/Batch.svelte";
  import Editor from "./views/Editor.svelte";
  import Settings from "./views/Settings.svelte";

  const session = new Session();
  const batch = new BatchStore();
  const editor = new EditorStore();
  const updates = new Updates();

  /** The check at start waits a moment, so it does not hold up the first screen. */
  const UPDATE_CHECK_DELAY_MS = 3000;

  let ready = $state(false);
  let dragging = $state(false);

  const VIEWS: readonly { view: View; label: string; key: string }[] = [
    { view: "batch", label: "Batch", key: "1" },
    { view: "editor", label: "Editor", key: "2" },
    { view: "settings", label: "Settings", key: "3" },
  ];

  onMount(() => {
    const stops: (() => void)[] = [];
    let gone = false;
    const keep = (stop: () => void) => (gone ? stop() : stops.push(stop));

    void (async () => {
      await session.init();
      keep(await batch.init());
      await editor.init();
      ready = true;
      if (session.settings?.check_for_updates) {
        const timer = setTimeout(() => void updates.check(true), UPDATE_CHECK_DELAY_MS);
        keep(() => clearTimeout(timer));
      }
    })();

    // Files dropped on the window go to the view that is showing.
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const drop = event.payload;
        if (drop.type === "enter" || drop.type === "over") {
          dragging = nav.view !== "settings";
        } else if (drop.type === "leave") {
          dragging = false;
        } else {
          dragging = false;
          if (nav.view === "batch") void batch.add(drop.paths);
          else if (nav.view === "editor" && drop.paths[0] !== undefined) void editor.open(drop.paths[0]);
        }
      })
      .then(keep);

    return () => {
      gone = true;
      stops.forEach((stop) => stop());
    };
  });

  function shortcut(event: KeyboardEvent) {
    const command = event.ctrlKey || event.metaKey;
    if (!command || event.altKey) {
      if (event.key === "Escape" && nav.view === "editor" && editor.busy !== null) void editor.cancel();
      return;
    }
    const key = event.key.toLowerCase();
    const view = VIEWS.find((v) => v.key === key);
    if (view) {
      event.preventDefault();
      nav.view = view.view;
      return;
    }
    if (nav.view !== "editor") return;
    // In a text field, Ctrl+Z belongs to the field.
    const typing = event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement;
    if (key === "z" && !typing) {
      event.preventDefault();
      void (event.shiftKey ? editor.redo() : editor.undo());
    } else if (key === "y" && !typing) {
      event.preventDefault();
      void editor.redo();
    } else if (key === "o") {
      event.preventDefault();
      void editor.pick();
    } else if (key === "s") {
      event.preventDefault();
      void editor.save();
    }
  }
</script>

<svelte:window onkeydown={shortcut} />

<div class="shell">
  <nav class="rail" aria-label="Views">
    <div class="wordmark"><span>Photos<span class="wordmark__suffix">.sh</span></span></div>
    <ul class="rail__list">
      {#each VIEWS as item (item.view)}
        <li>
          <button
            class="rail__item"
            type="button"
            aria-current={nav.view === item.view ? "page" : undefined}
            title="{item.label} (Ctrl+{item.key})"
            onclick={() => (nav.view = item.view)}
          >
            {item.label}
            {#if item.view === "batch" && batch.running}
              <span class="spinner" aria-label="running"></span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
    <div class="rail__account small" aria-live="polite">
      {#if !ready}
        <span class="muted">Starting…</span>
      {:else if !session.hasKey}
        <button class="rail__link" type="button" onclick={() => (nav.view = "settings")}>Add your API key</button>
      {:else if session.account !== null}
        <span class="num">{accountLine(session.account)}</span>
      {:else if session.accountLoading}
        <span class="muted">Connecting…</span>
      {:else if session.accountError !== null}
        <button class="rail__link rail__link--problem" type="button" onclick={() => (nav.view = "settings")}>
          Not connected
        </button>
      {/if}
    </div>
  </nav>

  <main class="main">
    {#if session.startError !== null}
      <div class="start-error"><ErrorNotice error={session.startError} /></div>
    {:else if !ready}
      <p class="loading muted" role="status"><span class="spinner"></span> Starting…</p>
    {:else}
      <UpdateBanner {updates} batchRunning={batch.running} editorOpen={editor.isOpen} />
      <!-- All three stay alive, so what was typed or painted survives a switch. -->
      <div class="pane" hidden={nav.view !== "batch"}>
        <Batch {batch} {session} dragging={dragging && nav.view === "batch"} />
      </div>
      <div class="pane" hidden={nav.view !== "editor"}>
        <Editor {editor} {session} dragging={dragging && nav.view === "editor"} />
      </div>
      <div class="pane" hidden={nav.view !== "settings"}>
        <Settings {session} {updates} />
      </div>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 12.5rem minmax(0, 1fr);
    height: 100%;
  }
  .rail {
    display: flex;
    flex-direction: column;
    gap: var(--ps-space-5);
    padding: var(--ps-space-5) var(--ps-space-3) var(--ps-space-4);
    border-right: 1px solid var(--ps-border);
    background: var(--ps-surface);
  }
  .wordmark {
    display: inline-flex;
    align-items: center;
    gap: 0.5em;
    padding: 0 var(--ps-space-2);
    font-size: 1.0625rem;
    font-weight: 600;
    line-height: 1;
    letter-spacing: -0.03em;
  }
  .wordmark::before {
    content: "";
    width: 1.625em;
    height: 1.625em;
    flex: none;
    border-radius: 0.375em;
    background:
      var(--ps-logo-glyph) center / 100% 100% no-repeat,
      repeating-conic-gradient(var(--ps-checker-a) 0% 25%, var(--ps-checker-b) 0% 50%) 0 0 / 50% 50%;
    box-shadow: inset 0 0 0 1px var(--ps-border);
  }
  .wordmark__suffix {
    margin-left: -0.12em;
    color: var(--ps-text-3);
    font-family: var(--ps-mono);
    font-size: 0.92em;
    font-weight: 500;
    letter-spacing: -0.06em;
  }
  .rail__list {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 0.125rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .rail__item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0.5rem 0.75rem;
    border: 0;
    border-radius: var(--ps-radius-sm);
    background: transparent;
    color: var(--ps-text-2);
    font: inherit;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
  }
  .rail__item:hover {
    background: var(--ps-surface-2);
    color: var(--ps-text);
  }
  .rail__item[aria-current="page"] {
    background: var(--ps-brand-soft);
    color: var(--ps-text);
    font-weight: 600;
  }
  .rail__account {
    padding: 0 var(--ps-space-2);
    color: var(--ps-text-2);
  }
  .rail__link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--ps-text);
    font: inherit;
    text-align: left;
    text-decoration: underline;
    text-underline-offset: 0.15em;
    cursor: pointer;
  }
  .rail__link--problem {
    color: var(--ps-danger-text);
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .pane {
    flex: 1;
    min-height: 0;
  }
  .loading {
    display: flex;
    align-items: center;
    gap: var(--ps-space-2);
    padding: var(--ps-space-6);
  }
  .start-error {
    padding: var(--ps-space-6);
  }
</style>
