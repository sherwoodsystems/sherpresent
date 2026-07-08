// Reactive hook that wraps `invoke('get_discovered_peers')` and the
// `peers-updated` Tauri event, exposing a `$state` array of peers.
//
// Replaces the polling loop the Python web UI did every 5s — Tauri events
// deliver updates as soon as the mDNS daemon surfaces them.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { DiscoveredPeer } from './types';

export function usePeers() {
  let peers = $state<DiscoveredPeer[]>([]);
  let unlisten: UnlistenFn | null = null;

  async function start() {
    try {
      peers = await invoke<DiscoveredPeer[]>('get_discovered_peers');
    } catch (e) {
      console.error('get_discovered_peers failed', e);
    }
    unlisten = await listen<DiscoveredPeer[]>('peers-updated', (event) => {
      peers = event.payload;
    });
  }

  async function stop() {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  }

  return {
    get peers() {
      return peers;
    },
    start,
    stop,
  };
}