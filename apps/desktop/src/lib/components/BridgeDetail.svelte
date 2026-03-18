<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type {
    BridgeGlobalConfig,
    BridgeStatus,
    BridgeRegisteredDevices,
    BridgeConnectedDevices,
    BridgeSatelliteStatus,
    BridgeApiResponse,
    SaveGlobalConfigRequest,
  } from '$lib/types';
  import BridgeDeviceSlot from './BridgeDeviceSlot.svelte';

  let {
    host,
    configPort,
    bridgeName,
    onclose,
    onsave,
  }: {
    host: string;
    configPort: number;
    bridgeName: string;
    onclose: () => void;
    onsave?: (savedName: string) => void;
  } = $props();

  let status = $state<BridgeStatus | null>(null);
  let config = $state<BridgeGlobalConfig | null>(null);
  let registeredDevices = $state<BridgeRegisteredDevices | null>(null);
  let connectedDevices = $state<BridgeConnectedDevices | null>(null);
  let satelliteStatus = $state<BridgeSatelliteStatus | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let shuttingDown = $state(false);
  let saveMessage = $state<string | null>(null);
  let errorCount = $state(0);
  let offline = $state(false);

  // Editable form state
  let editMode = $state('');
  let editBroadcastPort = $state(9002);
  let editFeedbackPort = $state(9002);
  let editLogLevel = $state('INFO');
  let editBridgeName = $state('');
  let editSatelliteHost = $state('');
  let editSatellitePort = $state(16622);

  let pollTimer: ReturnType<typeof setInterval> | null = null;

  onMount(async () => {
    await fetchAll();
    loading = false;
    pollTimer = setInterval(pollStatus, 3000);
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
  });

  async function fetchAll() {
    try {
      const [s, c, d, cd, sat] = await Promise.all([
        invoke<BridgeStatus>('bridge_get_status', { host, configPort }),
        invoke<BridgeGlobalConfig>('bridge_get_config', { host, configPort }),
        invoke<BridgeRegisteredDevices>('bridge_get_registered_devices', { host, configPort }),
        invoke<BridgeConnectedDevices>('bridge_get_devices', { host, configPort }),
        invoke<BridgeSatelliteStatus>('bridge_get_satellite_status', { host, configPort }),
      ]);
      status = s;
      config = c;
      registeredDevices = d;
      connectedDevices = cd;
      satelliteStatus = sat;
      errorCount = 0;
      offline = false;
      syncFormFromConfig(c);
    } catch (e) {
      handleError();
    }
  }

  function syncFormFromConfig(c: BridgeGlobalConfig) {
    editMode = c.mode;
    editBroadcastPort = c.broadcast_port;
    editFeedbackPort = c.feedback_port;
    editLogLevel = c.log_level;
    editBridgeName = c.bridge_name || '';
    editSatelliteHost = c.satellite?.host || '';
    editSatellitePort = c.satellite?.port || 16622;
  }

  async function pollStatus() {
    try {
      status = await invoke<BridgeStatus>('bridge_get_status', { host, configPort });
      registeredDevices = await invoke<BridgeRegisteredDevices>('bridge_get_registered_devices', { host, configPort });
      connectedDevices = await invoke<BridgeConnectedDevices>('bridge_get_devices', { host, configPort });
      if (editMode === 'satellite') {
        satelliteStatus = await invoke<BridgeSatelliteStatus>('bridge_get_satellite_status', { host, configPort });
      }
      errorCount = 0;
      offline = false;
    } catch {
      handleError();
    }
  }

  function handleError() {
    errorCount++;
    if (errorCount >= 3) {
      offline = true;
      if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = setInterval(pollStatus, 10000);
      }
    }
  }

  async function saveConfig() {
    saving = true;
    saveMessage = null;
    try {
      const req: SaveGlobalConfigRequest = {
        mode: editMode,
        broadcast_port: editBroadcastPort,
        feedback_port: editFeedbackPort,
        log_level: editLogLevel,
        bridge_name: editBridgeName || undefined,
        satellite: editMode === 'satellite' ? {
          host: editSatelliteHost || null,
          port: editSatellitePort,
        } : undefined,
      };
      const result = await invoke<BridgeApiResponse>('bridge_save_config', {
        host, configPort, config: req,
      });
      saveMessage = result.message;
      onsave?.(editBridgeName || bridgeName);
      await fetchAll();
    } catch (e) {
      saveMessage = `Error: ${e}`;
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
  }

  async function refreshDevices() {
    try {
      registeredDevices = await invoke<BridgeRegisteredDevices>('bridge_get_registered_devices', { host, configPort });
      connectedDevices = await invoke<BridgeConnectedDevices>('bridge_get_devices', { host, configPort });
    } catch { /* ignore */ }
  }

  function findConnectedDevice(usbPhys: string) {
    if (!connectedDevices?.devices) return null;
    return connectedDevices.devices.find(d => d.usb_phys === usbPhys) ?? null;
  }

  async function shutdownBridge() {
    if (!confirm('Are you sure you want to shut down the bridge? You will need physical access to restart it.')) return;
    shuttingDown = true;
    try {
      await invoke<BridgeApiResponse>('bridge_shutdown', { host, configPort });
      saveMessage = 'Bridge is shutting down...';
      setTimeout(() => onclose(), 2000);
    } catch (e) {
      saveMessage = `Shutdown failed: ${e}`;
      shuttingDown = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="modal-overlay" role="dialog" aria-label="Bridge configuration">
  <div class="modal-content">
    <header class="modal-header">
      <div>
        <h2>{editBridgeName || bridgeName}</h2>
        <span class="host-label">{host}:{configPort}</span>
        <button class="config-link" onclick={() => openUrl(`http://${host}:${configPort}`)}>
          Open config page
        </button>
      </div>
      <button class="close-btn" onclick={onclose} aria-label="Close">&times;</button>
    </header>

    {#if offline}
      <div class="offline-banner">Bridge offline - retrying...</div>
    {/if}

    {#if loading}
      <p class="loading-text">Loading bridge configuration...</p>
    {:else if config}
      <div class="modal-body">
        <!-- Status -->
        <section class="detail-section">
          <h3>Status</h3>
          <div class="status-row">
            <span>Service</span>
            <span class="status-indicator" class:running={status?.running}>
              {status?.running ? 'Running' : 'Stopped'}
            </span>
          </div>
          {#if editMode === 'satellite' && satelliteStatus}
            <div class="status-row">
              <span>Satellite</span>
              <span class="status-indicator" class:running={satelliteStatus.connected}>
                {satelliteStatus.connected ? 'Connected' : 'Disconnected'}
              </span>
            </div>
          {/if}
        </section>

        <!-- Mode & Ports -->
        <section class="detail-section">
          <h3>Configuration</h3>
          <div class="form-group">
            <label for="bridge-name">Bridge Name</label>
            <input id="bridge-name" type="text" bind:value={editBridgeName} placeholder="Bridge name" />
          </div>
          <div class="form-group">
            <label for="mode">Mode</label>
            <select id="mode" bind:value={editMode}>
              {#each config.valid_modes as m}
                <option value={m}>{m}</option>
              {/each}
            </select>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label for="broadcast-port">Broadcast Port</label>
              <input id="broadcast-port" type="number" bind:value={editBroadcastPort} />
            </div>
            <div class="form-group">
              <label for="feedback-port">Feedback Port</label>
              <input id="feedback-port" type="number" bind:value={editFeedbackPort} />
            </div>
          </div>
          <div class="form-group">
            <label for="log-level">Log Level</label>
            <select id="log-level" bind:value={editLogLevel}>
              <option value="DEBUG">DEBUG</option>
              <option value="INFO">INFO</option>
              <option value="WARNING">WARNING</option>
              <option value="ERROR">ERROR</option>
            </select>
          </div>
        </section>

        <!-- Satellite Config -->
        {#if editMode === 'satellite'}
          <section class="detail-section">
            <h3>Satellite</h3>
            <div class="form-row">
              <div class="form-group" style="flex: 2">
                <label for="sat-host">Companion Host</label>
                <input id="sat-host" type="text" bind:value={editSatelliteHost} placeholder="192.168.1.x" />
              </div>
              <div class="form-group" style="flex: 1">
                <label for="sat-port">Port</label>
                <input id="sat-port" type="number" bind:value={editSatellitePort} />
              </div>
            </div>
          </section>
        {/if}

        <!-- Devices -->
        <section class="detail-section">
          <h3>Devices</h3>
          {#if registeredDevices}
            {#each Object.entries(registeredDevices.devices).sort(([a], [b]) => a.localeCompare(b, undefined, { numeric: true })) as [slotName, device]}
              <BridgeDeviceSlot
                {slotName}
                {device}
                connectedDevice={device ? findConnectedDevice(device.usb_phys) : null}
                {host}
                {configPort}
                mode={editMode}
                validChannels={config.valid_channels}
                onchange={refreshDevices}
              />
            {/each}
          {/if}
        </section>

        <!-- Save -->
        <div class="save-area">
          <button class="save-btn" onclick={saveConfig} disabled={saving}>
            {saving ? 'Saving...' : 'Save Configuration'}
          </button>
          {#if saveMessage}
            <p class="save-message" class:error={saveMessage.startsWith('Error') || saveMessage.startsWith('Shutdown failed')}>{saveMessage}</p>
          {/if}
        </div>

        <!-- Shutdown -->
        <div class="shutdown-area">
          <button class="shutdown-btn" onclick={shutdownBridge} disabled={shuttingDown}>
            {shuttingDown ? 'Shutting down...' : 'Shutdown Bridge'}
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding: 2rem;
    z-index: 100;
    overflow-y: auto;
  }

  .modal-content {
    background: #fff;
    border-radius: 12px;
    width: 100%;
    max-width: 520px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 1.25rem 1.25rem 1rem;
    border-bottom: 1px solid #e0e0e0;
  }

  .modal-header h2 {
    margin: 0;
    font-size: 1.25rem;
  }

  .host-label {
    font-size: 0.75rem;
    font-family: monospace;
    color: #888;
  }

  .close-btn {
    background: none;
    border: none;
    font-size: 1.5rem;
    cursor: pointer;
    color: #666;
    padding: 0 0.25rem;
    line-height: 1;
  }
  .close-btn:hover { color: #333; }

  .config-link {
    display: inline-block;
    margin-top: 0.25rem;
    padding: 0;
    background: none;
    border: none;
    font-size: 0.75rem;
    color: #1a73e8;
    cursor: pointer;
    font-family: inherit;
    text-decoration: underline;
  }
  .config-link:hover { color: #1557b0; }

  .offline-banner {
    background: #fce4e4;
    color: #c62828;
    text-align: center;
    padding: 0.5rem;
    font-size: 0.85rem;
    font-weight: 500;
  }

  .loading-text {
    text-align: center;
    padding: 2rem;
    color: #888;
  }

  .modal-body {
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .detail-section {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .detail-section h3 {
    margin: 0;
    font-size: 0.85rem;
    font-weight: 600;
    color: #555;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding-bottom: 0.25rem;
    border-bottom: 1px solid #eee;
  }

  .status-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.875rem;
  }

  .status-indicator {
    font-weight: 500;
    color: #c62828;
  }
  .status-indicator.running {
    color: #2e7d32;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .form-group label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #666;
  }

  .form-group input,
  .form-group select {
    padding: 0.5rem;
    border: 1px solid #ddd;
    border-radius: 6px;
    font-size: 0.875rem;
    font-family: inherit;
    background: #fafafa;
  }

  .form-group input:focus,
  .form-group select:focus {
    outline: none;
    border-color: #1a73e8;
    box-shadow: 0 0 0 2px rgba(26, 115, 232, 0.15);
  }

  .form-row {
    display: flex;
    gap: 0.75rem;
  }
  .form-row .form-group { flex: 1; }

  .save-area {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding-top: 0.5rem;
  }

  .save-btn {
    width: 100%;
    padding: 0.625rem;
    background: #1a73e8;
    color: #fff;
    border: none;
    border-radius: 8px;
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .save-btn:hover { background: #1557b0; }
  .save-btn:disabled { background: #999; cursor: not-allowed; }

  .save-message {
    margin: 0;
    font-size: 0.8rem;
    color: #2e7d32;
  }
  .save-message.error { color: #c62828; }

  .shutdown-area {
    display: flex;
    justify-content: center;
    padding-top: 0.5rem;
    border-top: 1px solid #eee;
  }

  .shutdown-btn {
    padding: 0.5rem 1.25rem;
    background: #fff;
    color: #c62828;
    border: 1px solid #ef9a9a;
    border-radius: 8px;
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .shutdown-btn:hover { background: #ffebee; }
  .shutdown-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  @media (prefers-color-scheme: dark) {
    .modal-content {
      background: #2a2a2a;
      color: #eee;
    }
    .modal-header { border-bottom-color: #444; }
    .modal-header h2 { color: #eee; }
    .host-label { color: #777; }
    .close-btn { color: #888; }
    .close-btn:hover { color: #eee; }
    .config-link { color: #6ab7ff; }
    .config-link:hover { color: #90caf9; }
    .offline-banner { background: #4a1a1a; color: #ff8a80; }
    .detail-section h3 { color: #aaa; border-bottom-color: #444; }
    .form-group label { color: #aaa; }
    .form-group input, .form-group select {
      background: #333;
      border-color: #555;
      color: #eee;
    }
    .form-group input:focus, .form-group select:focus {
      border-color: #6ab7ff;
      box-shadow: 0 0 0 2px rgba(106, 183, 255, 0.15);
    }
    .save-message { color: #81c784; }
    .save-message.error { color: #ff8a80; }
    .shutdown-area { border-top-color: #444; }
    .shutdown-btn { background: #333; color: #ff8a80; border-color: #5c2a2a; }
    .shutdown-btn:hover { background: #4a1a1a; }
  }
</style>
