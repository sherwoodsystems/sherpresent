import { onMount, onDestroy } from 'svelte';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { DiscoveredPeer } from './types';
import { appStore } from '$lib/state.svelte';

/**
 * Reactive peer discovery state. Sets up the `peers-updated` event listener
 * and fetches initial peers on mount. Cleans up on destroy.
 *
 * @param onUpdate Optional callback invoked after each peers update.
 */
export function usePeers(onUpdate?: () => void) {
  let peers = $state<DiscoveredPeer[]>([]);
  let unlistenPeers: UnlistenFn | null = null;

  onMount(async () => {
    unlistenPeers = await listen<DiscoveredPeer[]>('peers-updated', (event) => {
      peers = event.payload;
      onUpdate?.();
    });
    peers = await appStore.getDiscoveredPeers();
    onUpdate?.();
  });

  onDestroy(() => {
    unlistenPeers?.();
  });

  return {
    get peers() { return peers; },
    set peers(v: DiscoveredPeer[]) { peers = v; },
  };
}
