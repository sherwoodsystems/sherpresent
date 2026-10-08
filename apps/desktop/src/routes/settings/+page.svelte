<script lang="ts">
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
      <OscConfig config={appStore.config.osc} onchange={(c) => appStore.updateOscConfig(c)} />
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
    color: var(--text);
  }

  .subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.875rem;
    color: var(--text-muted);
  }

  .section {
    background: var(--surface);
    border-radius: 12px;
    padding: 1rem;
    box-shadow: var(--shadow-card);
  }

  .loading {
    text-align: center;
    color: var(--text-muted);
    font-style: italic;
  }
</style>
