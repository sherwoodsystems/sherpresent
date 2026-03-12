<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import type { DiscoveredPeer } from '../types';
  import { appStore } from '$lib/state.svelte';

  let peers = $state<DiscoveredPeer[]>([]);
  let editingName = $state(false);
  let nameInput = $state('');
  let unlistenPeers: UnlistenFn | null = null;

  onMount(async () => {
    // Listen for peer discovery events
    unlistenPeers = await listen<DiscoveredPeer[]>('peers-updated', (event) => {
      peers = event.payload;
    });

    // Initial fetch of discovered peers
    peers = await appStore.getDiscoveredPeers();
  });

  onDestroy(() => {
    unlistenPeers?.();
  });

  // Find our own peer in the list
  let selfPeer = $derived(peers.find(p => p.isSelf));

  // Start editing the name
  function startEditing() {
    nameInput = selfPeer?.displayName || '';
    editingName = true;
  }

  // Save the new name
  async function saveName() {
    const newName = nameInput.trim() || null;
    try {
      await appStore.setInstanceName(newName);
      editingName = false;
      // Refresh peers to show updated name
      peers = await appStore.getDiscoveredPeers();
    } catch (e) {
      console.error('Failed to set instance name:', e);
    }
  }

  // Cancel editing
  function cancelEditing() {
    editingName = false;
    nameInput = '';
  }

  // Handle keydown in input
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      saveName();
    } else if (e.key === 'Escape') {
      cancelEditing();
    }
  }

  // Get display name for a peer
  function getPeerDisplayName(peer: DiscoveredPeer): string {
    return peer.displayName || peer.host;
  }
</script>

<div class="peer-discovery">
  <h3 class="section-title">Network Peers</h3>

  {#if peers.length === 0}
    <p class="no-peers">Searching for peers...</p>
  {:else}
    <ul class="peer-list">
      {#each peers as peer (peer.instanceId)}
        <li class="peer-item" class:is-self={peer.isSelf}>
          <div class="peer-info">
            <span class="peer-id">#{peer.displayId}</span>
            {#if peer.isSelf && editingName}
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
              <span class="peer-name">
                {getPeerDisplayName(peer)}
                {#if peer.isSelf}
                  <span class="you-badge">(You)</span>
                {/if}
              </span>
              {#if peer.isSelf}
                <button class="btn-edit" onclick={startEditing} title="Rename">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path>
                    <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path>
                  </svg>
                </button>
              {/if}
            {/if}
          </div>
          <span class="peer-address">{peer.host}:{peer.port}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .peer-discovery {
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
  }

  .no-peers {
    font-size: 0.875rem;
    color: #888;
    font-style: italic;
    margin: 0;
    padding: 1rem;
    background: #f9f9f9;
    border-radius: 6px;
    text-align: center;
  }

  .peer-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .peer-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.625rem 0.75rem;
    background: #f5f5f5;
    border-radius: 8px;
    border: 1px solid #e0e0e0;
    transition: all 0.15s ease;
  }

  .peer-item.is-self {
    background: #e8f4fd;
    border-color: #b3d9f7;
  }

  .peer-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    min-width: 0;
  }

  .peer-id {
    font-size: 0.75rem;
    font-weight: 700;
    color: #666;
    background: #e0e0e0;
    padding: 0.125rem 0.375rem;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .is-self .peer-id {
    background: #b3d9f7;
    color: #1a56c4;
  }

  .peer-name {
    font-size: 0.875rem;
    font-weight: 500;
    color: #333;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .is-self .peer-name {
    color: #1a56c4;
  }

  .you-badge {
    font-size: 0.7rem;
    font-weight: 400;
    color: #666;
    margin-left: 0.25rem;
  }

  .is-self .you-badge {
    color: #1a73e8;
  }

  .peer-address {
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
    .section-title {
      color: #eee;
      border-bottom-color: #444;
    }

    .no-peers {
      background: #333;
      color: #777;
    }

    .peer-item {
      background: #333;
      border-color: #444;
    }

    .peer-item.is-self {
      background: #1a3a5c;
      border-color: #2a5a8c;
    }

    .peer-id {
      background: #444;
      color: #aaa;
    }

    .is-self .peer-id {
      background: #2a5a8c;
      color: #8fcfff;
    }

    .peer-name {
      color: #eee;
    }

    .is-self .peer-name {
      color: #8fcfff;
    }

    .you-badge {
      color: #888;
    }

    .is-self .you-badge {
      color: #6ab7ff;
    }

    .peer-address {
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
