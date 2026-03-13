<script lang="ts">
  import type { LiveStatus } from '../types';

  interface Props {
    status: LiveStatus | null;
    adapter: string;
    onprev?: () => void;
    onnext?: () => void;
  }

  let { status, adapter, onprev, onnext }: Props = $props();

  const supportsZoom = $derived(adapter === 'powerpoint');
</script>

<div class="status-display">
  <h3 class="section-title">Live Status</h3>

  {#if !status}
    <p class="placeholder">Waiting for status...</p>
  {:else if !status.is_open}
    <p class="placeholder">Presentation not open</p>
  {:else if !status.is_presenting}
    <p class="placeholder">Not presenting</p>
  {:else}
    <div class="status-content">
      <div class="status-grid">
        <div class="status-item">
          <span class="status-label">Slide</span>
          <span class="status-value">
            {status.current_slide} / {status.total_slides}
          </span>
        </div>

        <div class="status-item">
          <span class="status-label">Status</span>
          <span class="status-value presenting">
            Presenting
          </span>
        </div>

        {#if supportsZoom && status.zoom_level !== null}
          <div class="status-item">
            <span class="status-label">Zoom</span>
            <span class="status-value">{status.zoom_level}%</span>
          </div>
        {/if}
      </div>

      <div class="nav-buttons">
        <button class="nav-btn" onclick={onprev} disabled={!status.current_slide || status.current_slide <= 1}>
          ← Prev
        </button>
        <button class="nav-btn" onclick={onnext} disabled={!status.current_slide || status.current_slide >= status.total_slides}>
          Next →
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .status-display {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .section-title {
    font-size: 0.875rem;
    font-weight: 600;
    color: #333;
    margin: 0;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid #eee;
    text-align: center;
  }

  .placeholder {
    color: #888;
    font-size: 0.875rem;
    font-style: italic;
    margin: 0;
    padding: 1rem 0;
    text-align: center;
  }

  .status-content {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2rem;
    flex-wrap: wrap;
  }

  .status-grid {
    display: flex;
    gap: 1.5rem;
    flex-wrap: wrap;
  }

  .nav-buttons {
    display: flex;
    gap: 0.5rem;
  }

  .nav-btn {
    padding: 0.5rem 1rem;
    border: 1px solid #ddd;
    border-radius: 8px;
    background: #f5f5f5;
    color: #333;
    font-size: 0.875rem;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .nav-btn:hover:not(:disabled) {
    background: #e8e8e8;
    border-color: #ccc;
  }

  .nav-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .status-item {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .status-label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #666;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-value {
    font-size: 1.25rem;
    font-weight: 600;
    color: #333;
  }

  .status-value.presenting {
    color: #34c759;
  }

  @media (prefers-color-scheme: dark) {
    .section-title {
      color: #eee;
      border-bottom-color: #444;
    }

    .placeholder {
      color: #777;
    }

    .status-label {
      color: #aaa;
    }

    .status-value {
      color: #eee;
    }

    .status-value.presenting {
      color: #30d158;
    }

    .nav-btn {
      background: #3a3a3a;
      border-color: #555;
      color: #eee;
    }

    .nav-btn:hover:not(:disabled) {
      background: #4a4a4a;
      border-color: #666;
    }
  }
</style>
