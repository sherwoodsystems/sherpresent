<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type {
    BridgeInfo,
    BridgeConfig,
    DeviceConfig,
    DiscoveredPeer,
    FeedbackState,
    UsbDeviceInfo,
    UsbRegistrationDetected,
    UsbPermissionStatus,
    KeyAction,
  } from '$lib/types';
  import { usePeers } from '$lib/usePeers.svelte';
  import { actionLabel } from '$lib/format';
  import BridgeInfoCard from '$lib/components/BridgeInfoCard.svelte';
  import FeedbackCard from '$lib/components/FeedbackCard.svelte';
  import PeersCard from '$lib/components/PeersCard.svelte';
  import SettingsCard from '$lib/components/SettingsCard.svelte';
  import UsbDevicesCard from '$lib/components/UsbDevicesCard.svelte';

  // --- reactive state -------------------------------------------------------
  let bridgeInfo = $state<BridgeInfo | null>(null);
  let config = $state<BridgeConfig | null>(null);
  const peers = usePeers();
  let loadError = $state<string | null>(null);
  let toast = $state<{ kind: 'ok' | 'err'; msg: string } | null>(null);

  // Settings form draft (owned here; SettingsCard binds its fields).
  let settingsDraft = $state<BridgeConfig | null>(null);

  // Feedback state from desktops
  let feedback = $state<FeedbackState | null>(null);
  let unlistenFeedback: UnlistenFn | null = null;
  let sendingTest = $state<Record<string, boolean>>({});

  // USB clicker state
  let usbDevices = $state<UsbDeviceInfo[]>([]);
  let usbAccessDenied = $state(false);
  let usbFixCommand = $state('sudo usermod -aG input $USER');
  let unlistenUsbConnected: UnlistenFn | null = null;
  let unlistenUsbDisconnected: UnlistenFn | null = null;
  let unlistenUsbRegistration: UnlistenFn | null = null;
  let unlistenUsbAccessDenied: UnlistenFn | null = null;
  let bindingAction = $state<KeyAction | null>(null);

  // Derived: just desktop peers (version === '1') — the bridge browses the
  // whole `sher-present` namespace and will see other bridges too.
  let desktopPeers = $derived(peers.peers.filter((p) => p.version === '1'));
  let bridgePeers = $derived(peers.peers.filter((p) => p.version === 'bridge'));

  // Registered devices (≥1 binding) that aren't currently connected.
  let registeredOffline = $derived(
    Object.entries(config?.devices ?? {}).filter(
      ([id, device]) =>
        Object.keys(device.bindings).length > 0 && !usbDevices.some((d) => d.id === id),
    ) as [string, DeviceConfig][],
  );

  // --- lifecycle -----------------------------------------------------------
  onMount(async () => {
    await peers.start();
    unlistenFeedback = await listen<FeedbackState>('feedback-updated', (event) => {
      feedback = event.payload;
    });
    unlistenUsbConnected = await listen<UsbDeviceInfo>('usb-connected', (event) => {
      usbDevices = usbDevices.filter((d) => d.id !== event.payload.id).concat(event.payload);
    });
    unlistenUsbDisconnected = await listen<string>('usb-disconnected', (event) => {
      usbDevices = usbDevices.filter((d) => d.id !== event.payload);
    });
    unlistenUsbRegistration = await listen<UsbRegistrationDetected>('usb-registration-detected', (event) => {
      autoBind(event.payload);
    });
    unlistenUsbAccessDenied = await listen<number>('usb-access-denied', (event) => {
      usbAccessDenied = event.payload > 0;
    });
    await refreshAll();
  });

  // onDestroy isn't available in runes mode; use the return callback.
  $effect(() => {
    return () => {
      unlistenFeedback?.();
      unlistenUsbConnected?.();
      unlistenUsbDisconnected?.();
      unlistenUsbRegistration?.();
      unlistenUsbAccessDenied?.();
      unlistenFeedback = null;
      unlistenUsbConnected = null;
      unlistenUsbDisconnected = null;
      unlistenUsbRegistration = null;
      unlistenUsbAccessDenied = null;
    };
  });

  async function refreshAll() {
    try {
      const [info, cfg, fb] = await Promise.all([
        invoke<BridgeInfo>('get_bridge_info'),
        invoke<BridgeConfig>('get_config'),
        invoke<FeedbackState>('get_feedback_state'),
      ]);
      bridgeInfo = info;
      config = cfg;
      feedback = fb;
      settingsDraft = structuredClone($state.snapshot(cfg));
      await refreshUsbDevices();
    } catch (e) {
      loadError = String(e);
      console.error(e);
    }
  }

  async function refreshUsbDevices() {
    try {
      usbDevices = await invoke<UsbDeviceInfo[]>('get_usb_devices');
    } catch (e) {
      console.error('Failed to refresh USB devices:', e);
    }
    try {
      const status = await invoke<UsbPermissionStatus>('get_usb_permission_status');
      usbAccessDenied = status.accessDenied;
      if (status.fixCommand) {
        usbFixCommand = status.fixCommand;
      }
    } catch (e) {
      console.error('Failed to get USB permission status:', e);
    }
  }

  // --- helpers -------------------------------------------------------------
  function flashToast(kind: 'ok' | 'err', msg: string) {
    toast = { kind, msg };
    setTimeout(() => {
      toast = null;
    }, 2500);
  }

  /** Run an async action, surfacing any failure as an error toast. Success
   *  toasts (if any) are emitted inside `fn`. */
  async function withToast(fn: () => Promise<void>, errMsg?: string) {
    try {
      await fn();
    } catch (e) {
      flashToast('err', errMsg ?? String(e));
    }
  }

  // --- actions (bubbled up from card components) ---------------------------

  /** Rename the bridge. Throws on failure so the inline form stays open. */
  async function renameBridge(name: string) {
    const trimmed = name.trim();
    if (!trimmed) {
      flashToast('err', 'Bridge name cannot be empty');
      throw new Error('Bridge name cannot be empty');
    }
    try {
      await invoke('set_instance_name', { name: trimmed });
      await refreshAll();
      flashToast('ok', 'Bridge name updated');
    } catch (e) {
      flashToast('err', String(e));
      throw e;
    }
  }

  /** Persist settings. Throws on failure so SettingsCard keeps the panel open. */
  async function saveSettings() {
    if (!settingsDraft) return;
    try {
      // Keep `bridge_id`/`bridge_name` from the live config so we don't blow them away.
      const merged: BridgeConfig = {
        ...settingsDraft,
        bridge_id: config?.bridge_id ?? settingsDraft.bridge_id,
        bridge_name: config?.bridge_name ?? settingsDraft.bridge_name,
      };
      await invoke('save_config', { config: merged });
      await refreshAll();
      flashToast('ok', 'Settings saved');
    } catch (e) {
      flashToast('err', String(e));
      throw e;
    }
  }

  function resetSettings() {
    if (config) settingsDraft = structuredClone($state.snapshot(config));
  }

  function startBinding(action: KeyAction) {
    return withToast(async () => {
      await invoke('start_usb_registration', { action });
      bindingAction = action;
      flashToast('ok', `Press any key on any device to bind as ${actionLabel(action)}`);
    });
  }

  function cancelRegistration() {
    return withToast(async () => {
      await invoke('cancel_usb_registration');
      bindingAction = null;
    });
  }

  function autoBind(registration: UsbRegistrationDetected) {
    return withToast(async () => {
      const device = usbDevices.find((d) => d.id === registration.deviceId);
      await invoke('confirm_usb_binding', {
        deviceId: registration.deviceId,
        deviceName: device?.name ?? registration.deviceId,
        key: registration.key,
        action: registration.action,
      });
      bindingAction = null;
      await refreshAll();
      flashToast('ok', `Bound ${registration.key} as ${actionLabel(registration.action)}`);
    });
  }

  function removeBinding(deviceId: string, key: string) {
    return withToast(async () => {
      await invoke('remove_usb_binding', { deviceId, key });
      await refreshAll();
      flashToast('ok', 'Binding removed');
    });
  }

  function forgetDevice(deviceId: string, device: DeviceConfig) {
    return withToast(async () => {
      for (const key of Object.keys(device.bindings)) {
        await invoke('remove_usb_binding', { deviceId, key });
      }
      await refreshAll();
      flashToast('ok', 'Device forgotten');
    });
  }

  function setDeviceTarget(deviceId: string, peer: DiscoveredPeer | null) {
    return withToast(async () => {
      await invoke('set_device_target', {
        deviceId,
        host: peer?.host ?? null,
        port: peer?.port ?? null,
        name: peer?.displayName ?? null,
        instanceId: peer?.instanceId ?? null,
      });
      await refreshAll();
      flashToast('ok', peer ? 'Target assigned' : 'Target cleared');
    });
  }

  async function copyFixCommand() {
    await withToast(async () => {
      await navigator.clipboard.writeText(usbFixCommand);
      flashToast('ok', 'Command copied. Run it in a terminal, then log out and back in.');
    }, 'Could not copy to clipboard.');
  }

  async function copy(text: string) {
    await withToast(async () => {
      await navigator.clipboard.writeText(text);
      flashToast('ok', 'Copied to clipboard');
    }, 'Copy failed');
  }

  async function sendTest(peer: DiscoveredPeer, command: KeyAction) {
    if (peer.isSelf) {
      flashToast('err', "Cannot send a test command to the bridge's own feedback port");
      return;
    }
    const key = `${peer.instanceId}-${command}`;
    sendingTest[key] = true;
    await withToast(async () => {
      await invoke('send_test_osc', { host: peer.host, port: peer.port, command, slide: null });
      flashToast('ok', `Sent /oscpoint/${command} to ${peer.displayName ?? peer.host}`);
    });
    sendingTest[key] = false;
  }
