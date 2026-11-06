<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import type {
    Presentation,
    RecentEntry,
    ImportProgress,
  } from "$lib/types";
  import SlideRenderer from "$lib/components/SlideRenderer.svelte";
  import SlideStrip from "$lib/components/SlideStrip.svelte";

  type View = "home" | "importing" | "viewer";

  let view = $state<View>("home");
  let canvaUrl = $state("");
  let recentList = $state<RecentEntry[]>([]);
  let importProgress = $state<ImportProgress | null>(null);
  let importError = $state<string | null>(null);

  // Viewer state
  let currentPresentation = $state<Presentation | null>(null);
  let currentSlideIndex = $state(0);
  let viewerScale = $state(0.5);

  let currentSlide = $derived(
    currentPresentation?.slides[currentSlideIndex] ?? null
  );

  async function loadRecent() {
    recentList = await invoke<RecentEntry[]>("list_presentations");
  }

  async function startImport() {
    const url = canvaUrl.trim();
    if (!url) return;

    view = "importing";
    importError = null;
    importProgress = { stage: "Fetching", detail: "Starting import..." };

    try {
      const presentation = await invoke<Presentation>(
        "import_presentation",
        { url }
      );
      console.log("[CANVA] Imported presentation:", presentation.title);
      console.log("[CANVA] Slides:", presentation.slides.length);
      console.log("[CANVA] Fonts:", presentation.fonts.length);
      for (const [i, slide] of presentation.slides.entries()) {
        console.log(`[CANVA] Slide ${i}: ${slide.elements.length} elements, thumbnail: ${slide.thumbnail_url?.slice(0, 60) ?? "none"}`);
        for (const [j, elem] of slide.elements.entries()) {
          console.log(`[CANVA]   elem[${j}]:`, elem);
        }
      }
      currentPresentation = presentation;
      currentSlideIndex = 0;
      view = "viewer";
      canvaUrl = "";
      await loadRecent();
    } catch (e) {
      importError = String(e);
      importProgress = { stage: "Failed", detail: String(e) };
    }
  }

  async function openPresentation(id: string) {
    try {
      currentPresentation = await invoke<Presentation>(
        "get_presentation",
        { id }
      );
      currentSlideIndex = 0;
      view = "viewer";
    } catch (e) {
      console.error("Failed to load presentation:", e);
    }
  }

  async function deletePresentation(id: string) {
    try {
      await invoke("delete_presentation", { id });
      await loadRecent();
    } catch (e) {
      console.error("Failed to delete:", e);
    }
  }

  function goHome() {
    view = "home";
    currentPresentation = null;
    importProgress = null;
    importError = null;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (view !== "viewer" || !currentPresentation) return;
    if (e.key === "ArrowRight" || e.key === " ") {
      e.preventDefault();
      if (currentSlideIndex < currentPresentation.slides.length - 1) {
        currentSlideIndex++;
      }
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      if (currentSlideIndex > 0) {
        currentSlideIndex--;
      }
    } else if (e.key === "Escape") {
      goHome();
    }
  }

  function calculateScale(): number {
    const maxW = window.innerWidth - 80;
    const maxH = window.innerHeight - 200;
    return Math.min(maxW / 1920, maxH / 1080, 1);
  }

  onMount(() => {
    loadRecent();
    viewerScale = calculateScale();

    const unlisten = listen<ImportProgress>("import-progress", (event) => {
      importProgress = event.payload;
    });

    const handleResize = () => {
      viewerScale = calculateScale();
    };
    window.addEventListener("resize", handleResize);

    return () => {
      unlisten.then((fn) => fn());
      window.removeEventListener("resize", handleResize);
    };
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<main>
  <header>
    <h1>
      {#if view === "viewer" && currentPresentation}
        <button class="back-btn" onclick={goHome}>&larr;</button>
        {currentPresentation.title}
      {:else}
        Canva Importer
      {/if}
    </h1>
    {#if view === "viewer" && currentPresentation}
      <span class="slide-counter">
        {currentSlideIndex + 1} / {currentPresentation.slides.length}
      </span>
    {/if}
  </header>

  {#if view === "home"}
    <section class="welcome">
      <div class="url-input-group">
        <input
          type="text"
          bind:value={canvaUrl}
          placeholder="Paste Canva public view link..."
          class="url-input"
          onkeydown={(e) => {
            if (e.key === "Enter") startImport();
          }}
        />
        <button
          class="btn primary"
          onclick={startImport}
          disabled={!canvaUrl.trim()}
        >
          Import
        </button>
      </div>
      <p class="hint">
        Paste a public Canva presentation URL to import slides for offline
        viewing.
      </p>
    </section>

    {#if recentList.length > 0}
      <section class="recent">
        <h2>Recent Presentations</h2>
        <div class="recent-grid">
          {#each recentList as entry}
            <div class="recent-card">
              <button
                class="card-body"
                onclick={() => openPresentation(entry.id)}
              >
                <div class="card-thumb">
                  <span class="card-count">{entry.slide_count} slides</span>
                </div>
                <div class="card-info">
                  <span class="card-title">{entry.title}</span>
                  <span class="card-date">
                    {new Date(entry.imported_at).toLocaleDateString()}
                  </span>
                </div>
              </button>
              <button
                class="card-delete"
                onclick={() => deletePresentation(entry.id)}
                title="Delete"
              >
                &times;
              </button>
            </div>
          {/each}
        </div>
      </section>
    {/if}
  {:else if view === "importing"}
    <section class="import-progress">
      {#if importError}
        <div class="progress-error">
          <p>Import failed</p>
          <p class="error-detail">{importError}</p>
          <button class="btn" onclick={goHome}>Back</button>
        </div>
      {:else if importProgress}
        <div class="progress-indicator">
          <div class="progress-stage">{importProgress.stage}</div>
          <div class="progress-detail">{importProgress.detail}</div>
          <div class="progress-dots">
            <span
              class="dot"
              class:active={importProgress.stage === "Fetching"}
              class:done={["Parsing", "Downloading", "Complete"].includes(
                importProgress.stage
              )}
            ></span>
            <span
              class="dot"
              class:active={importProgress.stage === "Parsing"}
              class:done={["Downloading", "Complete"].includes(
                importProgress.stage
              )}
            ></span>
            <span
              class="dot"
              class:active={importProgress.stage === "Downloading"}
              class:done={importProgress.stage === "Complete"}
            ></span>
            <span
              class="dot"
              class:active={importProgress.stage === "Complete"}
            ></span>
          </div>
        </div>
      {/if}
    </section>
  {:else if view === "viewer" && currentPresentation && currentSlide}
    <section class="viewer">
      <div class="slide-area">
        <SlideRenderer
          slide={currentSlide}
          fonts={currentPresentation.fonts}
          scale={viewerScale}
        />
      </div>
      <div class="slide-nav">
        <SlideStrip
          slides={currentPresentation.slides}
          currentIndex={currentSlideIndex}
          onselect={(i) => (currentSlideIndex = i)}
        />
      </div>
    </section>
  {/if}
</main>

<style>
  :root {
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
    font-size: 13px;
    background-color: #1a1a2e;
    color: #e0e0e0;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    background: #16213e;
    border-bottom: 1px solid #0f3460;
    flex-shrink: 0;
  }

  h1 {
    font-size: 16px;
    margin: 0;
    font-weight: 600;
    color: #e94560;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  h2 {
    font-size: 13px;
    margin: 0 0 12px 0;
    color: #90a4ae;
  }

  .back-btn {
    background: none;
    border: 1px solid #0f3460;
    color: #4fc3f7;
    padding: 4px 10px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 14px;
    font-family: inherit;
  }
  .back-btn:hover {
    background: #0f3460;
  }

  .slide-counter {
    color: #546e7a;
    font-size: 14px;
  }

  .btn {
    padding: 6px 16px;
    border: 1px solid #0f3460;
    border-radius: 4px;
    background: #16213e;
    color: #e0e0e0;
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
  }
  .btn:hover {
    background: #0f3460;
  }
  .btn.primary {
    border-color: #4fc3f7;
    color: #4fc3f7;
  }
  .btn.primary:disabled {
    opacity: 0.4;
    cursor: default;
  }

  section {
    padding: 16px 20px;
  }

  .welcome {
    text-align: center;
    padding: 60px 20px 30px;
  }

  .url-input-group {
    display: flex;
    gap: 10px;
    max-width: 600px;
    margin: 0 auto 16px auto;
  }

  .url-input {
    flex: 1;
    padding: 8px 12px;
    border: 1px solid #0f3460;
    border-radius: 4px;
    background: #16213e;
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }
  .url-input:focus {
    border-color: #4fc3f7;
  }
  .url-input::placeholder {
    color: #546e7a;
  }

  .hint {
    font-size: 12px;
    color: #546e7a;
  }

  .recent {
    border-top: 1px solid #0f3460;
    flex: 1;
    overflow-y: auto;
  }

  .recent-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
  }

  .recent-card {
    position: relative;
    background: #16213e;
    border-radius: 6px;
    border: 1px solid #0f3460;
    overflow: hidden;
  }

  .card-body {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
    font-family: inherit;
  }
  .card-body:hover {
    background: #1c2a4a;
  }

  .card-thumb {
    height: 100px;
    background: #111;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .card-count {
    color: #546e7a;
    font-size: 12px;
  }

  .card-info {
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .card-title {
    font-size: 12px;
    font-weight: 600;
    color: #e0e0e0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-date {
    font-size: 11px;
    color: #546e7a;
  }

  .card-delete {
    position: absolute;
    top: 4px;
    right: 4px;
    background: rgba(0, 0, 0, 0.5);
    border: none;
    color: #e57373;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .recent-card:hover .card-delete {
    opacity: 1;
  }

  .import-progress {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .progress-indicator {
    text-align: center;
  }

  .progress-stage {
    font-size: 20px;
    font-weight: 600;
    color: #4fc3f7;
    margin-bottom: 8px;
  }

  .progress-detail {
    color: #90a4ae;
    margin-bottom: 20px;
  }

  .progress-dots {
    display: flex;
    gap: 12px;
    justify-content: center;
  }

  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #333;
    transition: background 0.3s;
  }
  .dot.active {
    background: #4fc3f7;
    animation: pulse 1s infinite;
  }
  .dot.done {
    background: #81c784;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.5;
    }
  }

  .progress-error {
    text-align: center;
    color: #e57373;
  }

  .error-detail {
    font-size: 12px;
    color: #90a4ae;
    max-width: 400px;
    margin: 8px auto 16px;
    word-break: break-word;
  }

  .viewer {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0;
  }

  .slide-area {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    overflow: hidden;
  }

  .slide-nav {
    padding: 8px 16px;
    border-top: 1px solid #0f3460;
    background: #16213e;
    flex-shrink: 0;
  }
</style>
