<script lang="ts">
  import type {
    BridgeConfig,
    DeviceConfig,
    DiscoveredPeer,
    KeyAction,
    UsbDeviceInfo,
  } from '$lib/types';
  import { actionLabel } from '$lib/format';

  let {
    devices,
    config,
    desktopPeers,
    registeredOffline,
    usbAccessDenied,
    usbFixCommand,
    bindingAction,
    onCopyFixCommand,
    onStartBinding,
    onCancelRegistration,
    onRemoveBinding,
    onSetTarget,
    onForget,
  }: {
    devices: UsbDeviceInfo[];
    config: BridgeConfig | null;
    desktopPeers: DiscoveredPeer[];
    registeredOffline: [string, DeviceConfig][];
    usbAccessDenied: boolean;
    usbFixCommand: string;
    bindingAction: KeyAction | null;
    onCopyFixCommand: () => void;
    onStartBinding: (action: KeyAction) => void;
    onCancelRegistration: () => void;
    onRemoveBinding: (deviceId: string, key: string) => void;
    onSetTarget: (deviceId: string, peer: DiscoveredPeer | null) => void;
    onForget: (deviceId: string, device: DeviceConfig) => void;
  } = $props();
</script>

<section class="card">
  <h2>USB Devices <span class="count">{devices.length}</span></h2>

  {#if usbAccessDenied}
    <div class="perms-banner">
      <div class="perms-head">
        <strong>⚠ No access to USB input devices</strong>
        <span class="muted small">
          The bridge found USB devices it can't read. On Linux your user needs
          to be in the <code>input</code> group to read <code>/dev/input/event*</code>.
          On Raspberry Pi OS this is already the default; on other distros run the
          command below, then log out and back in.
        </span>
      </div>
      <div class="perms-actions">
        <button class="btn-primary" onclick={onCopyFixCommand}>Copy fix command</button>
      </div>
      <details class="perms-manual" open>
        <summary class="muted small">Fix command</summary>
        <pre class="perms-cmd">{usbFixCommand}</pre>
        <span class="muted small">Run in a terminal, then log out and back in.</span>
      </details>
    </div>
  {/if}

  {#if bindingAction}
    <div class="registration-banner">
      <span>Press any key on any device to bind as <strong>{actionLabel(bindingAction)}</strong>…</span>
      <button class="btn-ghost" onclick={onCancelRegistration}>Cancel</button>
    </div>
  {/if}

  {#if devices.length === 0}
    <p class="empty">No USB devices detected. Plug one in and it will appear here.</p>
  {:else}
    <ul class="peer-list device-list">
      {#each devices as device (device.id)}
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
              <button class="btn-tiny" onclick={() => onStartBinding('prev')} disabled={!!bindingAction}>Bind Prev</button>
              <button class="btn-tiny" onclick={() => onStartBinding('next')} disabled={!!bindingAction}>Bind Next</button>
            </div>
          </div>

          {#if bindings.length > 0}
            <div class="usb-bindings">
              {#each bindings as [key, action] (key)}
                <span class="binding-chip">
                  {key} → {actionLabel(action)}
                  <button class="binding-x" title="Remove binding" onclick={() => onRemoveBinding(device.id, key)}>×</button>
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
                  onSetTarget(device.id, peer);
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
            <button class="btn-tiny" onclick={() => onForget(deviceId, device)}>Forget</button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</section>
