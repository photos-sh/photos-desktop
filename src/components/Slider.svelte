<script lang="ts">
  interface Props {
    id: string;
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    /** Said under the slider. */
    help?: string;
    disabled?: boolean;
  }

  let { id, label, value = $bindable(), min, max, step = 1, help, disabled = false }: Props = $props();
</script>

<div class="field">
  <label for={id}>{label} <span class="value num">{value}</span></label>
  <input {id} type="range" {min} {max} {step} {disabled} aria-describedby={help ? `${id}-help` : undefined} bind:value />
  {#if help}<span class="help" id="{id}-help">{help}</span>{/if}
</div>

<style>
  .value {
    margin-left: 0.25rem;
    color: var(--ps-text-3);
    font-weight: 500;
  }
</style>
