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

<div class="toggle-group">
  {#each visibleAdapters as [id, name] (id)}
    <button
      type="button"
      class="toggle-btn"
      class:active={config.adapter === id}
      aria-pressed={config.adapter === id}
      onclick={() => onchange(id as AdapterType)}
    >
      {name}
    </button>
  {/each}
</div>
{#if config.adapter === 'libreoffice'}
  <p class="hint tip">
    Enable remote control: Slide Show &gt; Slide Show Settings &gt; Enable remote control. When
    prompted for a PIN, enter <strong>1234</strong>.
  </p>
{:else if config.adapter === 'canva'}
  <p class="hint tip">Start presenting in Canva, then share the remote control link.</p>
{/if}

<style>
  /* Wide enough for the longest name, so it wraps rather than clips. */
  .toggle-group {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(6.5rem, 1fr));
    gap: 0.5rem;
  }

  .toggle-btn {
    height: var(--control-height);
    padding: 0 0.5rem;
    border: 2px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--input-bg);
    font: inherit;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition:
      background 0.15s,
      border-color 0.15s;
    color: var(--text);
  }

  .toggle-btn:hover {
    border-color: var(--border-strong);
  }

  .toggle-btn.active {
    border-color: var(--accent);
    background: var(--accent);
    color: #fff;
  }

  .tip {
    margin: 0;
    line-height: 1.4;
  }
</style>
