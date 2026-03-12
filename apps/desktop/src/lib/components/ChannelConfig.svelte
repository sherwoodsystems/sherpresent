<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import type { ChannelConfig, DiscoveredPeer, NetworkInterface } from '../types';
  import { VALID_CHANNELS, DEFAULT_BROADCAST_PORT } from '../types';
  import Select from './Select.svelte';

  interface Props {
    config: ChannelConfig;
    onchange: (config: ChannelConfig) => void;
  }

  let { config, onchange }: Props = $props();
  let discoveredPeers = $state<DiscoveredPeer[]>([]);
  let networkInterfaces = $state<NetworkInterface[]>([]);
  let unlistenPeers: UnlistenFn | null = null;

  onMount(async () => {
    // Listen for peer discovery events
    unlistenPeers = await listen<DiscoveredPeer[]>('channel-peers-updated', (event) => {
      discoveredPeers = event.payload;
    });

    // Initial fetch of discovered peers
    try {
      discoveredPeers = await invoke<DiscoveredPeer[]>('get_discovered_peers');
    } catch (e) {
      console.error('Failed to get discovered peers:', e);
    }

    // Fetch available network interfaces
    try {
      networkInterfaces = await invoke<NetworkInterface[]>('get_available_interfaces');
    } catch (e) {
      console.error('Failed to get network interfaces:', e);
    }
  });

  onDestroy(() => {
    unlistenPeers?.();
  });

  function updateChannelName(channelName: string) {
    onchange({ ...config, channelName });
  }

  function updateBroadcastPort(broadcastPort: number) {
    if (broadcastPort >= 1024 && broadcastPort <= 65535) {
      onchange({ ...config, broadcastPort });
    }
  }

  function updateNetworkInterface(value: string) {
    // "auto" means null (all interfaces)
    const networkInterface = value === 'auto' ? null : value;
    onchange({ ...config, networkInterface });
  }

  // Filter peers to those in our channel
  let channelPeers = $derived(
    config.channelName
      ? discoveredPeers.filter(p => p.channel === config.channelName && p.instanceId !== config.instanceId)
      : []
  );

  // Filter to non-loopback interfaces for the dropdown
  let availableInterfaces = $derived(
    networkInterfaces.filter(iface => !iface.isLoopback)
  );
</script>

<div class="channel-config">
  <h3 class="section-title">Channel Sync</h3>
  <p class="section-hint">Sync slides across multiple laptops</p>

  <div class="field">
    <label class="label" for="channel-name">Channel Name</label>
    <Select
      id="channel-name"
      value={config.channelName}
      onchange={updateChannelName}
    >
      {#each VALID_CHANNELS as channel (channel)}
        <option value={channel}>{channel}</option>
      {/each}
    </Select>
    <span class="hint">All instances with the same channel name will sync</span>
  </div>

  <div class="field">
    <label class="label" for="broadcast-port">Broadcast Port</label>
    <input
      id="broadcast-port"
      type="number"
      class="input port-input"
      min="1024"
      max="65535"
      value={config.broadcastPort}
      onchange={(e) => updateBroadcastPort(parseInt(e.currentTarget.value, 10))}
    />
    <span class="hint">UDP port to listen for commands (default: {DEFAULT_BROADCAST_PORT})</span>
  </div>

  <div class="field">
    <label class="label" for="network-interface">Network Interface</label>
    <Select
      id="network-interface"
      value={config.networkInterface ?? 'auto'}
      onchange={updateNetworkInterface}
    >
      <option value="auto">Auto (all interfaces)</option>
      {#each availableInterfaces as iface (iface.name)}
        <option value={iface.name}>{iface.name} ({iface.ip})</option>
      {/each}
    </Select>
    <span class="hint">Network to advertise mDNS service on</span>
  </div>

  {#if config.instanceId}
    <div class="instance-info">
      <span class="instance-label">Instance ID:</span>
      <code class="instance-id">{config.instanceId.slice(0, 8)}...</code>
    </div>
  {/if}

  {#if config.channelName}
    <div class="peers-section">
      <h4 class="peers-title">
        Discovered Peers
        {#if channelPeers.length > 0}
          <span class="peer-count">({channelPeers.length})</span>
        {/if}
      </h4>

      {#if channelPeers.length === 0}
        <p class="no-peers">No peers found in channel "{config.channelName}"</p>
      {:else}
        <ul class="peer-list">
          {#each channelPeers as peer (peer.instanceId)}
            <li class="peer-item">
              <div class="peer-info">
                <span class="peer-status"></span>
                <span class="peer-name">
                  {#if peer.displayName}
                    {peer.displayName}
                  {:else}
                    {peer.host}
                  {/if}
                </span>
                {#if peer.version === 'bridge'}
                  <span class="peer-badge bridge">Bridge</span>
                {:else if peer.version}
                  <span class="peer-badge instance">v{peer.version}</span>
                {/if}
              </div>
              <span class="peer-address">{peer.host}:{peer.port}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>

<style>
  .channel-config {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .section-title {
    font-size: 0.875rem;
    font-weight: 600;
    color: #333;
    margin: 0;
  }

  .section-hint {
    font-size: 0.75rem;
    color: #888;
    margin: 0.25rem 0 0.5rem 0;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid #eee;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #666;
  }

  .input {
    padding: 0.5rem 0.75rem;
    border: 2px solid #ddd;
    border-radius: 6px;
    background: #fff;
    font-size: 0.875rem;
  }

  .input:focus {
    outline: none;
    border-color: #007aff;
  }

  .port-input {
    width: 100px;
  }

  .hint {
    font-size: 0.7rem;
    color: #888;
    margin-top: 0.125rem;
  }

  .instance-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    background: #f5f5f5;
    border-radius: 6px;
  }

  .instance-label {
    font-size: 0.75rem;
    color: #666;
  }

  .instance-id {
    font-size: 0.75rem;
    font-family: monospace;
    color: #888;
  }

  .peers-section {
    margin-top: 0.5rem;
    padding-top: 0.5rem;
    border-top: 1px solid #eee;
  }

  .peers-title {
    font-size: 0.75rem;
    font-weight: 600;
    color: #666;
    margin: 0 0 0.5rem 0;
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .peer-count {
    font-weight: 400;
    color: #888;
  }

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
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem;
    background: #e8f4fd;
    border-radius: 6px;
    border: 1px solid #b3d9f7;
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
    .section-title {
      color: #eee;
    }

    .section-hint {
      color: #777;
      border-bottom-color: #444;
    }

    .label {
      color: #aaa;
    }

    .input {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .input:focus {
      border-color: #0a84ff;
    }

    .input::placeholder {
      color: #666;
    }

    .hint {
      color: #777;
    }

    .instance-info {
      background: #333;
    }

    .instance-label {
      color: #aaa;
    }

    .instance-id {
      color: #777;
    }

    .peers-section {
      border-top-color: #444;
    }

    .peers-title {
      color: #aaa;
    }

    .peer-count {
      color: #777;
    }

    .no-peers {
      background: #333;
      color: #777;
    }

    .peer-item {
      background: #1a3a5c;
      border-color: #2a5a8c;
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
