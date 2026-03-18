<script lang="ts">
  import AppSelector from '$lib/components/AppSelector.svelte';
  import AdapterConfig from '$lib/components/AdapterConfig.svelte';
  import PresentationPicker from '$lib/components/PresentationPicker.svelte';
  import StatusDisplay from '$lib/components/StatusDisplay.svelte';
  import NetworkOverview from '$lib/components/NetworkOverview.svelte';

  import { appStore } from '$lib/state.svelte';
</script>

<main class="container">
  <header class="header">
    <h1>sher-present</h1>
    <p class="subtitle">Presentation Remote Control</p>
  </header>

  {#if appStore.configLoaded}
    <section class="section status-section">
      <StatusDisplay
        status={appStore.liveStatus}
        adapter={appStore.config.adapter}
        onprev={() => appStore.prevSlide()}
        onnext={() => appStore.nextSlide()}
        ongoto={(slide) => appStore.gotoSlide(slide)}
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

    {#if appStore.config.adapter !== 'canva' && appStore.config.adapter !== 'libreoffice'}
      <section class="section">
        <PresentationPicker
          adapter={appStore.config.adapter}
          selectedPresentation={appStore.config.presentationName}
          onselect={(n) => appStore.selectPresentation(n)}
        />
      </section>
    {/if}

    <section class="section">
      <NetworkOverview />
    </section>

  {:else}
    <p class="loading">Loading...</p>
  {/if}
</main>

<style>
  .container {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 1rem;
  }

  .header {
    grid-column: 1 / -1;
    text-align: center;
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
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  .status-section {
    grid-column: 1 / -1;
  }

  .loading {
    grid-column: 1 / -1;
    text-align: center;
    color: #888;
    font-style: italic;
  }

  @media (prefers-color-scheme: dark) {
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
