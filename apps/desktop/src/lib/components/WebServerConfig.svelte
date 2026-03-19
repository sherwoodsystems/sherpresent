<script lang="ts">
  import type { WebServerConfig } from '../types';
  import { appStore } from '$lib/state.svelte';
  import ConnectionInfo from './ConnectionInfo.svelte';

  interface Props {
    config: WebServerConfig;
    onchange: (config: WebServerConfig) => void;
  }

  let { config, onchange }: Props = $props();

  function updateField(field: keyof WebServerConfig, value: string | number | boolean) {
    const updated = { ...config, [field]: value };
    onchange(updated);
  }
</script>

<div class="ws-config">
  <h3 class="section-title">Stage View Server</h3>
  <p class="description">Always running — serves notes and timer to browsers on your network</p>

  {#if appStore.webServerRunning && appStore.webServerUrl}
    <ConnectionInfo label="Stage view available at:" value={appStore.webServerUrl} />
  {/if}

  <div class="config-grid">
    <div class="field">
      <label class="label" for="ws-port">Port</label>
      <input
        id="ws-port"
        type="number"
        class="input"
        value={config.port}
        min="1024"
        max="65535"
        onchange={(e) => updateField('port', parseInt(e.currentTarget.value) || 8080)}
      />
    </div>

    <div class="field">
      <label class="label" for="ws-font-size">Stage Font Size</label>
      <input
        id="ws-font-size"
        type="number"
        class="input"
        value={config.fontSize}
        min="16"
        max="96"
        step="2"
        onchange={(e) => updateField('fontSize', parseInt(e.currentTarget.value) || 32)}
      />
      <span class="hint">px — text size for notes in the stage view</span>
    </div>

    <div class="field">
      <label class="label" for="ontime-host">Ontime Host</label>
      <input
        id="ontime-host"
        type="text"
        class="input"
        value={config.ontimeHost}
        placeholder="e.g. 192.168.1.50"
        onchange={(e) => updateField('ontimeHost', e.currentTarget.value)}
      />
      <span class="hint">IP of the machine running Ontime</span>
    </div>

    <div class="field">
      <label class="label" for="ontime-port">Ontime Port</label>
      <input
        id="ontime-port"
        type="number"
        class="input"
        value={config.ontimePort}
        min="1"
        max="65535"
        onchange={(e) => updateField('ontimePort', parseInt(e.currentTarget.value) || 4001)}
      />
    </div>
  </div>
</div>

<style>
  .ws-config {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .section-title {
    font-size: 0.875rem;
    font-weight: 600;
    color: #333;
    margin: 0;
    padding-bottom: 0.25rem;
  }

  .description {
    font-size: 0.75rem;
    color: #888;
    margin: 0;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid #eee;
  }

  .hint {
    font-size: 0.7rem;
    color: #888;
    margin-top: 0.125rem;
  }

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

  .label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #666;
  }

  .input {
    padding: 0.5rem 0.75rem;
    border: 2px solid #ddd;
    border-radius: 6px;
    background: #fff;
    font-size: 0.875rem;
    font-family: monospace;
  }

  .input:focus {
    outline: none;
    border-color: #007aff;
  }

  @media (prefers-color-scheme: dark) {
    .section-title {
      color: #eee;
    }

    .description {
      color: #777;
      border-bottom-color: #444;
    }

    .hint {
      color: #777;
    }

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
  }
</style>
