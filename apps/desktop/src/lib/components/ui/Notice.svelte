<script lang="ts">
  import type { Snippet } from 'svelte';

  let { tone = 'warn', children }: { tone?: 'warn' | 'error'; children: Snippet } = $props();
</script>

<p class="notice {tone}">{@render children()}</p>

<style>
  .notice {
    margin: 0;
    padding: 0.5rem 0.75rem;
    border-radius: var(--radius-control);
    font-size: 0.8rem;
  }

  .warn {
    color: var(--notice-warn-text);
    background: var(--notice-warn-bg);
  }

  .error {
    color: var(--danger);
    background: var(--danger-bg);
  }
</style>
