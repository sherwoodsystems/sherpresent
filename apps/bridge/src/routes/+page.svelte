<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
import type { BridgeInfo, BridgeConfig, DiscoveredPeer, BridgeMode, FeedbackState, UsbDeviceInfo, UsbRegistrationDetected } from '$lib/types';
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
  let unlistenUsbConnected: UnlistenFn | null = null;
  let unlistenUsbDisconnected: UnlistenFn | null = null;
  let unlistenUsbRegistration: UnlistenFn | null = null;
  let registrationSlot = $state<string | null>(null);
  let pendingRegistration = $state<UsbRegistrationDetected | null>(null);
  let confirmingRegistration = $state(false);

  // Derived: just desktop peers (version === '1') — the bridge browses the
  // whole `sher-present` namespace and will see other bridges too.
  let desktopPeers = $derived(peers.peers.filter((p) => p.version === '1'));
  let bridgePeers = $derived(peers.peers.filter((p) => p.version === 'bridge'));

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
  }

  async function startRegistration(slot: string) {
    try {
      await invoke('start_usb_registration', { slot });
      registrationSlot = slot;
      pendingRegistration = null;
      flashToast('ok', `Press a button on the clicker you want to register as "${slot}"`);
    } catch (e) {
      flashToast('err', String(e));
    }
  }

  async function cancelRegistration() {
    try {
      await invoke('cancel_usb_registration');
      registrationSlot = null;
      pendingRegistration = null;
    } catch (e) {
      flashToast('err', String(e));
    }
  }

  async function confirmRegistration() {
    if (!pendingRegistration) return;
    confirmingRegistration = true;
    try {
      const device = usbDevices.find((d) => d.id === pendingRegistration!.deviceId);
      await invoke('confirm_usb_registration', {
        slot: pendingRegistration.slot,
        deviceId: pendingRegistration.deviceId,
        deviceName: device?.name ?? pendingRegistration.deviceId,
      });
      registrationSlot = null;
      pendingRegistration = null;
      await refreshAll();
      flashToast('ok', 'Clicker registered');
    } catch (e) {
      flashToast('err', String(e));
    } finally {
      confirmingRegistration = false;
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
    <section class="card">
      <button class="collapse-header" onclick={() => (settingsOpen = !settingsOpen)}>
        <h2>Settings</h2>
        <span class="chev" class:open={settingsOpen} aria-hidden="true">▾</span>
      </button>

      {#if settingsOpen}
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
      {/if}
    </section>
  {/if}

  <!-- ============= USB Devices ============= -->
  <section class="card">
    <h2>USB Devices <span class="count">{usbDevices.length}</span></h2>

    {#if pendingRegistration}
      <div class="registration-banner">
        <strong>New clicker detected!</strong>
        <span class="muted">
          {usbDevices.find((d) => d.id === pendingRegistration!.deviceId)?.name ?? pendingRegistration.deviceId}
          sent {pendingRegistration.key}.
        </span>
        <div class="registration-actions">
          <button class="btn-primary" onclick={confirmRegistration} disabled={confirmingRegistration}>
            {confirmingRegistration ? 'Saving…' : `Register as ${pendingRegistration.slot}`}
          </button>
          <button class="btn-ghost" onclick={cancelRegistration}>Cancel</button>
        </div>
      </div>
    {:else if registrationSlot}
      <div class="registration-banner">
        <span>Waiting for a button press on the clicker to register as <strong>{registrationSlot}</strong>…</span>
        <button class="btn-ghost" onclick={cancelRegistration}>Cancel</button>
      </div>
    {/if}

    {#if usbDevices.length === 0}
      <p class="empty">No USB clickers detected. Plug one in and it will appear here.</p>
    {:else}
      <ul class="peer-list device-list">
        {#each usbDevices as device (device.id)}
          {@const registered = config?.devices?.[device.id]}
          <li class="peer device-row">
            <span class="peer-dot" class:muted-dot={!registered} aria-hidden="true"></span>
            <span class="peer-name">
              {device.name}
              {#if device.is_perfect_cue}
                <span class="chip chip-green">Perfect Cue</span>
              {/if}
            </span>
            <span class="peer-id">{registered ? registered.label : 'unregistered'}</span>
            <div class="peer-actions">
              {#if registered}
                <select
                  class="target-select"
                  onchange={(e) => {
                    const id = (e.currentTarget as HTMLSelectElement).value;
                    const peer = desktopPeers.find((p) => p.instanceId === id) ?? null;
                    setDeviceTarget(device.id, peer);
                  }}
                  value={registered.target?.instance_id ?? ''}
                >
                  <option value="">No target</option>
                  {#each desktopPeers as peer (peer.instanceId)}
                    <option value={peer.instanceId}>{peer.displayName ?? peer.host}:{peer.port}</option>
                  {/each}
                </select>
              {:else}
                <button class="btn-tiny" onclick={() => startRegistration(device.name || device.id)}>
                  Register
                </button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    {#if config && Object.keys(config.devices).length > 0}
      <div class="registered-summary">
        <h3>Registered slots</h3>
        <ul class="peer-list">
          {#each Object.entries(config.devices) as [deviceId, device] (deviceId)}
            {#if device}
              {@const present = usbDevices.some((d) => d.id === deviceId)}
              <li class="peer">
                <span class="peer-dot" class:muted-dot={!present} aria-hidden="true"></span>
                <span class="peer-name">{device.label}</span>
                <span class="peer-host">{device.target ? `${device.target.host}:${device.target.port}` : 'no target'}</span>
                <button class="btn-tiny" onclick={() => setDeviceTarget(deviceId, null)}>Clear target</button>
              </li>
            {/if}
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

<style>
  :root {
    --bg: #0f1115;
    --bg-card: #181b22;
    --bg-input: #0c0e12;
    --border: #283042;
    --text: #e6e9ef;
    --muted: #8b94a3;
    --accent: #4ade80;
    --accent-bg: rgba(74, 222, 128, 0.12);
    --warn: #f59e0b;
    --danger: #ef4444;
  }

  * { box-sizing: border-box; }
  body, html { margin: 0; padding: 0; background: var(--bg); color: var(--text); }

  main {
    max-width: 720px;
    margin: 0 auto;
    padding: 32px 20px 64px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .header { display: flex; align-items: baseline; gap: 12px; }
  .header h1 { font-size: 1.6rem; margin: 0; font-weight: 600; }
  .badge {
    font-size: 0.75rem; padding: 2px 8px; border-radius: 999px;
    background: var(--accent-bg); color: var(--accent);
    border: 1px solid rgba(74, 222, 128, 0.35);
  }

  .card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 18px 20px;
  }
  .card h2 {
    font-size: 1.05rem; margin: 0 0 12px; font-weight: 600;
    display: flex; align-items: center; gap: 10px;
  }
  .count {
    font-size: 0.75rem; background: var(--border); color: var(--muted);
    padding: 1px 8px; border-radius: 999px;
  }

  .info-grid { display: grid; grid-template-columns: max-content 1fr; gap: 6px 16px; margin: 0; }
  .info-grid dt { color: var(--muted); font-size: 0.85rem; align-self: center; }
  .info-grid dd { margin: 0; align-self: center; }
  .info-grid .name-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .name-value { font-weight: 500; }

  code, .mono { font-family: 'SFMono-Regular', 'JetBrains Mono', Consolas, monospace; }
  code { font-size: 0.85rem; color: var(--accent); background: var(--bg-input); padding: 1px 6px; border-radius: 4px; }

  .chip {
    font-size: 0.75rem; padding: 1px 8px; border-radius: 6px;
    background: var(--border); color: var(--text); margin-right: 6px;
  }

  .muted { color: var(--muted); }
  .small { font-size: 0.75rem; }

  a { color: var(--accent); text-decoration: none; }
  a:hover { text-decoration: underline; }

  .peer-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 6px; }
  .peer {
    display: grid; grid-template-columns: auto 1fr auto auto auto; gap: 10px;
    align-items: center; padding: 8px 10px; background: var(--bg-input);
    border: 1px solid var(--border); border-radius: 6px;
  }
  .peer-name { font-weight: 500; }
  .peer-host { color: var(--muted); font-size: 0.85rem; font-variant-numeric: tabular-nums; }
  .peer-id { color: var(--muted); font-size: 0.85rem; }
  .peer-actions { display: flex; gap: 6px; }
  .peer-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--accent); box-shadow: 0 0 6px var(--accent);
  }
  .muted-dot { background: var(--muted); box-shadow: none; }

  .empty { color: var(--muted); margin: 0; }

  .feedback-grid {
    display: grid; grid-template-columns: 1fr 1fr; gap: 10px 16px;
  }
  .feedback-cell { display: flex; flex-direction: column; gap: 2px; }
  .feedback-cell.wide { grid-column: 1 / -1; }
  .feedback-label { color: var(--muted); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .feedback-value { font-size: 0.95rem; }
  .chip-green { background: var(--accent-bg); color: var(--accent); border: 1px solid rgba(74, 222, 128, 0.35); }
  .notes { margin-top: 12px; padding: 10px; background: var(--bg-input); border: 1px solid var(--border); border-radius: 6px; }
  .notes p { margin: 4px 0 0; color: var(--muted); }

  .btn-tiny:disabled { opacity: 0.5; cursor: not-allowed; }

  .collapse-header {
    background: transparent; border: none; padding: 0; cursor: pointer;
    width: 100%; display: flex; justify-content: space-between; align-items: center;
    color: inherit;
  }
  .collapse-header h2 { margin: 0; }
  .chev { transition: transform 0.15s ease; color: var(--muted); }
  .chev.open { transform: rotate(180deg); }

  .settings {
    display: grid; grid-template-columns: 1fr 1fr; gap: 12px 16px;
    margin-top: 16px;
  }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 0.85rem; color: var(--muted); }
  label > span { font-weight: 500; }
  input, select {
    background: var(--bg-input); color: var(--text); border: 1px solid var(--border);
    padding: 6px 10px; border-radius: 6px; font: inherit; outline: none;
  }
  input:focus, select:focus { border-color: var(--accent); }

  .settings-actions { grid-column: 1 / -1; display: flex; gap: 8px; margin-top: 4px; }
  .settings-actions + .muted { font-size: 0.75rem; }

  .btn-primary, .btn-ghost {
    border: 1px solid var(--border); padding: 6px 14px;
    border-radius: 6px; cursor: pointer; font: inherit; color: var(--text);
    background: var(--bg-input);
  }
  .btn-primary { background: var(--accent); color: #06120c; border-color: var(--accent); font-weight: 600; }
  .btn-primary:hover { filter: brightness(1.05); }
  .btn-primary:disabled { opacity: 0.6; cursor: not-allowed; }
  .btn-ghost:hover { border-color: var(--accent); color: var(--accent); }

  .btn-tiny {
    background: transparent; border: 1px solid var(--border); color: var(--muted);
    padding: 1px 8px; border-radius: 4px; cursor: pointer; font-size: 0.78rem; font: inherit;
  }
  .btn-tiny:hover { color: var(--accent); border-color: var(--accent); }

  .device-list { margin-top: 12px; }
  .device-row { grid-template-columns: auto 1fr auto auto; }

  .target-select {
    background: var(--bg-input); color: var(--text); border: 1px solid var(--border);
    padding: 2px 8px; border-radius: 4px; font: inherit; font-size: 0.78rem; max-width: 180px;
  }

  .registration-banner {
    display: flex; align-items: center; gap: 12px; flex-wrap: wrap;
    padding: 10px 12px; background: var(--accent-bg); border: 1px solid rgba(74, 222, 128, 0.35);
    border-radius: 6px; margin-bottom: 12px;
  }
  .registration-actions { display: flex; gap: 8px; margin-left: auto; }

  .registered-summary { margin-top: 18px; padding-top: 14px; border-top: 1px solid var(--border); }
  .registered-summary h3 { font-size: 0.85rem; margin: 0 0 10px; color: var(--muted); font-weight: 500; }

  .error { border-color: var(--danger); background: rgba(239, 68, 68, 0.1); }
  .error pre { white-space: pre-wrap; margin: 6px 0 0; font-size: 0.85rem; }

  .toast {
    position: fixed; bottom: 16px; left: 50%; transform: translateX(-50%);
    padding: 8px 16px; border-radius: 8px; font-size: 0.9rem;
    background: var(--bg-card); border: 1px solid var(--border);
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
  }
  .toast.ok { color: var(--accent); border-color: var(--accent); }
  .toast.err { color: var(--danger); border-color: var(--danger); }

  @media (max-width: 600px) {
    .settings { grid-template-columns: 1fr; }
    .info-grid { grid-template-columns: 1fr; }
    .info-grid dt { margin-top: 8px; }
  }
</style>