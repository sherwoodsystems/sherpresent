<script lang="ts">
  import { usePeers } from '$lib/usePeers.svelte';
  import type { DiscoveredPeer } from '../types';
  import SelfPeerEditor from './SelfPeerEditor.svelte';
  import { appStore } from '$lib/state.svelte';

  const peerState = usePeers();

  let selfPeer = $derived(peerState.peers.find(p => p.isSelf));

  function getPeerDisplayName(peer: DiscoveredPeer): string {
    return peer.displayName || peer.host;
  }

  async function refreshPeers() {
    peerState.peers = await appStore.getDiscoveredPeers();
  }
</script>

<div class="peer-discovery">
  <h3 class="section-title">Network Peers</h3>

  {#if peerState.peers.length === 0}
    <p class="no-peers">Searching for peers...</p>
  {:else}
    <ul class="peer-list">
      {#each peerState.peers as peer (peer.instanceId)}
        <li class="peer-item" class:is-self={peer.isSelf}>
          {#if peer.isSelf && selfPeer}
            <SelfPeerEditor peer={selfPeer} onSaved={refreshPeers} />
          {:else}
            <div class="peer-info">
              <span class="peer-id">#{peer.displayId}</span>
              <span class="peer-name">
                {getPeerDisplayName(peer)}
              </span>
            </div>
            <span class="peer-address">{peer.host}:{peer.port}</span>
          {/if}
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
    padding: 0;
    background: transparent;
    border: none;
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

  .peer-name {
    font-size: 0.875rem;
    font-weight: 500;
    color: #333;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .peer-address {
    font-size: 0.75rem;
    font-family: monospace;
    color: #888;
    flex-shrink: 0;
    margin-left: 0.5rem;
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
      background: transparent;
      border-color: transparent;
    }

    .peer-id {
      background: #444;
      color: #aaa;
    }

    .peer-name {
      color: #eee;
    }

    .peer-address {
      color: #777;
    }
  }
</style>
