<script lang="ts">
  import type { WebServerConfig } from '../types';
  import { appStore } from '$lib/state.svelte';

  interface Props {
    config: WebServerConfig;
    onchange: (config: WebServerConfig) => void;
  }

  let { config, onchange }: Props = $props();

  function updateField(field: keyof WebServerConfig, value: string | number | boolean) {
    const updated = { ...config, [field]: value };
    onchange(updated);
  }

  async function toggleServer() {
    if (appStore.webServerRunning) {
      await appStore.stopWebServer();
    } else {
      // Ensure enabled before starting
      if (!config.enabled) {
        updateField('enabled', true);
      }
      await appStore.startWebServer();
    }
  }

  function copyUrl() {
    if (appStore.webServerUrl) {
      navigator.clipboard.writeText(appStore.webServerUrl);
    }
  }
</script>

<div class="ws-config">
  <h3 class="section-title">Stage View Server</h3>
  <p class="description">Serve notes and timer to external browsers on your network</p>

  <div class="toggle-row">
    <label class="toggle-label">
      <input
        type="checkbox"
        checked={config.enabled}
        onchange={(e) => updateField('enabled', e.currentTarget.checked)}
      />
      Enable on startup
    </label>

    <button
      class="server-toggle"
      class:running={appStore.webServerRunning}
      onclick={toggleServer}
    >
      {appStore.webServerRunning ? 'Stop Server' : 'Start Server'}
    </button>
  </div>

  {#if appStore.webServerRunning && appStore.webServerUrl}
    <div class="connection-info">
      <span class="connection-label">Stage view available at:</span>
      <div class="url-row">
        <code class="connection-address">{appStore.webServerUrl}</code>
        <button class="copy-btn" onclick={copyUrl}>Copy</button>
      </div>
    </div>
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
      <!-- spacer -->
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

  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .toggle-label {
    font-size: 0.8rem;
    color: #666;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .server-toggle {
    padding: 0.375rem 1rem;
    border: none;
    border-radius: 6px;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    background: #007aff;
    color: #fff;
  }

  .server-toggle:hover {
    background: #005ec4;
  }

  .server-toggle.running {
    background: #ef4444;
  }

  .server-toggle.running:hover {
    background: #dc2626;
  }

  .connection-info {
    background: #e8f4fd;
    border: 1px solid #b3d9f7;
    border-radius: 8px;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .connection-label {
    font-size: 0.75rem;
    color: #1a73e8;
    font-weight: 500;
  }

  .url-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .connection-address {
    font-size: 1.125rem;
    font-weight: 600;
    color: #1a56c4;
  }

  .copy-btn {
    padding: 0.25rem 0.5rem;
    font-size: 0.7rem;
    border: 1px solid #b3d9f7;
    border-radius: 4px;
    background: #fff;
    color: #1a73e8;
    cursor: pointer;
  }

  .copy-btn:hover {
    background: #d0e8fc;
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

    .toggle-label {
      color: #aaa;
    }

    .connection-info {
      background: #1a3a5c;
      border-color: #2a5a8c;
    }

    .connection-label {
      color: #6ab7ff;
    }

    .connection-address {
      color: #8fcfff;
    }

    .copy-btn {
      background: #2a5a8c;
      border-color: #3a6a9c;
      color: #8fcfff;
    }

    .copy-btn:hover {
      background: #3a6a9c;
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
