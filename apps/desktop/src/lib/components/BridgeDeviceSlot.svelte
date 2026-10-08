<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { BridgeDeviceSlot, BridgeApiResponse, BridgeConnectedDevices } from '$lib/types';
  import BridgeRegistration from './BridgeRegistration.svelte';

  let {
    slotName,
    device,
    connectedDevice,
    host,
    configPort,
    mode,
    validChannels,
    onchange
  }: {
    slotName: string;
    device: BridgeDeviceSlot | null;
    connectedDevice: BridgeConnectedDevices['devices'][number] | null;
    host: string;
    configPort: number;
    mode: string;
    validChannels: string[];
    onchange: () => void;
  } = $props();

  let registering = $state(false);
  let testing = $state(false);
  let message = $state<string | null>(null);

  async function unregister() {
    try {
      const result = await invoke<BridgeApiResponse>('bridge_unregister_device', {
        host,
        configPort,
        slot: slotName
      });
      message = result.message;
      onchange();
    } catch (e) {
      message = `Error: ${e}`;
    }
  }

  async function testDevice(command: string) {
    testing = true;
    try {
      await invoke<BridgeApiResponse>('bridge_test_device', {
        host,
        configPort,
        slot: slotName,
        command
      });
    } catch (e) {
      message = `Error: ${e}`;
    } finally {
      testing = false;
    }
  }

  function startRegistration() {
    registering = true;
  }

  function onRegistrationComplete() {
    registering = false;
    onchange();
  }

  function onRegistrationCancel() {
    registering = false;
  }
</script>

{#if registering}
  <BridgeRegistration
    targetSlot={slotName}
    {host}
    {configPort}
    {validChannels}
    oncomplete={onRegistrationComplete}
    oncancel={onRegistrationCancel}
  />
{:else}
  <div class="slot" class:empty={!device}>
    <div class="slot-columns">
      <div class="slot-left">
        <div class="slot-header">
          <span class="slot-name">{slotName.replace('_', ' ')}</span>
          {#if device}
            <span class="slot-label">{device.label}</span>
          {/if}
        </div>

        {#if device}
          <div class="slot-info">
            <span class="channel-badge">{device.channel}</span>
            {#if mode === 'broadcast'}
              <div class="test-buttons">
                <button class="btn-sm" onclick={() => testDevice('prev')} disabled={testing}
                  >Test Prev</button
                >
                <button class="btn-sm" onclick={() => testDevice('next')} disabled={testing}
                  >Test Next</button
                >
              </div>
            {/if}
            <button class="btn-sm btn-danger" onclick={unregister}>Unregister</button>
          </div>
        {:else}
          <button class="btn-register" onclick={startRegistration}>Register Device</button>
        {/if}
      </div>

      <div class="slot-right">
        {#if connectedDevice}
          <div class="connected-device-name">
            {connectedDevice.name}{connectedDevice.is_perfect_cue ? ' [Perfect Cue]' : ''}
          </div>
          <div class="connected-device-port">USB: {connectedDevice.usb_phys || 'unknown'}</div>
        {:else if device}
          <span class="connected-empty">Not connected</span>
        {:else}
          <span class="connected-empty">No device</span>
        {/if}
      </div>
    </div>

    {#if message}
      <p class="slot-message">{message}</p>
    {/if}
  </div>
{/if}

<style>
  .slot {
    padding: 0.75rem;
    background: #f5f5f5;
    border-radius: 8px;
    border: 1px solid #e0e0e0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .slot.empty {
    border-style: dashed;
  }

  .slot-columns {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
  }

  .slot-left {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .slot-right {
    display: flex;
    flex-direction: column;
    justify-content: center;
    font-size: 0.8rem;
    border-left: 1px solid #e0e0e0;
    padding-left: 0.75rem;
  }

  .connected-device-name {
    font-weight: 500;
    font-size: 0.8rem;
    color: var(--text);
  }

  .connected-device-port {
    font-size: 0.7rem;
    font-family: monospace;
    color: var(--text-muted);
    margin-top: 0.125rem;
  }

  .connected-empty {
    font-size: 0.75rem;
    color: var(--text-faint);
    font-style: italic;
  }

  .slot-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .slot-name {
    font-size: 0.8rem;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--text-secondary);
  }

  .slot-label {
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--text);
  }

  .slot-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .channel-badge {
    font-size: 0.75rem;
    background: #e3f2fd;
    color: #1565c0;
    padding: 0.125rem 0.5rem;
    border-radius: 4px;
    font-weight: 500;
  }

  .test-buttons {
    display: flex;
    gap: 0.25rem;
  }

  .btn-sm {
    padding: 0.25rem 0.5rem;
    font-size: 0.75rem;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: #fff;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.15s ease;
    color: var(--text);
  }
  .btn-sm:hover {
    background: #f0f0f0;
  }
  .btn-sm:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-danger {
    color: var(--danger);
    border-color: var(--danger-border);
    margin-left: auto;
  }
  .btn-danger:hover {
    background: var(--danger-bg);
  }

  .btn-register {
    padding: 0.5rem;
    background: #e8f5e9;
    color: var(--success);
    border: 1px solid #a5d6a7;
    border-radius: 6px;
    cursor: pointer;
    font-family: inherit;
    font-size: 0.85rem;
    font-weight: 500;
    transition: all 0.15s ease;
  }
  .btn-register:hover {
    background: #c8e6c9;
  }

  .slot-message {
    margin: 0;
    font-size: 0.75rem;
    color: #888;
  }
  @media (prefers-color-scheme: dark) {
    .slot {
      background: #333;
      border-color: #555;
    }

    .slot.empty {
      border-color: #555;
    }

    .slot-right {
      border-left-color: #555;
    }

    .channel-badge {
      background: #1a3a5c;
      color: #8fcfff;
    }

    .btn-sm {
      background: #444;
    }

    .btn-sm:hover {
      background: #555;
    }

    .btn-register {
      background: #1b3a1b;
      border-color: #2e5a2e;
    }

    .btn-register:hover {
      background: #2a4a2a;
    }
  }
</style>
