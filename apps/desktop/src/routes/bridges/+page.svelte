<script lang="ts">
  import { page } from '$app/state';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { DiscoveredPeer } from '$lib/types';
  import { usePeers } from '$lib/usePeers.svelte';
  import BridgeDetail from '$lib/components/BridgeDetail.svelte';

  const peerState = usePeers(() => checkBridgeParam());

  let selectedBridgeId = $state<string | null>(null);

  let bridges = $derived(peerState.peers.filter((p) => p.version === 'bridge' && p.configPort));

  let selectedBridge = $derived(
    selectedBridgeId ? (bridges.find((b) => b.instanceId === selectedBridgeId) ?? null) : null
  );

  function checkBridgeParam() {
    if (selectedBridgeId) return;
    const bridgeParam = page.url.searchParams.get('bridge');
    if (bridgeParam) {
      const target = bridges.find((b) => b.instanceId === bridgeParam);
      if (target) selectedBridgeId = target.instanceId;
    }
  }

  function openBridge(bridge: DiscoveredPeer) {
    selectedBridgeId = bridge.instanceId;
  }

  function closeBridge() {
    selectedBridgeId = null;
  }

  function handleBridgeSave(savedName: string) {
    // Optimistically patch the peer's displayName so the card updates instantly
    const idx = peerState.peers.findIndex((p) => p.instanceId === selectedBridgeId);
    if (idx !== -1) {
      peerState.peers[idx] = { ...peerState.peers[idx], displayName: savedName };
      peerState.peers = [...peerState.peers];
    }
  }
</script>

<main>
  <header class="header">
    <h1>Bridges</h1>
    <p class="subtitle">Remote bridge configuration</p>
  </header>

  {#if bridges.length === 0}
    <div class="empty-state">
      <p>Searching for bridges on the network...</p>
      <p class="hint">
        Bridges announce themselves via mDNS. Make sure your bridge devices are running.
      </p>
    </div>
  {:else}
    <div class="bridge-grid">
      {#each bridges as bridge (bridge.instanceId)}
        <div class="bridge-card" role="group">
          <button class="bridge-card-main" onclick={() => openBridge(bridge)}>
            <div class="bridge-header">
              <span class="bridge-name">{bridge.displayName || bridge.host}</span>
              <span class="online-dot"></span>
            </div>
            <div class="bridge-meta">
              <span class="bridge-host">{bridge.host}</span>
            </div>
          </button>
          <button
            class="open-config-btn"
            onclick={() => openUrl(`http://${bridge.host}:${bridge.configPort}`)}
          >
            Open Config Page
          </button>
        </div>
      {/each}
    </div>
  {/if}
</main>

{#if selectedBridge}
  <BridgeDetail
    host={selectedBridge.host}
    configPort={selectedBridge.configPort!}
    bridgeName={selectedBridge.displayName || selectedBridge.host}
    onclose={closeBridge}
    onsave={handleBridgeSave}
  />
{/if}

<style>
  .header {
    text-align: center;
    margin-bottom: 1.5rem;
  }

  .header h1 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 700;
    color: var(--text);
  }

  .subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.875rem;
    color: var(--text-muted);
  }

  .empty-state {
    text-align: center;
    padding: 2rem;
    background: var(--surface);
    border-radius: 12px;
    box-shadow: var(--shadow-card);
  }

  .empty-state p {
    margin: 0.5rem 0;
    color: var(--text-secondary);
  }

  .empty-state .hint {
    font-size: 0.8rem;
    color: var(--text-faint);
  }

  .bridge-grid {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .bridge-card {
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--divider);
    border-radius: 12px;
    box-shadow: var(--shadow-card);
    overflow: hidden;
    transition: all 0.15s ease;
    color: var(--text);
  }

  .bridge-card:hover {
    border-color: var(--link);
    box-shadow: 0 2px 8px rgba(26, 115, 232, 0.15);
  }

  .bridge-card-main {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 1rem;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    font-size: inherit;
    color: inherit;
    width: 100%;
  }

  .bridge-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .bridge-name {
    font-weight: 600;
    font-size: 1rem;
  }

  .online-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #34a853;
    flex-shrink: 0;
  }

  .bridge-meta {
    display: flex;
    justify-content: space-between;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .open-config-btn {
    padding: 0.5rem 1rem;
    font-size: 0.75rem;
    font-family: inherit;
    font-weight: 500;
    color: var(--link);
    background: rgba(26, 115, 232, 0.04);
    border: none;
    border-top: 1px solid var(--divider);
    cursor: pointer;
    transition: background 0.15s ease;
    width: 100%;
    text-align: center;
  }
  .open-config-btn:hover {
    background: rgba(26, 115, 232, 0.1);
  }

  .bridge-host {
    font-family: monospace;
  }
  @media (prefers-color-scheme: dark) {
    .bridge-card:hover {
      box-shadow: 0 2px 8px rgba(106, 183, 255, 0.15);
    }

    .open-config-btn {
      background: rgba(106, 183, 255, 0.04);
    }

    .open-config-btn:hover {
      background: rgba(106, 183, 255, 0.1);
    }
  }
</style>
