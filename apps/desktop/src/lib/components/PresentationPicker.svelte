<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  interface Props {
    adapter: string;
    selectedPresentation: string;
    onselect: (name: string) => void;
  }

  let { adapter, selectedPresentation, onselect }: Props = $props();

  let presentations = $state<string[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function refreshPresentations() {
    loading = true;
    error = null;
    try {
      presentations = await invoke('get_open_presentations', { adapter });
    } catch (e) {
      error = e as string;
      presentations = [];
    } finally {
      loading = false;
    }
  }

  // Refresh when adapter changes
  $effect(() => {
    adapter; // Track dependency
    refreshPresentations();
  });
</script>

<div class="presentation-picker">
  <label class="label">Presentation</label>
  <div class="picker-row">
    <select
      class="select"
      value={selectedPresentation}
      onchange={(e) => onselect(e.currentTarget.value)}
      disabled={loading}
    >
      <option value="">Select a presentation...</option>
      {#each presentations as pres}
        <option value={pres}>{pres}</option>
      {/each}
    </select>
    <button
      class="refresh-btn"
      onclick={refreshPresentations}
      disabled={loading}
      title="Refresh presentation list"
    >
      {#if loading}
        ...
      {:else}
        ↻
      {/if}
    </button>
  </div>
  {#if error}
    <p class="error">{error}</p>
  {/if}
  {#if !loading && presentations.length === 0 && !error}
    <p class="hint">No presentations open in {adapter === 'powerpoint' ? 'PowerPoint' : 'Keynote'}</p>
  {/if}
</div>

<style>
  .presentation-picker {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .label {
    font-size: 0.875rem;
    font-weight: 500;
    color: #666;
  }

  .picker-row {
    display: flex;
    gap: 0.5rem;
  }

  .select {
    flex: 1;
    padding: 0.75rem;
    border: 2px solid #ddd;
    border-radius: 8px;
    background: #fff;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .refresh-btn {
    padding: 0.75rem 1rem;
    border: 2px solid #ddd;
    border-radius: 8px;
    background: #fff;
    font-size: 1rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .refresh-btn:hover:not(:disabled) {
    border-color: #007aff;
    color: #007aff;
  }

  .refresh-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .error {
    color: #ff3b30;
    font-size: 0.75rem;
    margin: 0;
  }

  .hint {
    color: #888;
    font-size: 0.75rem;
    margin: 0;
    font-style: italic;
  }

  @media (prefers-color-scheme: dark) {
    .label {
      color: #aaa;
    }

    .select,
    .refresh-btn {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .select option {
      background: #333;
      color: #eee;
    }

    .refresh-btn:hover:not(:disabled) {
      border-color: #0a84ff;
      color: #0a84ff;
    }

    .hint {
      color: #777;
    }
  }
</style>
