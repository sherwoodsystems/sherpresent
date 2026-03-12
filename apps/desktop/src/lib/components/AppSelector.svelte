<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { AppConfig, AdapterType } from '../types';

  interface Props {
    config: AppConfig;
    onchange: (adapter: AdapterType) => void;
  }

  let { config, onchange }: Props = $props();

  // Available adapters for this platform (loaded from Rust)
  let availableAdapters: [string, string][] = $state([]);

  onMount(async () => {
    try {
      availableAdapters = await invoke<[string, string][]>('get_adapters');
    } catch (e) {
      console.error('Failed to get adapters:', e);
      // Fallback to libreoffice only
      availableAdapters = [['libreoffice', 'LibreOffice Impress']];
    }
  });
</script>

<div class="app-selector">
  <label class="label">Application</label>
  <div class="toggle-group">
    {#each availableAdapters as [id, name]}
      <button
        class="toggle-btn"
        class:active={config.adapter === id}
        onclick={() => onchange(id as AdapterType)}
      >
        {name}
      </button>
    {/each}
  </div>
  {#if config.adapter === 'libreoffice'}
    <p class="hint">
      Enable remote control: Slide Show &gt; Slide Show Settings &gt; Enable remote control
    </p>
  {/if}
</div>

<style>
  .app-selector {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .label {
    font-size: 0.875rem;
    font-weight: 500;
    color: #666;
  }

  .toggle-group {
    display: flex;
    gap: 0.5rem;
  }

  .toggle-btn {
    flex: 1;
    padding: 0.75rem 1rem;
    border: 2px solid #ddd;
    border-radius: 8px;
    background: #fff;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }

  .toggle-btn:hover {
    border-color: #aaa;
  }

  .toggle-btn.active {
    border-color: #007aff;
    background: #007aff;
    color: white;
  }

  .hint {
    font-size: 0.75rem;
    color: #888;
    margin: 0.5rem 0 0 0;
    line-height: 1.4;
  }

  @media (prefers-color-scheme: dark) {
    .label {
      color: #aaa;
    }

    .toggle-btn {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .toggle-btn:hover {
      border-color: #777;
    }

    .toggle-btn.active {
      border-color: #0a84ff;
      background: #0a84ff;
      color: white;
    }
  }
</style>