</script>

<svelte:head>
  <title>SherPresent Bridge</title>
</svelte:head>

<main>
  <header class="header">
    <h1>SherPresent Bridge</h1>
    <span class="badge">v0.1.0</span>
  </header>

  {#if loadError}
    <div class="card error">
      <strong>Failed to load bridge state.</strong>
      <pre>{loadError}</pre>
    </div>
  {/if}

  {#if bridgeInfo}
    <BridgeInfoCard info={bridgeInfo} onRename={renameBridge} onCopy={copy} />
  {/if}

  <FeedbackCard {feedback} />

  <PeersCard peers={desktopPeers} {sendingTest} onTest={sendTest} />

  <!-- ============= Other Bridges ============= -->
  {#if bridgePeers.length > 0}
    <section class="card">
      <h2>Other Bridges <span class="count">{bridgePeers.length}</span></h2>
      <ul class="peer-list">
        {#each bridgePeers as peer (peer.instanceId)}
          <li class="peer">
            <span class="peer-dot muted-dot" aria-hidden="true"></span>
            <span class="peer-name">{peer.displayName ?? '(unnamed bridge)'}</span>
            <span class="peer-host">{peer.host}:{peer.port}</span>
            {#if peer.configPort}
              <a
                href={`http://${peer.host}:${peer.configPort}/`}
                onclick={(e) => { e.preventDefault(); openUrl(`http://${peer.host}:${peer.configPort}/`); }}
                class="btn-tiny"
              >
                Open config
              </a>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if config && settingsDraft}
    <SettingsCard draft={settingsDraft} onSave={saveSettings} onReset={resetSettings} />
  {/if}

  <UsbDevicesCard
    devices={usbDevices}
    {config}
    {desktopPeers}
    {registeredOffline}
    {usbAccessDenied}
    {usbFixCommand}
    {bindingAction}
    onCopyFixCommand={copyFixCommand}
    onStartBinding={startBinding}
    onCancelRegistration={cancelRegistration}
    onRemoveBinding={removeBinding}
    onSetTarget={setDeviceTarget}
    onForget={forgetDevice}
  />
</main>

{#if toast}
  <div class="toast" class:ok={toast.kind === 'ok'} class:err={toast.kind === 'err'} role="status">
    {toast.msg}
  </div>
{/if}
