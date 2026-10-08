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

  // LibreOffice isn't in use right now, so its button is hidden unless a saved
  // config already selects it (the non-macOS default), so nobody is stranded.
  const visibleAdapters = $derived(
    availableAdapters.filter(([id]) => id !== 'libreoffice' || config.adapter === id)
  );

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
  <span class="label">Application</span>
  <div class="toggle-group">
    {#each visibleAdapters as [id, name] (id)}
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
      Enable remote control: Slide Show &gt; Slide Show Settings &gt; Enable remote control. When
      prompted for a PIN, enter <strong>1234</strong>.
    </p>
  {:else if config.adapter === 'canva'}
    <p class="hint">Start presenting in Canva, then share the remote control link.</p>
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
  }

  .toggle-group {
    display: flex;
    gap: 0.5rem;
  }

  .toggle-btn {
    flex: 1;
    padding: 0.75rem 1rem;
    border: 2px solid var(--border);
    border-radius: 8px;
    background: var(--input-bg);
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
    color: var(--text);
  }

  .toggle-btn:hover {
    border-color: #aaa;
  }

  .toggle-btn.active {
    border-color: var(--accent);
    background: var(--accent);
    color: white;
  }

  .hint {
    font-size: 0.75rem;
    color: #888;
    margin: 0.5rem 0 0 0;
    line-height: 1.4;
  }
  @media (prefers-color-scheme: dark) {
    .toggle-btn:hover {
      border-color: #777;
    }

    .toggle-btn.active {
      color: white;
    }
  }
</style>
