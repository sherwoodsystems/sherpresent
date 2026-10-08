<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { OutputStatus, SyphonOutputConfig } from '../types';

  interface Props {
    /** Prefix for element ids, unique per page */
    id: string;
    label: string;
    config: SyphonOutputConfig;
    status: OutputStatus | undefined;
    /** Used when the name field is cleared */
    defaultName: string;
    onchange: (config: SyphonOutputConfig) => void;
    /** Extra fields shown while the output is enabled */
    children?: Snippet;
  }

  let { id, label, config, status, defaultName, onchange, children }: Props = $props();

  const summary = $derived.by(() => {
    if (!status) return '';
    switch (status.state) {
      case 'running': {
        const receivers = status.hasClients ? 'Receiver connected' : 'No receivers';
        return status.message ? `${status.message} · ${receivers}` : receivers;
      }
      case 'starting':
        return 'Starting…';
      case 'error':
        return status.message ?? 'Error';
      default:
        return 'Stopped';
    }
  });
</script>

<div class="config-grid">
  <div class="field span">
    <label class="check-row">
      <input
        type="checkbox"
        checked={config.enabled}
        onchange={(e) => onchange({ ...config, enabled: e.currentTarget.checked })}
      />
      <span class="label">{label}</span>
    </label>
  </div>

  {#if config.enabled}
    <div class="field">
      <label class="label" for="{id}-name">Source Name</label>
      <input
        id="{id}-name"
        class="input"
        value={config.serverName}
        onchange={(e) =>
          onchange({ ...config, serverName: e.currentTarget.value.trim() || defaultName })}
      />
    </div>
    {@render children?.()}
    {#if status}
      <div class="field status-field">
        <div class="status-row">
          <span
            class="status-dot"
            class:ok={status.state === 'running' && status.hasClients}
            class:idle={status.state === 'running' && !status.hasClients}
            class:bad={status.state === 'error'}
          ></span>
          <span class="status-text">{summary}</span>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .span {
    grid-column: 1 / -1;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .input {
    font-family: inherit;
    width: 100%;
    box-sizing: border-box;
  }

  .status-field {
    justify-content: flex-end;
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }

  .status-dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warning);
  }

  .status-dot.idle {
    background: #8e8e93;
  }

  .status-dot.bad {
    background: #ff3b30;
  }

  .status-dot.ok {
    background: #34c759;
  }

  .status-text {
    font-size: 0.75rem;
    color: #555;
  }
  @media (prefers-color-scheme: dark) {
    .status-text {
      color: #bbb;
    }
  }
</style>
