<script lang="ts">
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
        class="name-input"
        bind:value={nameInput}
        onkeydown={handleKeydown}
        placeholder="Enter name..."
        autofocus
      />
      <button class="btn-save" onclick={saveName}>Save</button>
      <button class="btn-cancel" onclick={cancelEditing}>Cancel</button>
    {:else}
      <span class="self-name">
        {peer.displayName || peer.host}
        <span class="you-badge">(You)</span>
      </span>
      <button class="btn-edit" onclick={startEditing} title="Rename">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
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
    padding: 0.625rem 0.75rem;
    background: #e8f4fd;
    border-radius: 8px;
    border: 1px solid #b3d9f7;
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
    color: #1a56c4;
    background: #b3d9f7;
    padding: 0.125rem 0.375rem;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .self-name {
    font-size: 0.875rem;
    font-weight: 500;
    color: #1a56c4;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .you-badge {
    font-size: 0.7rem;
    font-weight: 400;
    color: #1a73e8;
    margin-left: 0.25rem;
  }

  .self-address {
    font-size: 0.75rem;
    font-family: monospace;
    color: #888;
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
    color: #666;
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .btn-edit:hover {
    background: rgba(0, 0, 0, 0.05);
    color: #333;
  }

  .name-input {
    flex: 1;
    min-width: 100px;
    padding: 0.25rem 0.5rem;
    font-size: 0.875rem;
    border: 1px solid #b3d9f7;
    border-radius: 4px;
    background: #fff;
    outline: none;
  }

  .name-input:focus {
    border-color: #1a73e8;
    box-shadow: 0 0 0 2px rgba(26, 115, 232, 0.2);
  }

  .btn-save,
  .btn-cancel {
    padding: 0.25rem 0.5rem;
    font-size: 0.75rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .btn-save {
    background: #1a73e8;
    color: #fff;
  }

  .btn-save:hover {
    background: #1557b0;
  }

  .btn-cancel {
    background: #e0e0e0;
    color: #666;
  }

  .btn-cancel:hover {
    background: #d0d0d0;
  }

  @media (prefers-color-scheme: dark) {
    .self-peer-editor {
      background: #1a3a5c;
      border-color: #2a5a8c;
    }

    .peer-id {
      background: #2a5a8c;
      color: #8fcfff;
    }

    .self-name {
      color: #8fcfff;
    }

    .you-badge {
      color: #6ab7ff;
    }

    .self-address {
      color: #777;
    }

    .btn-edit {
      color: #888;
    }

    .btn-edit:hover {
      background: rgba(255, 255, 255, 0.1);
      color: #eee;
    }

    .name-input {
      background: #222;
      border-color: #2a5a8c;
      color: #eee;
    }

    .name-input:focus {
      border-color: #6ab7ff;
      box-shadow: 0 0 0 2px rgba(106, 183, 255, 0.2);
    }

    .btn-save {
      background: #1a73e8;
    }

    .btn-save:hover {
      background: #2a8af8;
    }

    .btn-cancel {
      background: #444;
      color: #aaa;
    }

    .btn-cancel:hover {
      background: #555;
    }
  }
</style>
