<script lang="ts">
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

<main class="container">
  <header class="header">
    <h1>SherPresent</h1>
    <p class="subtitle">Presentation Remote Control</p>
  </header>

  {#if appStore.configLoaded}
    <section class="section status-section">
      <StatusDisplay
        status={appStore.liveStatus}
        notes={appStore.notesCache[appStore.liveStatus?.current_slide ?? 0]}
        adapter={appStore.config.adapter}
        onprev={() => appStore.prevSlide()}
        onnext={() => appStore.nextSlide()}
        ongoto={appStore.supportsGoto ? (slide) => appStore.gotoSlide(slide) : undefined}
        onpagenotes={() => appStore.pageNotes()}
      />
    </section>

    <section class="section">
      <AppSelector config={appStore.config} onchange={(a) => appStore.updateAdapter(a)} />
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

    <section class="quit-section">
      <button
        class="quit-btn"
        onclick={() => {
          if (confirm('Are you sure you want to quit SherPresent?')) {
            invoke('quit_app');
          }
        }}
      >
        Quit SherPresent
      </button>
    </section>

    {#if appStore.config.adapter === 'powerpoint' && appStore.powerpointEvents}
      <p class="events-line" class:ok={appStore.powerpointEvents.state === 'receiving'}>
        <span class="dot"></span>{eventsLine(appStore.powerpointEvents)}
      </p>
    {/if}
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

  .status-section {
    grid-column: 1 / -1;
  }

  .loading {
    grid-column: 1 / -1;
    text-align: center;
    color: var(--text-muted);
    font-style: italic;
  }

  .events-line {
    grid-column: 1 / -1;
    margin: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    font-size: 0.8125rem;
    color: #dc3545;
  }

  .events-line.ok {
    color: #198754;
  }

  .events-line .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: currentColor;
  }

  .quit-section {
    grid-column: 1 / -1;
    text-align: center;
  }

  .quit-btn {
    width: 100%;
    padding: 0.875rem 1.5rem;
    font-size: 1rem;
    font-weight: 600;
    color: #fff;
    background: #dc3545;
    border: none;
    border-radius: 12px;
    cursor: pointer;
    transition: background 0.15s;
  }

  .quit-btn:hover {
    background: #c82333;
  }
  @media (prefers-color-scheme: dark) {
    .quit-btn {
      background: #a82030;
    }

    .quit-btn:hover {
      background: #c82333;
    }
  }
</style>
