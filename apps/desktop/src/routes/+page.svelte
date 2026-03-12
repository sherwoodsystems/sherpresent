<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  
  import AppSelector from '$lib/components/AppSelector.svelte';
  import AdapterConfig from '$lib/components/AdapterConfig.svelte';
  import PresentationPicker from '$lib/components/PresentationPicker.svelte';
  import OscConfig from '$lib/components/OscConfig.svelte';
  import StatusDisplay from '$lib/components/StatusDisplay.svelte';
  import ChannelConfig from '$lib/components/ChannelConfig.svelte';
  import PeerDiscovery from '$lib/components/PeerDiscovery.svelte';
  
  import { appStore } from '$lib/state.svelte';

  onMount(() => {
    appStore.init();
  });

  onDestroy(() => {
    appStore.destroy();
  });
</script>

<main class="container">
  <header class="header">
    <h1>sher-present</h1>
    <p class="subtitle">Presentation Remote Control</p>
  </header>

  {#if appStore.configLoaded}
    <section class="section">
      <ChannelConfig 
        config={appStore.config.channel} 
        onchange={(c) => appStore.updateChannelConfig(c)} 
      />
    </section>

    <section class="section">
      <AppSelector
        config={appStore.config}
        onchange={(a) => appStore.updateAdapter(a)}
      />
    </section>

    {#if appStore.config.adapter === 'libreoffice' || appStore.config.adapter === 'canva'}
      <section class="section">
        <AdapterConfig
          adapter={appStore.config.adapter}
          adapterConfig={appStore.config.adapterConfig}
          connectionStatus={appStore.connectionStatus}
          onchange={(c) => appStore.updateAdapterConfig(c)}
        />
      </section>
    {/if}

    {#if appStore.config.adapter !== 'canva'}
      <section class="section">
        <PresentationPicker
          adapter={appStore.config.adapter}
          selectedPresentation={appStore.config.presentationName}
          onselect={(n) => appStore.selectPresentation(n)}
        />
      </section>
    {/if}

    <section class="section">
      <OscConfig 
        config={appStore.config.osc} 
        onchange={(c) => appStore.updateOscConfig(c)} 
      />
    </section>

    <section class="section">
      <PeerDiscovery />
    </section>

    <section class="section">
      <StatusDisplay 
        status={appStore.liveStatus} 
        adapter={appStore.config.adapter} 
      />
    </section>
  {:else}
    <p class="loading">Loading...</p>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen,
      Ubuntu, Cantarell, 'Open Sans', 'Helvetica Neue', sans-serif;
    background: #fafafa;
    color: #333;
  }

  .container {
    max-width: 480px;
    margin: 0 auto;
    padding: 1.5rem;
  }

  .header {
    text-align: center;
    margin-bottom: 1.5rem;
  }

  .header h1 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 700;
    color: #333;
  }

  .subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.875rem;
    color: #888;
  }

  .section {
    background: #fff;
    border-radius: 12px;
    padding: 1rem;
    margin-bottom: 1rem;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  .loading {
    text-align: center;
    color: #888;
    font-style: italic;
  }

  @media (prefers-color-scheme: dark) {
    :global(body) {
      background: #1a1a1a;
      color: #eee;
      color-scheme: dark;
    }

    .header h1 {
      color: #eee;
    }

    .subtitle {
      color: #777;
    }

    .section {
      background: #2a2a2a;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
    }

    .loading {
      color: #777;
    }
  }
</style>
