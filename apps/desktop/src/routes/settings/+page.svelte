<script lang="ts">
  import Card from '$lib/components/ui/Card.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import OscConfig from '$lib/components/OscConfig.svelte';
  import WebServerConfig from '$lib/components/WebServerConfig.svelte';
  import { appStore } from '$lib/state.svelte';
</script>

<main class="page">
  <PageHeader title="Settings" subtitle="Network & OSC configuration" />

  {#if appStore.configLoaded}
    <Card title="OSC Configuration">
      <OscConfig config={appStore.config.osc} onchange={(c) => appStore.updateOscConfig(c)} />
    </Card>

    <Card
      title="Stage View Server"
      description="Always running — serves notes and timer to browsers on your network"
    >
      <WebServerConfig
        config={appStore.config.webServer}
        onchange={(c) => appStore.updateWebServerConfig(c)}
      />
    </Card>
  {:else}
    <p class="empty loading">Loading...</p>
  {/if}
</main>
