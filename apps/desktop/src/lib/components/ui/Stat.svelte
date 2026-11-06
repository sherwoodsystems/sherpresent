<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    tone?: 'ok' | 'warn' | 'bad';
    /** Under the value */
    below?: Snippet;
    children: Snippet;
  }

  let { label, tone, below, children }: Props = $props();
</script>

<div class="stat">
  <span class="stat-label">{label}</span>
  <span class="stat-value {tone ?? ''}">{@render children()}</span>
  {@render below?.()}
</div>

<style>
  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
    padding: 0.5rem 0.75rem;
    background: var(--subtle);
    border-radius: 8px;
  }

  .stat-label {
    font-size: 0.65rem;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .stat-value {
    font-size: 0.95rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }

  .ok {
    color: var(--success);
  }

  .warn {
    color: var(--warning);
  }

  .bad {
    color: var(--danger);
  }
</style>
