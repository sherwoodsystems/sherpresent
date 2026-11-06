<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { save } from '@tauri-apps/plugin-dialog';
  import { appStore } from '$lib/state.svelte';

  let scrollContainer: HTMLElement;
  let notesViewUrl = $state('');

  const totalSlides = $derived(appStore.liveStatus?.total_slides ?? 0);
  const currentSlide = $derived(appStore.liveStatus?.current_slide ?? 0);
  const scanProgress = $derived(appStore.notesScanProgress);
  const isScanning = $derived(scanProgress !== null && scanProgress.status === 'scanning');

  // Canva and LibreOffice can't bulk-fetch notes — they need scan
  const needsScan = $derived(
    appStore.config.adapter === 'canva' || appStore.config.adapter === 'libreoffice'
  );

  const slides = $derived.by(() => {
    const result: Array<{ number: number; notes: string | null }> = [];
    for (let i = 1; i <= totalSlides; i++) {
      const key = String(i);
      result.push({
        number: i,
        notes: appStore.notesCache[key] ?? null
      });
    }
    return result;
  });

  const notesCount = $derived(slides.filter((s) => s.notes !== null).length);

  async function scrollToCurrent() {
    await tick();
    const el = scrollContainer?.querySelector(`[data-slide="${currentSlide}"]`);
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }

  $effect(() => {
    if (currentSlide > 0) {
      scrollToCurrent();
    }
  });

  async function openNotesView() {
    if (notesViewUrl) {
      await openUrl(notesViewUrl);
    }
  }

  async function exportMarkdown() {
    const lines: string[] = [];
    for (const slide of slides) {
      if (slide.notes) {
        lines.push(`# Slide ${slide.number}\n\n${slide.notes}\n`);
      }
    }
    const markdown = lines.join('\n');

    const defaultName = appStore.config.presentationName
      ? `${appStore.config.presentationName} - Notes.md`
      : 'Presentation Notes.md';

    const filePath = await save({
      defaultPath: defaultName,
      filters: [{ name: 'Markdown', extensions: ['md'] }]
    });

    if (filePath) {
      await invoke('save_text_file', { path: filePath, content: markdown });
    }
  }

  onMount(async () => {
    appStore.fetchAllNotes();
    try {
      notesViewUrl = await invoke<string>('get_web_server_url');
    } catch {
      // web server config not available
    }
  });
</script>

<main class="notes-page" bind:this={scrollContainer}>
  <PageHeader
    title="Notes Outline"
    subtitle={totalSlides > 0
      ? `${notesCount} of ${totalSlides} slides have notes`
      : 'No presentation active'}
  >
    {#snippet actions()}
      {#if notesCount > 0}
        <Button onclick={exportMarkdown} title="Export notes as Markdown">Export</Button>
      {/if}
      {#if notesViewUrl}
        <Button onclick={openNotesView} title="Open notes webview">{notesViewUrl} ↗</Button>
      {/if}
      {#if isScanning}
        <Button variant="danger" onclick={() => appStore.stopNotesScan()}>Cancel</Button>
      {:else if needsScan}
        <Button variant="primary" onclick={() => appStore.startNotesScan()}>Scan All Slides</Button>
      {:else}
        <Button onclick={() => appStore.fetchAllNotes()}>Refresh</Button>
      {/if}
    {/snippet}
  </PageHeader>

  {#if scanProgress}
    <div class="scan-progress">
      <div class="progress-bar">
        <div
          class="progress-fill"
          style="width: {scanProgress.total > 0
            ? (scanProgress.current / scanProgress.total) * 100
            : 0}%"
        ></div>
      </div>
      <span class="hint">
        {#if scanProgress.status === 'scanning'}
          Scanning slide {scanProgress.current} of {scanProgress.total}...
        {:else if scanProgress.status === 'complete'}
          Scan complete
        {:else if scanProgress.status === 'cancelled'}
          Scan cancelled
        {/if}
      </span>
    </div>
  {/if}

  {#if totalSlides === 0}
    <p class="empty">Select a presentation and start presenting to see notes.</p>
  {:else}
    <div class="slides">
      {#each slides as slide (slide.number)}
        <button
          type="button"
          class="slide-row"
          class:current={slide.number === currentSlide}
          data-slide={slide.number}
          disabled={!appStore.supportsGoto}
          onclick={() => appStore.gotoSlide(slide.number)}
        >
          <span class="slide-number">{slide.number}</span>
          <span class="slide-notes" class:empty-notes={!slide.notes}>
            {slide.notes ?? 'No notes'}
          </span>
        </button>
      {/each}
    </div>
  {/if}
</main>

<style>
  .notes-page {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    overflow-y: auto;
    max-height: calc(100vh - 50px);
  }

  .progress-bar {
    height: 4px;
    background: var(--border-subtle);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--link);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .slides {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .slide-row {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    width: 100%;
    text-align: left;
    padding: 0.75rem 1rem;
    color: inherit;
    background: var(--surface);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    border-left: 3px solid transparent;
    cursor: pointer;
    transition:
      background 0.15s,
      border-color 0.15s;
    font: inherit;
  }

  .slide-row:hover:not(:disabled) {
    background: var(--hover);
    border-color: var(--border);
    border-left-color: var(--border-strong);
  }

  .slide-row:disabled {
    cursor: default;
  }

  .slide-row.current {
    border-left-color: var(--link);
    background: var(--info-bg);
  }

  .slide-number {
    font-weight: 700;
    font-size: 0.9rem;
    color: var(--text-secondary);
    min-width: 2rem;
    flex-shrink: 0;
  }

  .slide-notes {
    font-size: 0.85rem;
    color: var(--text);
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .empty-notes {
    color: var(--text-faint);
    font-style: italic;
  }
</style>
