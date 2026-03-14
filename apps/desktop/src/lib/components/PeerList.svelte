<script lang="ts">
  import type { DiscoveredPeer } from '../types';

  interface Props {
    peers: DiscoveredPeer[];
    emptyMessage?: string;
    onpeerclick?: (peer: DiscoveredPeer) => void;
  }

  let { peers, emptyMessage = 'No peers found', onpeerclick }: Props = $props();
  let clickable = $derived(!!onpeerclick);
</script>

{#if peers.length === 0}
  <p class="no-peers">{emptyMessage}</p>
{:else}
  <ul class="peer-list">
    {#each peers as peer (peer.instanceId)}
      <li class="peer-item" class:clickable>
        {#if clickable}
          <button class="peer-button" onclick={() => onpeerclick?.(peer)}>
            <div class="peer-info">
              <span class="peer-status"></span>
              <span class="peer-name">
                {peer.displayName || peer.host}
              </span>
              {#if peer.version === 'bridge'}
                <span class="peer-badge bridge">Bridge</span>
              {:else if peer.version}
                <span class="peer-badge instance">v{peer.version}</span>
              {/if}
            </div>
            <span class="peer-address">{peer.host}:{peer.port}</span>
          </button>
        {:else}
          <div class="peer-content">
            <div class="peer-info">
              <span class="peer-status"></span>
              <span class="peer-name">
                {peer.displayName || peer.host}
              </span>
              {#if peer.version === 'bridge'}
                <span class="peer-badge bridge">Bridge</span>
              {:else if peer.version}
                <span class="peer-badge instance">v{peer.version}</span>
              {/if}
            </div>
            <span class="peer-address">{peer.host}:{peer.port}</span>
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .no-peers {
    font-size: 0.75rem;
    color: #888;
    font-style: italic;
    margin: 0;
    padding: 0.5rem;
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
    background: #e8f4fd;
    border-radius: 6px;
    border: 1px solid #b3d9f7;
  }

  .peer-button,
  .peer-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem;
    width: 100%;
  }

  .peer-button {
    background: none;
    border: none;
    font: inherit;
    color: inherit;
    cursor: pointer;
    border-radius: 6px;
    transition: background 0.15s ease;
  }

  .peer-button:hover {
    background: rgba(0, 0, 0, 0.05);
  }

  .peer-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .peer-status {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #34c759;
    box-shadow: 0 0 4px rgba(52, 199, 89, 0.5);
    flex-shrink: 0;
  }

  .peer-name {
    font-size: 0.875rem;
    font-weight: 500;
    color: #1a56c4;
  }

  .peer-badge {
    font-size: 0.625rem;
    font-weight: 600;
    padding: 0.125rem 0.375rem;
    border-radius: 4px;
    text-transform: uppercase;
  }

  .peer-badge.bridge {
    background: #ff9500;
    color: white;
  }

  .peer-badge.instance {
    background: #007aff;
    color: white;
  }

  .peer-address {
    font-size: 0.75rem;
    font-family: monospace;
    color: #1a73e8;
  }

  @media (prefers-color-scheme: dark) {
    .no-peers {
      background: #333;
      color: #777;
    }

    .peer-item {
      background: #1a3a5c;
      border-color: #2a5a8c;
    }

    .peer-button:hover {
      background: rgba(255, 255, 255, 0.05);
    }

    .peer-status {
      background: #30d158;
      box-shadow: 0 0 4px rgba(48, 209, 88, 0.5);
    }

    .peer-name {
      color: #8fcfff;
    }

    .peer-badge.bridge {
      background: #ff9f0a;
    }

    .peer-badge.instance {
      background: #0a84ff;
    }

    .peer-address {
      color: #6ab7ff;
    }
  }
</style>
