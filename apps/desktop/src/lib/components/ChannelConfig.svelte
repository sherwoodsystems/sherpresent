<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { ChannelConfig, NetworkInterface } from '../types';
  import { VALID_CHANNELS, DEFAULT_BROADCAST_PORT } from '../types';
  import Select from './Select.svelte';

  interface Props {
    config: ChannelConfig;
    onchange: (config: ChannelConfig) => void;
  }

  let { config, onchange }: Props = $props();
  let networkInterfaces = $state<NetworkInterface[]>([]);

  onMount(async () => {
    try {
      networkInterfaces = await invoke<NetworkInterface[]>('get_available_interfaces');
    } catch (e) {
      console.error('Failed to get network interfaces:', e);
    }
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
    const networkInterface = value === 'auto' ? null : value;
    onchange({ ...config, networkInterface });
  }

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

  }
</style>
