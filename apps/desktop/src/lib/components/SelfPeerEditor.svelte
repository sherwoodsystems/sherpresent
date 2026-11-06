<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import type { DiscoveredPeer } from '../types';
  import { appStore } from '$lib/state.svelte';

  let { peer, onSaved }: { peer: DiscoveredPeer; onSaved?: () => void } = $props();

  let editing = $state(false);
  let nameInput = $state('');

  function startEditing() {
    nameInput = peer.displayName || '';
    editing = true;
  }

  async function saveName() {
    const newName = nameInput.trim() || null;
    try {
      await appStore.setInstanceName(newName);
      editing = false;
      onSaved?.();
    } catch (e) {
      console.error('Failed to set instance name:', e);
    }
  }

  function cancelEditing() {
    editing = false;
    nameInput = '';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      saveName();
    } else if (e.key === 'Escape') {
      cancelEditing();
    }
  }
</script>

<div class="self-peer-editor">
  <div class="self-info">
    <span class="peer-id">#{peer.displayId}</span>
    {#if editing}
      <input
        type="text"
        class="input name-input"
        aria-label="Name"
        bind:value={nameInput}
        onkeydown={handleKeydown}
        placeholder="Enter name..."
        {@attach (el) => el.focus()}
      />
      <Button variant="primary" size="sm" onclick={saveName}>Save</Button>
      <Button size="sm" onclick={cancelEditing}>Cancel</Button>
    {:else}
      <span class="self-name">
        {peer.displayName || peer.host}
        <span class="you-badge">(You)</span>
      </span>
      <button
        type="button"
        class="btn-edit"
        onclick={startEditing}
        title="Rename"
        aria-label="Rename"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path>
          <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path>
        </svg>
      </button>
    {/if}
  </div>
  <span class="self-address">{peer.host}:{peer.port}</span>
</div>

<style>
  .self-peer-editor {
    display: flex;
    justify-content: space-between;
    align-items: center;
    box-sizing: border-box;
    min-height: var(--control-height);
    padding: 0.25rem 0.75rem;
    background: var(--info-bg);
    border-radius: var(--radius-control);
    border: 1px solid var(--info-border);
  }

  .self-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    min-width: 0;
  }

  .peer-id {
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--info-text);
    background: var(--info-border);
    padding: 0.125rem 0.375rem;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .self-name {
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--info-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .you-badge {
    font-size: 0.7rem;
    font-weight: 400;
    color: var(--link);
    margin-left: 0.25rem;
  }

  .self-address {
    font-size: 0.75rem;
    font-family: monospace;
    color: var(--link);
    flex-shrink: 0;
    margin-left: 0.5rem;
  }

  .btn-edit {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0.25rem;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .btn-edit:hover {
    background: var(--hover);
    color: var(--text);
  }

  .name-input {
    flex: 1;
    min-width: 100px;
    height: var(--control-height-sm);
    padding: 0 0.5rem;
  }
</style>
