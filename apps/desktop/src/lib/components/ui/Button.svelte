<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  interface Props extends HTMLButtonAttributes {
    variant?: 'secondary' | 'primary' | 'success' | 'danger' | 'link';
    size?: 'md' | 'sm';
    full?: boolean;
    children: Snippet;
  }

  let {
    variant = 'secondary',
    size = 'md',
    full = false,
    type = 'button',
    children,
    ...rest
  }: Props = $props();
</script>

<button {type} class="button {variant} {size}" class:full {...rest}>
  {@render children()}
</button>

<style>
  .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    box-sizing: border-box;
    height: var(--control-height);
    padding: 0 1rem;
    border: 2px solid transparent;
    border-radius: var(--radius-control);
    font: inherit;
    font-size: 0.875rem;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background 0.15s,
      border-color 0.15s,
      filter 0.15s;
  }

  .sm {
    height: var(--control-height-sm);
    padding: 0 0.625rem;
    font-size: 0.75rem;
  }

  .full {
    width: 100%;
  }

  .button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .secondary {
    background: var(--input-bg);
    border-color: var(--border);
    color: var(--text);
  }

  .secondary:hover:not(:disabled) {
    background: var(--hover);
    border-color: var(--border-strong);
  }

  .primary,
  .success,
  .danger {
    color: #fff;
    font-weight: 600;
  }

  .primary {
    background: var(--accent);
  }

  .success {
    background: var(--solid-success);
  }

  .danger {
    background: var(--solid-danger);
  }

  .primary:hover:not(:disabled),
  .success:hover:not(:disabled),
  .danger:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .link {
    height: auto;
    padding: 0;
    border: none;
    background: none;
    color: var(--link);
    font-size: 0.75rem;
  }

  .link:hover:not(:disabled) {
    text-decoration: underline;
  }
</style>
