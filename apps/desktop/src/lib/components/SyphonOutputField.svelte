<script lang="ts">
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
  }

  let { id, label, config, status, defaultName, onchange }: Props = $props();

  const summary = $derived.by(() => {
    if (!status) return '';
    switch (status.state) {
      case 'running':
        return status.hasClients ? 'Receiver connected' : 'No receivers';
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
  .config-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .span {
    grid-column: 1 / -1;
  }

  .label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #666;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .input {
    padding: 0.5rem 0.75rem;
    border: 2px solid #ddd;
    border-radius: 6px;
    background: #fff;
    font-size: 0.875rem;
    font-family: inherit;
    width: 100%;
    box-sizing: border-box;
  }

  .input:focus {
    outline: none;
    border-color: #007aff;
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
    background: #b8860b;
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
    .label {
      color: #aaa;
    }

    .input {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .input:focus {
      border-color: #0a84ff;
    }

    .status-text {
      color: #bbb;
    }

    .status-dot {
      background: #d9a441;
    }
  }
</style>
