<script lang="ts">
  import type { Updates } from "../lib/updates.svelte";
  import ErrorNotice from "./ErrorNotice.svelte";
  import Notice from "./Notice.svelte";

  interface Props {
    updates: Updates;
    /** An update is not installed while a batch runs. */
    batchRunning: boolean;
    /** A photo is open in the editor: installing restarts the app and closes it. */
    editorOpen: boolean;
  }

  let { updates, batchRunning, editorOpen }: Props = $props();

  let notesOpen = $state(false);
</script>

{#if updates.available !== null && (!updates.dismissed || updates.installing)}
  <div class="banner stack">
    <Notice tone="info" ondismiss={updates.installing ? undefined : () => updates.later()}>
      {#if updates.installing}
        <span class="spinner"></span>
        Downloading Photos.sh {updates.available.version}{#if updates.percent !== null}: <span class="num"
            >{updates.percent}%</span
          >{/if}. The app restarts when it is installed.
      {:else}
        <strong>Photos.sh {updates.available.version} is available.</strong>
        You have {updates.available.current_version}.
        {#if batchRunning}
          It can be installed when the batch has finished.
        {:else if editorOpen}
          The app restarts to install it: save the photo in the editor first.
        {/if}
        {#if notesOpen && updates.available.notes}
          <span class="notes">{updates.available.notes}</span>
        {/if}
      {/if}
      {#snippet actions()}
        {#if !updates.installing}
          {#if updates.available?.notes}
            <button class="button button--small button--quiet" type="button" onclick={() => (notesOpen = !notesOpen)}>
              {notesOpen ? "Hide what's new" : "What's new"}
            </button>
          {/if}
          <button class="button button--small button--primary" type="button" disabled={batchRunning} onclick={() => updates.install()}>
            Install and restart
          </button>
        {/if}
      {/snippet}
    </Notice>
    {#if updates.error !== null}
      <div class="row row--wrap">
        <div class="grow"><ErrorNotice error={updates.error} ondismiss={() => (updates.error = null)} /></div>
        <button class="button button--small" type="button" onclick={() => updates.openReleases()}>Download it from GitHub</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .banner {
    padding: var(--ps-space-3) var(--ps-space-6) 0;
  }
  /* In a line of text, not a flex row. */
  .spinner {
    display: inline-block;
    margin-right: 0.4em;
    vertical-align: -0.15em;
  }
  .notes {
    display: block;
    margin-top: var(--ps-space-2);
    white-space: pre-line;
  }
</style>
