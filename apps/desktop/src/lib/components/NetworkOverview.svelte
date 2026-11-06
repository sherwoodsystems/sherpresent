<script lang="ts">
  import { usePeers } from '$lib/usePeers.svelte';
  import SelfPeerEditor from './SelfPeerEditor.svelte';
  import PeerList from './PeerList.svelte';

  const peerState = usePeers();

  let selfPeer = $derived(peerState.peers.find((p) => p.isSelf));
  let otherPeers = $derived(peerState.peers.filter((p) => !p.isSelf));
</script>

<div class="network-overview">
  {#if selfPeer}
    <SelfPeerEditor peer={selfPeer} />
  {/if}

  <PeerList peers={otherPeers} emptyMessage="No other peers found" />
</div>

<style>
  .network-overview {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
</style>
