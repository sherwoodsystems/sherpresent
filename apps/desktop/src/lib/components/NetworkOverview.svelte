<script lang="ts">
  import { goto } from '$app/navigation';
  import { usePeers } from '$lib/usePeers.svelte';
  import type { DiscoveredPeer } from '../types';
  import SelfPeerEditor from './SelfPeerEditor.svelte';
  import PeerList from './PeerList.svelte';

  const peerState = usePeers();

  let selfPeer = $derived(peerState.peers.find(p => p.isSelf));
  let otherPeers = $derived(peerState.peers.filter(p => !p.isSelf));

  function handlePeerClick(peer: DiscoveredPeer) {
    if (peer.version === 'bridge') {
      goto(`/bridges?bridge=${peer.instanceId}`);
    }
  }
</script>

<div class="network-overview">
  <h3 class="section-title">Network Overview</h3>

  {#if selfPeer}
    <SelfPeerEditor peer={selfPeer} />
  {/if}

  <PeerList
    peers={otherPeers}
    emptyMessage="No other peers found"
    onpeerclick={handlePeerClick}
  />
</div>

<style>
  .network-overview {
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

  @media (prefers-color-scheme: dark) {
    .section-title {
      color: #eee;
      border-bottom-color: #444;
    }
  }
</style>
