<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { appStore } from '$lib/state.svelte';

  let scrollContainer: HTMLElement;

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

  const notesCount = $derived(
    slides.filter(s => s.notes !== null).length
  );

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

  onMount(() => {
    appStore.fetchAllNotes();
  });
</script>

<main class="container" bind:this={scrollContainer}>
  <header class="header">
    <div class="header-row">
      <div>
        <h1>Notes Outline</h1>
        {#if totalSlides > 0}
          <p class="subtitle">{notesCount} of {totalSlides} slides have notes</p>
        {:else}
          <p class="subtitle">No presentation active</p>
        {/if}
      </div>
      <div class="header-actions">
        {#if isScanning}
          <button class="cancel-btn" onclick={() => appStore.stopNotesScan()}>Cancel</button>
        {:else if needsScan}
          <button class="scan-btn" onclick={() => appStore.startNotesScan()}>Scan All Slides</button>
        {:else}
          <button class="refresh-btn" onclick={() => appStore.fetchAllNotes()}>Refresh</button>
        {/if}
      </div>
    </div>
    {#if scanProgress}
      <div class="scan-progress">
        <div class="progress-bar">
          <div class="progress-fill" style="width: {scanProgress.total > 0 ? (scanProgress.current / scanProgress.total * 100) : 0}%"></div>
        </div>
        <span class="progress-text">
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
  </header>

  {#if totalSlides === 0}
    <p class="empty">Select a presentation and start presenting to see notes.</p>
  {:else}
    {#each slides as slide (slide.number)}
      <button
        class="slide-row"
        class:current={slide.number === currentSlide}
        data-slide={slide.number}
        onclick={() => appStore.gotoSlide(slide.number)}
      >
        <span class="slide-number">{slide.number}</span>
        <span class="slide-notes" class:empty-notes={!slide.notes}>
          {slide.notes ?? 'No notes'}
        </span>
      </button>
    {/each}
  {/if}
</main>

<style>
  .container {
    max-width: 800px;
    margin: 0 auto;
    padding: 1.5rem;
    overflow-y: auto;
    max-height: calc(100vh - 50px);
  }

  .header {
    margin-bottom: 1rem;
  }

  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .header h1 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 700;
    color: #333;
  }

  .subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.8rem;
    color: #888;
  }

  .refresh-btn {
    padding: 0.4rem 0.8rem;
    font-size: 0.8rem;
    border: 1px solid #ddd;
    border-radius: 6px;
    background: #fff;
    color: #555;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .refresh-btn:hover {
    background: #f5f5f5;
    border-color: #ccc;
  }

  .header-actions {
    display: flex;
    gap: 0.5rem;
  }

  .scan-btn {
    padding: 0.4rem 0.8rem;
    font-size: 0.8rem;
    border: 1px solid #1a73e8;
    border-radius: 6px;
    background: #1a73e8;
    color: #fff;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .scan-btn:hover {
    background: #1557b0;
    border-color: #1557b0;
  }

  .cancel-btn {
    padding: 0.4rem 0.8rem;
    font-size: 0.8rem;
    border: 1px solid #d93025;
    border-radius: 6px;
    background: #d93025;
    color: #fff;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .cancel-btn:hover {
    background: #b3261e;
    border-color: #b3261e;
  }

  .scan-progress {
    margin-top: 0.75rem;
  }

  .progress-bar {
    height: 4px;
    background: #e8e8e8;
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: #1a73e8;
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .progress-text {
    display: block;
    margin-top: 0.25rem;
    font-size: 0.75rem;
    color: #888;
  }

  .empty {
    text-align: center;
    color: #888;
    font-style: italic;
    padding: 2rem 0;
  }

  .slide-row {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    width: 100%;
    text-align: left;
    padding: 0.75rem 1rem;
    margin-bottom: 0.5rem;
    background: #fff;
    border: 1px solid #e8e8e8;
    border-radius: 8px;
    border-left: 3px solid transparent;
    cursor: pointer;
    transition: all 0.15s ease;
    font: inherit;
  }

  .slide-row:hover {
    background: #f8f9fa;
    border-color: #ddd;
    border-left-color: #ccc;
  }

  .slide-row.current {
    border-left-color: #1a73e8;
    background: #f0f6ff;
  }

  .slide-number {
    font-weight: 700;
    font-size: 0.9rem;
    color: #555;
    min-width: 2rem;
    flex-shrink: 0;
  }

  .slide-notes {
    font-size: 0.85rem;
    color: #333;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .empty-notes {
    color: #bbb;
    font-style: italic;
  }

  @media (prefers-color-scheme: dark) {
    .header h1 {
      color: #eee;
    }

    .subtitle {
      color: #777;
    }

    .refresh-btn {
      background: #333;
      border-color: #555;
      color: #ccc;
    }

    .refresh-btn:hover {
      background: #3a3a3a;
      border-color: #666;
    }

    .empty {
      color: #777;
    }

    .slide-row {
      background: #2a2a2a;
      border-color: #444;
    }

    .slide-row:hover {
      background: #333;
      border-color: #555;
      border-left-color: #666;
    }

    .slide-row.current {
      border-left-color: #6ab7ff;
      background: #1e2d3d;
    }

    .slide-number {
      color: #aaa;
    }

    .slide-notes {
      color: #ddd;
    }

    .empty-notes {
      color: #666;
    }

    .progress-bar {
      background: #444;
    }

    .progress-fill {
      background: #6ab7ff;
    }

    .progress-text {
      color: #777;
    }
  }
</style>
