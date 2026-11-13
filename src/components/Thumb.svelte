<script lang="ts">
  interface Props {
    src: string;
  }

  let { src }: Props = $props();
  let failedSrc = $state<string | null>(null);
</script>

<!-- Decorative: the file's name is right beside it. -->
<span class="thumb checker" aria-hidden="true">
  {#if failedSrc === src}
    <svg viewBox="0 0 24 24" width="20" height="20">
      <path
        d="M4 5h16v14H4zM4 15l4-4 4 4 3-3 5 5"
        fill="none"
        stroke="currentColor"
        stroke-width="1.75"
        stroke-linejoin="round"
      />
      <circle cx="9" cy="9" r="1.25" fill="currentColor" />
    </svg>
  {:else}
    <img {src} alt="" loading="lazy" decoding="async" draggable="false" onerror={() => (failedSrc = src)} />
  {/if}
</span>

<style>
  .thumb {
    display: grid;
    width: 2.75rem;
    height: 2.75rem;
    flex: none;
    place-items: center;
    overflow: hidden;
    border: 1px solid var(--ps-border);
    border-radius: 6px;
    background-size: 10px 10px;
    color: var(--ps-text-3);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
</style>
