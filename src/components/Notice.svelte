<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    tone?: "danger" | "success" | "info" | "warning" | "neutral";
    /** Errors interrupt a screen reader; the rest wait their turn. */
    children: Snippet;
    actions?: Snippet;
    ondismiss?: () => void;
  }

  let { tone = "info", children, actions, ondismiss }: Props = $props();
</script>

<div class="notice tone-{tone}" role={tone === "danger" ? "alert" : "status"}>
  <div class="notice__text selectable">{@render children()}</div>
  {#if actions}
    <div class="notice__actions">{@render actions()}</div>
  {/if}
  {#if ondismiss}
    <button class="button button--quiet button--icon" type="button" aria-label="Dismiss" onclick={ondismiss}>
      <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
        <path d="M6 6l12 12M18 6L6 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
      </svg>
    </button>
  {/if}
</div>

<style>
  .notice {
    display: flex;
    align-items: center;
    gap: var(--ps-space-3);
    padding: 0.625rem 0.875rem;
    border: 1px solid color-mix(in srgb, var(--tone) 35%, transparent);
    border-radius: var(--ps-radius-sm);
    background: var(--tone-soft);
    color: var(--tone-text);
    font-size: 0.875rem;
  }
  .notice__text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .notice__actions {
    display: flex;
    flex: none;
    gap: var(--ps-space-2);
  }
</style>
