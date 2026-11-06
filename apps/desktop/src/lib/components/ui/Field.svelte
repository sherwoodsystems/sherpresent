<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label?: string;
    /** The id of the control this labels */
    for?: string;
    hint?: string;
    /** Span both columns of a `.config-grid` */
    span?: boolean;
    /** Beside the label: a current value or a small action */
    aside?: Snippet;
    children: Snippet;
  }

  let { label, for: htmlFor, hint, span = false, aside, children }: Props = $props();
</script>

<div class="field" class:span>
  {#if label}
    <div class="field-head">
      {#if htmlFor}
        <label class="label" for={htmlFor}>{label}</label>
      {:else}
        <span class="label">{label}</span>
      {/if}
      {@render aside?.()}
    </div>
  {/if}
  {@render children()}
  {#if hint}
    <span class="hint">{hint}</span>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    min-width: 0;
  }

  .span {
    grid-column: 1 / -1;
  }

  .field-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }
</style>
