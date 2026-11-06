<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import StatusLine from '$lib/components/ui/StatusLine.svelte';
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
    if (typeof connectionStatus === 'object' && 'Error' in connectionStatus)
      return `Error: ${connectionStatus.Error}`;
    return '';
  });

  let statusTone = $derived.by((): 'ok' | 'warn' | 'idle' | 'bad' => {
    if (connectionStatus === 'Connected') return 'ok';
    if (connectionStatus === 'Connecting') return 'warn';
    if (connectionStatus === 'Disconnected') return 'idle';
    return 'bad';
  });
</script>

{#if adapter === 'libreoffice'}
  <div class="config-grid">
    <Field label="Host" for="lo-host">
      <input
        id="lo-host"
        type="text"
        class="input mono"
        bind:value={loHost}
        onblur={updateLibreOffice}
        placeholder="127.0.0.1"
      />
    </Field>
    <Field label="Port" for="lo-port">
      <input
        id="lo-port"
        type="number"
        class="input mono"
        bind:value={loPort}
        onblur={updateLibreOffice}
        min="1"
        max="65535"
      />
    </Field>
  </div>
  <p class="hint tip">
    Default: 127.0.0.1:1599. Change host to connect to a remote LibreOffice instance.
  </p>
{:else if adapter === 'canva'}
  <input
    type="text"
    class="input"
    aria-label="Canva remote control URL"
    bind:value={canvaUrl}
    placeholder="Paste Canva remote control URL..."
  />
  <div class="connect-row">
    {#if connectionStatus === 'Connected'}
      <Button variant="danger" onclick={disconnectCanva}>Disconnect</Button>
    {:else}
      <Button variant="primary" onclick={connectCanva} disabled={!canvaUrl.trim()}>Connect</Button>
    {/if}
    <StatusLine tone={statusTone}>{statusText}</StatusLine>
  </div>
  <p class="hint tip">
    In Canva, start presenting, then share the remote control link and paste it here.
  </p>
{/if}

<style>
  .connect-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .tip {
    margin: 0;
    line-height: 1.4;
  }
</style>
