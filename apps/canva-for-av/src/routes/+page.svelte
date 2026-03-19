<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  interface Stats {
    totalEntries: number;
    categories: Record<string, number>;
    logFile: string;
    canvaOpen?: boolean;
  }

  interface CaptureFile {
    name: string;
    path: string;
    size: number;
  }

  let stats = $state<Stats>({ totalEntries: 0, categories: {}, logFile: "" });
  let canvaOpen = $state(false);
  let logFilePath = $state("");
  let captures = $state<CaptureFile[]>([]);
  let logDir = $state("");

  let sortedCategories = $derived(
    Object.entries(stats.categories).sort((a, b) => b[1] - a[1])
  );

  async function openCanva() {
    try {
      logFilePath = await invoke<string>("open_canva");
      canvaOpen = true;
      stats = { totalEntries: 0, categories: {}, logFile: logFilePath };
    } catch (e) {
      console.error("Failed to open Canva:", e);
    }
  }

  async function closeCanva() {
    try {
      await invoke("close_canva");
      canvaOpen = false;
      await refreshCaptures();
    } catch (e) {
      console.error("Failed to close Canva:", e);
    }
  }

  async function refreshCaptures() {
    captures = await invoke<CaptureFile[]>("list_captures");
  }

  async function refreshStats() {
    const s = await invoke<Stats>("get_stats");
    stats = s;
    canvaOpen = s.canvaOpen ?? false;
    logFilePath = s.logFile;
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return bytes + " B";
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
    return (bytes / (1024 * 1024)).toFixed(1) + " MB";
  }

  function categoryColor(cat: string): string {
    if (cat.startsWith("WS")) return "#4fc3f7";
    if (cat.startsWith("FETCH")) return "#81c784";
    if (cat.startsWith("XHR")) return "#aed581";
    if (cat.startsWith("CONSOLE_ERROR")) return "#e57373";
    if (cat.startsWith("CONSOLE_WARN")) return "#ffb74d";
    if (cat.startsWith("CONSOLE")) return "#bdbdbd";
    if (cat.startsWith("ANIMATION") || cat.startsWith("TRANSITION")) return "#ce93d8";
    if (cat.startsWith("DOM")) return "#ffab91";
    if (cat.startsWith("CANVAS")) return "#f06292";
    if (cat.startsWith("STATE")) return "#fff176";
    return "#90a4ae";
  }

  onMount(async () => {
    logDir = await invoke<string>("get_log_dir");
    await refreshStats();
    await refreshCaptures();

    const unlisten = listen<Stats>("canva-stats", (event) => {
      stats = event.payload;
    });

    // Poll stats every 2s as a fallback (the event only fires every 50 entries)
    const interval = setInterval(refreshStats, 2000);

    return () => {
      unlisten.then((fn) => fn());
      clearInterval(interval);
    };
  });
</script>

<main>
  <header>
    <h1>Canva Analyzer</h1>
    <div class="controls">
      {#if canvaOpen}
        <button class="btn close" onclick={closeCanva}>Close Canva</button>
      {:else}
        <button class="btn open" onclick={openCanva}>Open Canva</button>
      {/if}
    </div>
  </header>

  {#if canvaOpen}
    <section class="stats-panel">
      <div class="stat-big">
        <span class="stat-number">{stats.totalEntries.toLocaleString()}</span>
        <span class="stat-label">events captured</span>
      </div>

      <div class="stat-file">
        Writing to: <code>{logFilePath}</code>
      </div>

      {#if sortedCategories.length > 0}
        <div class="category-grid">
          {#each sortedCategories as [cat, count]}
            <div class="category-row">
              <span class="cat-name" style="color: {categoryColor(cat)}">{cat}</span>
              <span class="cat-bar-wrap">
                <span
                  class="cat-bar"
                  style="width: {Math.min(100, (count / stats.totalEntries) * 100)}%; background: {categoryColor(cat)}"
                ></span>
              </span>
              <span class="cat-count">{count.toLocaleString()}</span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="hint">Waiting for events... browse Canva, open a presentation, hit Present.</p>
      {/if}
    </section>
  {:else}
    <section class="welcome">
      <p>Click <strong>Open Canva</strong> to launch the instrumented browser.</p>
      <p class="hint">All network traffic, console output, animations, DOM mutations, and canvas usage will be captured to a JSONL file for offline analysis.</p>
    </section>
  {/if}

  <section class="captures">
    <h2>
      Past Captures
      <button class="btn-small" onclick={refreshCaptures}>Refresh</button>
    </h2>
    {#if logDir}
      <p class="hint">Stored in: <code>{logDir}</code></p>
    {/if}
    {#if captures.length === 0}
      <p class="hint">No captures yet.</p>
    {:else}
      <div class="capture-list">
        {#each captures as cap}
          <div class="capture-row">
            <span class="capture-name">{cap.name}</span>
            <span class="capture-size">{formatBytes(cap.size)}</span>
          </div>
        {/each}
      </div>
    {/if}
  </section>
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
    padding: 0;
    margin: 0;
    overflow-y: auto;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    background: #16213e;
    border-bottom: 1px solid #0f3460;
  }

  h1 {
    font-size: 16px;
    margin: 0;
    font-weight: 600;
    color: #e94560;
  }

  h2 {
    font-size: 13px;
    margin: 0 0 8px 0;
    color: #90a4ae;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .controls { display: flex; gap: 10px; }

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
  .btn:hover { background: #0f3460; }
  .btn.open { border-color: #4fc3f7; color: #4fc3f7; }
  .btn.close { border-color: #e57373; color: #e57373; }

  .btn-small {
    padding: 2px 8px;
    border: 1px solid #333;
    border-radius: 3px;
    background: transparent;
    color: #546e7a;
    cursor: pointer;
    font-size: 11px;
    font-family: inherit;
  }
  .btn-small:hover { color: #90a4ae; }

  section { padding: 16px 20px; }

  .stats-panel { border-bottom: 1px solid #0f3460; }

  .stat-big {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 8px;
  }
  .stat-number { font-size: 36px; font-weight: 700; color: #4fc3f7; }
  .stat-label { font-size: 14px; color: #546e7a; }

  .stat-file { font-size: 11px; color: #546e7a; margin-bottom: 16px; }
  .stat-file code { color: #90a4ae; }

  .category-grid { display: flex; flex-direction: column; gap: 3px; }

  .category-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .cat-name {
    font-size: 11px;
    font-weight: 600;
    min-width: 160px;
    text-align: right;
  }

  .cat-bar-wrap {
    flex: 1;
    height: 14px;
    background: #111;
    border-radius: 2px;
    overflow: hidden;
  }

  .cat-bar {
    display: block;
    height: 100%;
    border-radius: 2px;
    transition: width 0.3s ease;
    opacity: 0.7;
  }

  .cat-count {
    font-size: 11px;
    color: #90a4ae;
    min-width: 60px;
    text-align: right;
  }

  .welcome {
    text-align: center;
    padding: 60px 20px;
  }
  .welcome p { margin: 8px 0; }

  .hint { font-size: 12px; color: #546e7a; }
  .hint code { color: #90a4ae; }

  .captures { border-top: 1px solid #0f3460; }

  .capture-list { display: flex; flex-direction: column; gap: 2px; }

  .capture-row {
    display: flex;
    justify-content: space-between;
    padding: 4px 8px;
    background: #16213e;
    border-radius: 3px;
    font-size: 12px;
  }
  .capture-name { color: #b0bec5; }
  .capture-size { color: #546e7a; }
</style>
