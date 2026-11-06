<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    tone?: 'ok' | 'idle' | 'warn' | 'bad';
    mono?: boolean;
    center?: boolean;
    children: Snippet;
  }

  let { tone = 'warn', mono = false, center = false, children }: Props = $props();
</script>

<div class="status-line" class:center>
  <span class="dot {tone}"></span>
  <span class="text" class:mono>{@render children()}</span>
</div>

<style>
  .status-line {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }

  .center {
    justify-content: center;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .ok {
    background: var(--dot-ok);
  }

  .idle {
    background: var(--dot-idle);
  }

  .warn {
    background: var(--warning);
  }

  .bad {
    background: var(--dot-bad);
  }

  .text {
    font-size: 0.8rem;
    color: var(--text-secondary);
    overflow-wrap: anywhere;
  }

  .mono {
    font-family: monospace;
  }
</style>
