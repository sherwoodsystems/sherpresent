<script lang="ts">
  import ChannelConfig from '$lib/components/ChannelConfig.svelte';
  import OscConfig from '$lib/components/OscConfig.svelte';
  import WebServerConfig from '$lib/components/WebServerConfig.svelte';
  import { appStore } from '$lib/state.svelte';
</script>

<main class="container">
  <header class="header">
    <h1>Settings</h1>
    <p class="subtitle">Network & OSC configuration</p>
  </header>

  {#if appStore.configLoaded}
    <section class="section">
      <ChannelConfig
        config={appStore.config.channel}
        onchange={(c) => appStore.updateChannelConfig(c)}
      />
    </section>

    <section class="section">
      <OscConfig
        config={appStore.config.osc}
        onchange={(c) => appStore.updateOscConfig(c)}
      />
    </section>

    <section class="section">
      <WebServerConfig
        config={appStore.config.webServer}
        onchange={(c) => appStore.updateWebServerConfig(c)}
      />
    </section>

  {:else}
    <p class="loading">Loading...</p>
  {/if}
</main>

<style>
  .container {
    max-width: 600px;
    margin: 0 auto;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .header {
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

  .loading {
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
