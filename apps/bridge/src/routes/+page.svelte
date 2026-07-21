<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
import type { BridgeInfo, BridgeConfig, DeviceConfig, DiscoveredPeer, BridgeMode, FeedbackState, UsbDeviceInfo, UsbRegistrationDetected, UsbPermissionStatus, KeyAction } from '$lib/types';
import { usePeers } from '$lib/usePeers.svelte';

  // --- reactive state -------------------------------------------------------
  let bridgeInfo = $state<BridgeInfo | null>(null);
  let config = $state<BridgeConfig | null>(null);
  const peers = usePeers();
  let loadError = $state<string | null>(null);
  let toast = $state<{ kind: 'ok' | 'err'; msg: string } | null>(null);

  // Edit-name form state
  let editingName = $state(false);
  let nameDraft = $state('');
  let newNameSaving = $state(false);

  // Settings form state
  let settingsOpen = $state(false);
  let settingsDraft = $state<BridgeConfig | null>(null);
  let settingsSaving = $state(false);

  // Feedback state from desktops
  let feedback = $state<FeedbackState | null>(null);
  let unlistenFeedback: UnlistenFn | null = null;
  let sendingTest = $state<Record<string, boolean>>({});

  // USB clicker state
  let usbDevices = $state<UsbDeviceInfo[]>([]);
  let usbAccessDenied = $state(false);
  let installingPerms = $state(false);
  let permsInstalled = $state(false);
  let unlistenUsbConnected: UnlistenFn | null = null;
  let unlistenUsbDisconnected: UnlistenFn | null = null;
  let unlistenUsbRegistration: UnlistenFn | null = null;
  let unlistenUsbAccessDenied: UnlistenFn | null = null;
  let bindingAction = $state<KeyAction | null>(null);
  let pendingRegistration = $state<UsbRegistrationDetected | null>(null);
  let confirmingRegistration = $state(false);

  // Derived: just desktop peers (version === '1') — the bridge browses the
  // whole `sher-present` namespace and will see other bridges too.
  let desktopPeers = $derived(peers.peers.filter((p) => p.version === '1'));
  let bridgePeers = $derived(peers.peers.filter((p) => p.version === 'bridge'));

  // Registered devices (≥1 binding) that aren't currently connected.
  let registeredOffline = $derived(
    (config ? Object.entries(config.devices) : []).filter(
      (e): e is [string, DeviceConfig] =>
        !!e[1] &&
        Object.keys(e[1].bindings).length > 0 &&
        !usbDevices.some((d) => d.id === e[0]),
    ),
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
      pendingRegistration = event.payload;
    });
    unlistenUsbAccessDenied = await listen<number>('usb-access-denied', (event) => {
      usbAccessDenied = event.payload > 0;
    });
    await refreshAll();
  });

  // onDestroy isn't available in runes mode; use the return callback.
  $effect(() => {
    return () => {
      if (unlistenFeedback) {
        unlistenFeedback();
        unlistenFeedback = null;
      }
      if (unlistenUsbConnected) {
        unlistenUsbConnected();
        unlistenUsbConnected = null;
      }
      if (unlistenUsbDisconnected) {
        unlistenUsbDisconnected();
        unlistenUsbDisconnected = null;
      }
      if (unlistenUsbRegistration) {
        unlistenUsbRegistration();
        unlistenUsbRegistration = null;
      }
      if (unlistenUsbAccessDenied) {
        unlistenUsbAccessDenied();
        unlistenUsbAccessDenied = null;
      }
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
      nameDraft = cfg.bridge_name;
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
    } catch (e) {
      console.error('Failed to get USB permission status:', e);
    }
  }

  async function fixUsbPermissions() {
    installingPerms = true;
    try {
      await invoke('install_udev_rules');
      permsInstalled = true;
      flashToast('ok', 'Permissions installed — now unplug and replug your clicker.');
    } catch (e) {
      flashToast('err', String(e));
    } finally {
      installingPerms = false;
    }
  }

  async function startBinding(action: KeyAction) {
    try {
      await invoke('start_usb_registration', { action });
      bindingAction = action;
      pendingRegistration = null;
      flashToast('ok', `Press any key on any device to bind as ${action === 'next' ? 'Next' : 'Prev'}`);
    } catch (e) {
      flashToast('err', String(e));
    }
  }

  async function cancelRegistration() {
    try {
      await invoke('cancel_usb_registration');
      bindingAction = null;
      pendingRegistration = null;
    } catch (e) {
      flashToast('err', String(e));
    }
  }

  async function confirmBinding() {
    if (!pendingRegistration) return;
    confirmingRegistration = true;
    try {
      const device = usbDevices.find((d) => d.id === pendingRegistration!.deviceId);
      await invoke('confirm_usb_binding', {
        deviceId: pendingRegistration.deviceId,
        deviceName: device?.name ?? pendingRegistration.deviceId,
        key: pendingRegistration.key,
        action: pendingRegistration.action,
      });
      bindingAction = null;
      pendingRegistration = null;
      await refreshAll();
      flashToast('ok', 'Key bound');
    } catch (e) {
      flashToast('err', String(e));
    } finally {
      confirmingRegistration = false;
    }
  }

  async function removeBinding(deviceId: string, key: string) {
    try {
      await invoke('remove_usb_binding', { deviceId, key });
      await refreshAll();
      flashToast('ok', 'Binding removed');
    } catch (e) {
      flashToast('err', String(e));
    }
  }

  async function forgetDevice(deviceId: string, device: DeviceConfig) {
    try {
      for (const key of Object.keys(device.bindings)) {
        await invoke('remove_usb_binding', { deviceId, key });
      }
      await refreshAll();
      flashToast('ok', 'Device forgotten');
    } catch (e) {
      flashToast('err', String(e));
    }
  }

  async function setDeviceTarget(deviceId: string, peer: DiscoveredPeer | null) {
    try {
      await invoke('set_device_target', {
        deviceId,
        host: peer?.host ?? null,
        port: peer?.port ?? null,
        name: peer?.displayName ?? null,
        instanceId: peer?.instanceId ?? null,
      });
      await refreshAll();
      flashToast('ok', peer ? 'Target assigned' : 'Target cleared');
    } catch (e) {
      flashToast('err', String(e));
    }
  }

  async function saveName() {
    if (!config) return;
    const trimmed = nameDraft.trim();
    if (!trimmed) {
      flashToast('err', 'Bridge name cannot be empty');
      return;
    }
    newNameSaving = true;
    try {
      await invoke('set_instance_name', { name: trimmed });
      await refreshAll();
      editingName = false;
      flashToast('ok', 'Bridge name updated');
    } catch (e) {
      flashToast('err', String(e));
    } finally {
      newNameSaving = false;
    }
  }

  function cancelEditName() {
    if (config) nameDraft = config.bridge_name;
    editingName = false;
  }

  async function saveSettings() {
    if (!settingsDraft) return;
    settingsSaving = true;
    try {
      // Keep `bridge_id` from the live config so save_settings doesn't blow it away.
      const merged: BridgeConfig = {
        ...settingsDraft,
        bridge_id: config?.bridge_id ?? settingsDraft.bridge_id,
        bridge_name: config?.bridge_name ?? settingsDraft.bridge_name,
      };
      await invoke('save_config', { config: merged });
      await refreshAll();
      settingsOpen = false;
      flashToast('ok', 'Settings saved');
    } catch (e) {
      flashToast('err', String(e));
    } finally {
      settingsSaving = false;
    }
  }

  function flashToast(kind: 'ok' | 'err', msg: string) {
    toast = { kind, msg };
    setTimeout(() => {
      toast = null;
    }, 2500);
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      flashToast('ok', 'Copied to clipboard');
    } catch {
      flashToast('err', 'Copy failed');
    }
  }

  async function sendTest(peer: DiscoveredPeer, command: 'next' | 'prev') {
    if (peer.isSelf) {
      flashToast('err', "Cannot send a test command to the bridge's own feedback port");
      return;
    }
    const key = `${peer.instanceId}-${command}`;
    sendingTest[key] = true;
    try {
      await invoke('send_test_osc', { host: peer.host, port: peer.port, command, slide: null });
      flashToast('ok', `Sent /oscpoint/${command} to ${peer.displayName ?? peer.host}`);
    } catch (e) {
      flashToast('err', String(e));
    } finally {
      sendingTest[key] = false;
    }
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

  <!-- ============= Bridge Identity ============= -->
  {#if bridgeInfo}
    {@const info = bridgeInfo}
    <section class="card">
      <h2>This Bridge</h2>
      <dl class="info-grid">
        <dt>Name</dt>
        <dd class="name-row">
          {#if editingName}
            <input
              type="text"
              bind:value={nameDraft}
              onkeydown={(e) => e.key === 'Enter' && saveName()}
              disabled={newNameSaving}
              placeholder="Bridge name"
            />
            <button class="btn-primary" onclick={saveName} disabled={newNameSaving}>
              {newNameSaving ? 'Saving…' : 'Save'}
            </button>
            <button class="btn-ghost" onclick={cancelEditName} disabled={newNameSaving}>
              Cancel
            </button>
          {:else}
            <span class="name-value">{info.bridge_name}</span>
            <button class="btn-ghost" onclick={() => { nameDraft = info.bridge_name; editingName = true; }}>
              Rename
            </button>
          {/if}
        </dd>

        <dt>Bridge ID</dt>
        <dd class="mono">
          <code>{info.bridge_id}</code>
        </dd>

        <dt>Mode</dt>
        <dd><span class="chip">{info.mode}</span> <span class="chip">{info.log_level}</span></dd>

        <dt>OSC feedback</dt>
        <dd>UDP <code>{info.feedback_port}</code></dd>

        <dt>Config API</dt>
        <dd>
          TCP <code>{info.config_port}</code>
          {#if info.lan_ip}
            <span class="muted">— accessible at <a
              href={info.config_url}
              onclick={(e) => { e.preventDefault(); openUrl(info.config_url); }}
            >{info.config_url}</a>
              <button class="btn-tiny" onclick={() => copy(info.config_url)}>Copy</button>
            </span>
          {/if}
        </dd>

        <dt>LAN IP</dt>
        <dd class="mono">{info.lan_ip ?? 'unknown'}</dd>
      </dl>
    </section>
  {/if}

  <!-- ============= Desktop Feedback ============= -->
  <section class="card">
    <h2>Desktop Feedback</h2>
    {#if feedback}
      <div class="feedback-grid">
        <div class="feedback-cell">
          <span class="feedback-label">Status</span>
          <span class="feedback-value">
            {#if feedback.presenting}
              <span class="chip chip-green">Presenting</span>
            {:else}
              <span class="chip">Idle</span>
            {/if}
          </span>
        </div>
        <div class="feedback-cell">
          <span class="feedback-label">Slide</span>
          <span class="feedback-value">
            {#if feedback.currentSlide != null && feedback.slideCount != null}
              {feedback.currentSlide} / {feedback.slideCount}
            {:else}
              <span class="muted">—</span>
            {/if}
          </span>
        </div>
        <div class="feedback-cell wide">
          <span class="feedback-label">Presentation</span>
          <span class="feedback-value">{feedback.presentationName ?? '—'}</span>
        </div>
        <div class="feedback-cell wide">
          <span class="feedback-label">Last command</span>
          <span class="feedback-value muted">
            {feedback.lastCommand ? feedback.lastCommand : '—'}
            {feedback.lastCommandTime ? `at ${new Date(feedback.lastCommandTime).toLocaleTimeString()}` : ''}
          </span>
        </div>
      </div>
      {#if feedback.slideNotes}
        <div class="notes">
          <strong>Notes</strong>
          <p>{feedback.slideNotes}</p>
        </div>
      {/if}
    {:else}
      <p class="empty">No feedback yet. Desktops will send slide status here once they connect.</p>
    {/if}
  </section>

  <!-- ============= Discovered Desktops ============= -->
  <section class="card">
    <h2>Discovered Desktops <span class="count">{desktopPeers.length}</span></h2>
    {#if desktopPeers.length === 0}
      <p class="empty">
        No SherPresent desktops found on the LAN.
        <span class="muted">Ensure desktop apps are running and on the same subnet.</span>
      </p>
    {:else}
      <ul class="peer-list">
        {#each desktopPeers as peer (peer.instanceId)}
          <li class="peer">
            <span class="peer-dot" aria-hidden="true"></span>
            <span class="peer-name">{peer.displayName ?? '(unnamed)'}</span>
            <span class="peer-host">{peer.host}:{peer.port}</span>
            <span class="peer-id">#{peer.displayId}</span>
            <div class="peer-actions">
              <button
                class="btn-tiny"
                disabled={sendingTest[`${peer.instanceId}-prev`]}
                onclick={() => sendTest(peer, 'prev')}
              >
                Test Prev
              </button>
              <button
                class="btn-tiny"
                disabled={sendingTest[`${peer.instanceId}-next`]}
                onclick={() => sendTest(peer, 'next')}
              >
                Test Next
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

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

  <!-- ============= Settings ============= -->
  {#if config && settingsDraft}
    <section class="card collapsible">
      <button class="collapse-header" onclick={() => (settingsOpen = !settingsOpen)}>
        <h2>Settings</h2>
        <span class="chev" class:open={settingsOpen} aria-hidden="true">▾</span>
      </button>

      {#if settingsOpen}
        <div class="collapse-body">
        <div class="settings">
          <label>
            <span>Mode</span>
            <select bind:value={settingsDraft.mode}>
              <option value="direct">Direct (OSC UDP)</option>
              <option value="satellite">Companion Satellite (TCP)</option>
            </select>
          </label>

          <label>
            <span>OSC feedback port</span>
            <input
              type="number"
              min="1"
              max="65535"
              bind:value={settingsDraft.feedback_port}
            />
          </label>

          <label>
            <span>HTTP config port</span>
            <input
              type="number"
              min="1"
              max="65535"
              bind:value={settingsDraft.config_port}
            />
          </label>

          <label>
            <span>Log level</span>
            <select bind:value={settingsDraft.log_level}>
              <option value="DEBUG">DEBUG</option>
              <option value="INFO">INFO</option>
              <option value="WARNING">WARNING</option>
              <option value="ERROR">ERROR</option>
            </select>
          </label>

          <label>
            <span>Satellite host</span>
            <input
              type="text"
              placeholder="companion host (satellite mode only)"
              bind:value={settingsDraft.satellite.host}
            />
          </label>

          <label>
            <span>Satellite port</span>
            <input
              type="number"
              min="1"
              max="65535"
              bind:value={settingsDraft.satellite.port}
            />
          </label>

          <div class="settings-actions">
            <button class="btn-primary" onclick={saveSettings} disabled={settingsSaving}>
              {settingsSaving ? 'Saving…' : 'Save settings'}
            </button>
            <button class="btn-ghost" onclick={() => { settingsDraft = structuredClone($state.snapshot(config)); }}>
              Reset
            </button>
          </div>
          <p class="muted small">
            Note: changing ports requires restarting the bridge app to take effect
            for the OSC listener and HTTP API.
          </p>
        </div>
        </div>
      {/if}
    </section>
  {/if}

  <!-- ============= USB Devices ============= -->
  <section class="card">
    <h2>USB Devices <span class="count">{usbDevices.length}</span></h2>

    {#if usbAccessDenied}
      <div class="perms-banner">
        <div class="perms-head">
          <strong>⚠ No access to USB input devices</strong>
          <span class="muted small">
            The bridge found USB devices it can't read. On Linux it needs
            permission for <code>/dev/input/event*</code>.
          </span>
        </div>
        <div class="perms-actions">
          <button class="btn-primary" onclick={fixUsbPermissions} disabled={installingPerms}>
            {installingPerms ? 'Installing…' : 'Fix permissions'}
          </button>
        </div>
        {#if permsInstalled}
          <p class="muted small">Now unplug and replug your clicker for the change to take effect.</p>
        {/if}
        <details class="perms-manual">
          <summary class="muted small">Or run manually</summary>
          <pre class="perms-cmd">echo 'SUBSYSTEM=="input", SUBSYSTEMS=="usb", TAG+="uaccess"' | sudo tee /etc/udev/rules.d/70-sherpresent-clicker.rules
sudo udevadm control --reload &amp;&amp; sudo udevadm trigger</pre>
          <span class="muted small">Then unplug and replug the device.</span>
        </details>
      </div>
    {/if}

    {#if pendingRegistration}
      {@const pr = pendingRegistration}
      <div class="registration-banner">
        <strong>Key detected!</strong>
        <span class="muted">
          {usbDevices.find((d) => d.id === pr.deviceId)?.name ?? pr.deviceId}
          sent {pr.key}.
        </span>
        <div class="registration-actions">
          <button class="btn-primary" onclick={confirmBinding} disabled={confirmingRegistration}>
            {confirmingRegistration ? 'Saving…' : `Bind as ${pr.action === 'next' ? 'Next' : 'Prev'}`}
          </button>
          <button class="btn-ghost" onclick={cancelRegistration}>Cancel</button>
        </div>
      </div>
    {:else if bindingAction}
      <div class="registration-banner">
        <span>Press any key on any device to bind as <strong>{bindingAction === 'next' ? 'Next' : 'Prev'}</strong>…</span>
        <button class="btn-ghost" onclick={cancelRegistration}>Cancel</button>
      </div>
    {/if}

    {#if usbDevices.length === 0}
      <p class="empty">No USB devices detected. Plug one in and it will appear here.</p>
    {:else}
      <ul class="peer-list device-list">
        {#each usbDevices as device (device.id)}
          {@const registered = config?.devices?.[device.id] ?? null}
          {@const bindings = registered ? Object.entries(registered.bindings) : []}
          <li class="usb-device">
            <div class="usb-device-head">
              <span class="peer-dot" class:muted-dot={bindings.length === 0} aria-hidden="true"></span>
              <span class="peer-name">
                {device.name}
                {#if device.is_perfect_cue}
                  <span class="chip chip-green">Perfect Cue</span>
                {/if}
              </span>
              <div class="usb-device-actions">
                <button class="btn-tiny" onclick={() => startBinding('next')} disabled={!!bindingAction}>Bind Next</button>
                <button class="btn-tiny" onclick={() => startBinding('prev')} disabled={!!bindingAction}>Bind Prev</button>
              </div>
            </div>

            {#if bindings.length > 0}
              <div class="usb-bindings">
                {#each bindings as [key, action] (key)}
                  <span class="binding-chip">
                    {key} → {action === 'next' ? 'Next' : 'Prev'}
                    <button class="binding-x" title="Remove binding" onclick={() => removeBinding(device.id, key)}>×</button>
                  </span>
                {/each}
              </div>

              <label class="usb-target">
                <span class="muted small">Target</span>
                <select
                  class="target-select"
                  onchange={(e) => {
                    const id = (e.currentTarget as HTMLSelectElement).value;
                    const peer = desktopPeers.find((p) => p.instanceId === id) ?? null;
                    setDeviceTarget(device.id, peer);
                  }}
                  value={registered?.target?.instance_id ?? ''}
                >
                  <option value="">No target</option>
                  {#each desktopPeers as peer (peer.instanceId)}
                    <option value={peer.instanceId}>{peer.displayName ?? peer.host}:{peer.port}</option>
                  {/each}
                </select>
              </label>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if registeredOffline.length > 0}
      <div class="registered-summary">
        <h3>Registered (not connected)</h3>
        <ul class="peer-list">
          {#each registeredOffline as [deviceId, device] (deviceId)}
            <li class="peer">
              <span class="peer-dot muted-dot" aria-hidden="true"></span>
              <span class="peer-name">{device.label}</span>
              <span class="peer-host">{device.target ? `${device.target.host}:${device.target.port}` : 'no target'}</span>
              <button class="btn-tiny" onclick={() => forgetDevice(deviceId, device)}>Forget</button>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
  </section>
</main>

{#if toast}
  <div class="toast" class:ok={toast.kind === 'ok'} class:err={toast.kind === 'err'} role="status">
    {toast.msg}
  </div>
{/if}
