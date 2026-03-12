<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { AdapterType, AdapterConfig, ConnectionStatus } from '../types';
  import { appStore } from '$lib/state.svelte';

  interface Props {
    adapter: AdapterType;
    adapterConfig: AdapterConfig;
    connectionStatus: ConnectionStatus;
    onchange: (config: AdapterConfig) => void;
  }

  let { adapter, adapterConfig, connectionStatus, onchange }: Props = $props();

  // LibreOffice fields
  let loHost = $state('127.0.0.1');
  let loPort = $state(1599);

  // Canva fields
  let canvaUrl = $state('');

  // Sync from props
  $effect(() => {
    if (adapterConfig.type === 'libreoffice') {
      loHost = adapterConfig.host;
      loPort = adapterConfig.port;
    } else if (adapterConfig.type === 'canva') {
      canvaUrl = adapterConfig.url;
    }
  });

  function updateLibreOffice() {
    onchange({ type: 'libreoffice', host: loHost, port: loPort });
  }

  async function connectCanva() {
    if (!canvaUrl.trim()) return;
    onchange({ type: 'canva', url: canvaUrl });
    await appStore.openCanvaRemote(canvaUrl);
  }

  async function disconnectCanva() {
    await appStore.closeCanvaRemote();
  }

  let statusText = $derived.by(() => {
    if (connectionStatus === 'Connected') return 'Connected';
    if (connectionStatus === 'Connecting') return 'Connecting...';
    if (connectionStatus === 'Disconnected') return 'Disconnected';
    if (typeof connectionStatus === 'object' && 'Error' in connectionStatus) return `Error: ${connectionStatus.Error}`;
    return '';
  });

  let statusColor = $derived.by(() => {
    if (connectionStatus === 'Connected') return '#34c759';
    if (connectionStatus === 'Connecting') return '#ff9500';
    if (connectionStatus === 'Disconnected') return '#888';
    return '#ff3b30';
  });
</script>

{#if adapter === 'libreoffice'}
  <div class="adapter-config">
    <label class="label">LibreOffice Connection</label>
    <div class="row">
      <div class="field">
        <label class="field-label">Host</label>
        <input
          type="text"
          class="input"
          bind:value={loHost}
          onblur={updateLibreOffice}
          placeholder="127.0.0.1"
        />
      </div>
      <div class="field field-small">
        <label class="field-label">Port</label>
        <input
          type="number"
          class="input"
          bind:value={loPort}
          onblur={updateLibreOffice}
          min="1"
          max="65535"
        />
      </div>
    </div>
    <p class="hint">
      Default: 127.0.0.1:1599. Change host to connect to a remote LibreOffice instance.
    </p>
  </div>
{:else if adapter === 'canva'}
  <div class="adapter-config">
    <label class="label">Canva Remote Control</label>
    <div class="row">
      <input
        type="text"
        class="input"
        bind:value={canvaUrl}
        placeholder="Paste Canva remote control URL..."
        style="flex: 1"
      />
    </div>
    <div class="row" style="gap: 0.5rem; align-items: center;">
      {#if connectionStatus === 'Connected'}
        <button class="btn btn-disconnect" onclick={disconnectCanva}>Disconnect</button>
      {:else}
        <button class="btn btn-connect" onclick={connectCanva} disabled={!canvaUrl.trim()}>Connect</button>
      {/if}
      <span class="status" style="color: {statusColor}">{statusText}</span>
    </div>
    <p class="hint">
      In Canva, start presenting, then share the remote control link and paste it here.
    </p>
  </div>
{/if}

<style>
  .adapter-config {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .label {
    font-size: 0.875rem;
    font-weight: 500;
    color: #666;
  }

  .row {
    display: flex;
    gap: 0.75rem;
  }

  .field {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .field-small {
    flex: 0 0 80px;
  }

  .field-label {
    font-size: 0.75rem;
    color: #888;
  }

  .input {
    padding: 0.5rem;
    border: 1px solid #ddd;
    border-radius: 6px;
    font-size: 0.875rem;
    background: #fff;
    color: #333;
  }

  .btn {
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 6px;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.2s;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-connect {
    background: #007aff;
    color: white;
  }

  .btn-disconnect {
    background: #ff3b30;
    color: white;
  }

  .status {
    font-size: 0.8rem;
    font-weight: 500;
  }

  .hint {
    font-size: 0.75rem;
    color: #888;
    margin: 0.25rem 0 0 0;
    line-height: 1.4;
  }

  @media (prefers-color-scheme: dark) {
    .label {
      color: #aaa;
    }

    .field-label {
      color: #777;
    }

    .input {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .btn-connect {
      background: #0a84ff;
    }

    .btn-disconnect {
      background: #ff453a;
    }
  }
</style>
