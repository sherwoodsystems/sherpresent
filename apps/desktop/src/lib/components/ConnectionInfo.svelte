<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';

  interface Props {
    label: string;
    value: string;
    /** Adds an open button beside Copy */
    onopen?: () => void;
  }

  let { label, value, onopen }: Props = $props();

  function copy() {
    navigator.clipboard.writeText(value);
  }
</script>

<div class="connection-info">
  <span class="connection-label">{label}</span>
  <div class="url-row">
    <code class="connection-address">{value}</code>
    <div class="actions">
      <Button size="sm" onclick={copy}>Copy</Button>
      {#if onopen}
        <Button size="sm" onclick={onopen}>Open</Button>
      {/if}
    </div>
  </div>
</div>

<style>
  .connection-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.75rem;
    background: var(--info-bg);
    border: 1px solid var(--info-border);
    border-radius: 8px;
  }

  .connection-label {
    font-size: 0.75rem;
    font-weight: 500;
    color: var(--link);
  }

  .url-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .connection-address {
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--info-text);
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
  }
</style>
