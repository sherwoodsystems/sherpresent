<script lang="ts">
  import { page } from '$app/state';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { DiscoveredPeer } from '$lib/types';
  import { usePeers } from '$lib/usePeers.svelte';
  import BridgeDetail from '$lib/components/BridgeDetail.svelte';

  const peerState = usePeers(() => checkBridgeParam());

  let selectedBridgeId = $state<string | null>(null);

  let bridges = $derived(
    peerState.peers.filter(p => p.version === 'bridge' && p.configPort)
  );

  let selectedBridge = $derived(
    selectedBridgeId ? bridges.find(b => b.instanceId === selectedBridgeId) ?? null : null
  );

  function checkBridgeParam() {
    if (selectedBridgeId) return;
    const bridgeParam = page.url.searchParams.get('bridge');
    if (bridgeParam) {
      const target = bridges.find(b => b.instanceId === bridgeParam);
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
    const idx = peerState.peers.findIndex(p => p.instanceId === selectedBridgeId);
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
      <p class="hint">Bridges announce themselves via mDNS. Make sure your bridge devices are running.</p>
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
              <span class="bridge-channel">ch: {bridge.channel}</span>
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
    color: #333;
  }

  .subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.875rem;
    color: #888;
  }

  .empty-state {
    text-align: center;
    padding: 2rem;
    background: #fff;
    border-radius: 12px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  .empty-state p {
    margin: 0.5rem 0;
    color: #666;
  }

  .empty-state .hint {
    font-size: 0.8rem;
    color: #999;
  }

  .bridge-grid {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .bridge-card {
    display: flex;
    flex-direction: column;
    background: #fff;
    border: 1px solid #e0e0e0;
    border-radius: 12px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    overflow: hidden;
    transition: all 0.15s ease;
  }

  .bridge-card:hover {
    border-color: #1a73e8;
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
    color: #888;
  }

  .open-config-btn {
    padding: 0.5rem 1rem;
    font-size: 0.75rem;
    font-family: inherit;
    font-weight: 500;
    color: #1a73e8;
    background: rgba(26, 115, 232, 0.04);
    border: none;
    border-top: 1px solid #e0e0e0;
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
    .header h1 { color: #eee; }
    .subtitle { color: #777; }

    .empty-state {
      background: #2a2a2a;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
    }
    .empty-state p { color: #aaa; }
    .empty-state .hint { color: #666; }

    .bridge-card {
      background: #2a2a2a;
      border-color: #444;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
      color: #eee;
    }
    .bridge-card:hover {
      border-color: #6ab7ff;
      box-shadow: 0 2px 8px rgba(106, 183, 255, 0.15);
    }
    .bridge-meta { color: #777; }
    .open-config-btn {
      color: #6ab7ff;
      background: rgba(106, 183, 255, 0.04);
      border-top-color: #444;
    }
    .open-config-btn:hover {
      background: rgba(106, 183, 255, 0.1);
    }
  }
</style>
