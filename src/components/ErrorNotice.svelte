<script lang="ts">
  import { explain } from "../lib/errors";
  import { nav } from "../lib/nav.svelte";
  import type { AppError, ItemError } from "../lib/types";
  import Notice from "./Notice.svelte";

  interface Props {
    error: AppError | ItemError;
    ondismiss?: () => void;
  }

  let { error, ondismiss }: Props = $props();
  let explained = $derived(explain(error));
</script>

<Notice tone="danger" {ondismiss}>
  {explained.text}
  {#snippet actions()}
    {#if explained.settings && nav.view !== "settings"}
      <button class="button button--small" type="button" onclick={() => (nav.view = "settings")}>
        Open Settings
      </button>
    {/if}
  {/snippet}
</Notice>
