<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import Card from '$lib/components/ui/Card.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import StatusLine from '$lib/components/ui/StatusLine.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import AppSelector from '$lib/components/AppSelector.svelte';
  import AdapterConfig from '$lib/components/AdapterConfig.svelte';
  import PresentationPicker from '$lib/components/PresentationPicker.svelte';
  import StatusDisplay from '$lib/components/StatusDisplay.svelte';
  import NetworkOverview from '$lib/components/NetworkOverview.svelte';

  import { appStore } from '$lib/state.svelte';
  import type { PowerPointEvents } from '$lib/types';

  const eventsDetail: Record<Exclude<PowerPointEvents['state'], 'receiving'>, string> = {
    no_show: 'no slide show running',
    listening: 'listening, none yet',
    failed: ''
  };

  /** One line on whether PowerPoint's events arrive; polling covers them either way. */
  function eventsLine(events: PowerPointEvents): string {
    if (events.state === 'receiving') return 'PowerPoint events being received';
    const why = events.state === 'failed' ? events.error : eventsDetail[events.state];
    return `PowerPoint events not received (${why})`;
  }
</script>

<main class="control-page">
  <PageHeader title="SherPresent" subtitle="Presentation Remote Control" />

  {#if appStore.configLoaded}
    <Card title="Live Status" wide>
      <StatusDisplay
        status={appStore.liveStatus}
        notes={appStore.notesCache[appStore.liveStatus?.current_slide ?? 0]}
        adapter={appStore.config.adapter}
        onprev={() => appStore.prevSlide()}
        onnext={() => appStore.nextSlide()}
        ongoto={appStore.supportsGoto ? (slide) => appStore.gotoSlide(slide) : undefined}
        onpagenotes={() => appStore.pageNotes()}
      />
    </Card>

    <Card title="Application">
      <AppSelector config={appStore.config} onchange={(a) => appStore.updateAdapter(a)} />
    </Card>

    {#if appStore.config.adapter === 'libreoffice' || appStore.config.adapter === 'canva'}
      <Card
        title={appStore.config.adapter === 'canva'
          ? 'Canva Remote Control'
          : 'LibreOffice Connection'}
      >
        <AdapterConfig
          adapter={appStore.config.adapter}
          adapterConfig={appStore.config.adapterConfig}
          connectionStatus={appStore.connectionStatus}
          onchange={(c) => appStore.updateAdapterConfig(c)}
        />
      </Card>
    {:else}
      <Card title="Presentation">
        <PresentationPicker
          adapter={appStore.config.adapter}
          selectedPresentation={appStore.config.presentationName}
          onselect={(n) => appStore.selectPresentation(n)}
        />
      </Card>
    {/if}

    <Card title="Network Overview" wide>
      <NetworkOverview />
    </Card>

    <div class="wide">
      <Button
        variant="danger"
        full
        onclick={() => {
          if (confirm('Are you sure you want to quit SherPresent?')) {
            invoke('quit_app');
          }
        }}
      >
        Quit SherPresent
      </Button>
    </div>

    {#if appStore.config.adapter === 'powerpoint' && appStore.powerpointEvents}
      <div class="wide">
        <StatusLine tone={appStore.powerpointEvents.state === 'receiving' ? 'ok' : 'bad'} center>
          {eventsLine(appStore.powerpointEvents)}
        </StatusLine>
      </div>
    {/if}
  {:else}
    <p class="empty loading">Loading...</p>
  {/if}
</main>

<style>
  .control-page {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 1rem;
  }

  .wide {
    grid-column: 1 / -1;
  }
</style>
