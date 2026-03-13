<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { BridgeRegistrationStatus, BridgeApiResponse } from '$lib/types';

  let {
    targetSlot,
    host,
    configPort,
    validChannels,
    oncomplete,
    oncancel,
  }: {
    targetSlot: string;
    host: string;
    configPort: number;
    validChannels: string[];
    oncomplete: () => void;
    oncancel: () => void;
  } = $props();

  let status = $state<BridgeRegistrationStatus | null>(null);
  let countdown = $state(30);
  let channel = $state('main');
  let label = $state(targetSlot.replace('_', ' '));
  let confirming = $state(false);
  let message = $state<string | null>(null);
  let timedOut = $state(false);

  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let countdownTimer: ReturnType<typeof setInterval> | null = null;

  let detected = $derived(!!status?.detected_phys);

  onMount(async () => {
    try {
      await invoke<BridgeApiResponse>('bridge_start_registration', {
        host, configPort, slot: targetSlot,
      });
    } catch (e) {
      message = `Failed to start registration: ${e}`;
      return;
    }

    pollTimer = setInterval(pollRegistration, 1000);
    countdownTimer = setInterval(() => {
      countdown--;
      if (countdown <= 0) {
        timedOut = true;
        cleanup();
        cancelRegistration();
      }
    }, 1000);
  });

  onDestroy(() => {
    cleanup();
  });

  function cleanup() {
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
    if (countdownTimer) { clearInterval(countdownTimer); countdownTimer = null; }
  }

  async function pollRegistration() {
    try {
      status = await invoke<BridgeRegistrationStatus>('bridge_get_registration_status', {
        host, configPort,
      });
      if (status.detected_phys) {
        // Device detected, stop countdown
        if (countdownTimer) { clearInterval(countdownTimer); countdownTimer = null; }
      }
    } catch { /* ignore */ }
  }

  async function confirm() {
    if (!status?.detected_phys) return;
    confirming = true;
    try {
      const result = await invoke<BridgeApiResponse>('bridge_confirm_registration', {
        host, configPort,
        slot: targetSlot,
        usbPhys: status.detected_phys,
        channel,
        label,
      });
      message = result.message;
      cleanup();
      oncomplete();
    } catch (e) {
      message = `Error: ${e}`;
      confirming = false;
    }
  }

  async function cancelRegistration() {
    try {
      await invoke('bridge_cancel_registration', { host, configPort });
    } catch { /* ignore */ }
    cleanup();
    oncancel();
  }
</script>

<div class="registration">
  <div class="reg-header">
    <h4>Register Device to {targetSlot.replace('_', ' ')}</h4>
    <button class="btn-cancel" onclick={cancelRegistration}>Cancel</button>
  </div>

  {#if timedOut}
    <p class="timeout-message">Registration timed out. No device detected.</p>
  {:else if !detected}
    <div class="detect-prompt">
      <p>Press any button on the USB clicker...</p>
      <div class="countdown">{countdown}s</div>
    </div>
  {:else}
    <div class="detected-form">
      <p class="detected-info">
        Detected: <strong>{status?.detected_name || status?.detected_phys}</strong>
      </p>
      <div class="form-group">
        <label for="reg-label">Label</label>
        <input id="reg-label" type="text" bind:value={label} />
      </div>
      <div class="form-group">
        <label for="reg-channel">Channel</label>
        <select id="reg-channel" bind:value={channel}>
          {#each validChannels as ch}
            <option value={ch}>{ch}</option>
          {/each}
        </select>
      </div>
      <button class="btn-confirm" onclick={confirm} disabled={confirming}>
        {confirming ? 'Confirming...' : 'Confirm Registration'}
      </button>
    </div>
  {/if}

  {#if message}
    <p class="reg-message">{message}</p>
  {/if}
</div>

<style>
  .registration {
    padding: 0.75rem;
    background: #fff8e1;
    border: 1px solid #ffe082;
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .reg-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .reg-header h4 {
    margin: 0;
    font-size: 0.85rem;
  }

  .btn-cancel {
    padding: 0.25rem 0.5rem;
    font-size: 0.75rem;
    border: 1px solid #ddd;
    border-radius: 4px;
    background: #fff;
    cursor: pointer;
    font-family: inherit;
  }

  .detect-prompt {
    text-align: center;
    padding: 1rem 0;
  }

  .detect-prompt p {
    margin: 0 0 0.5rem;
    font-weight: 500;
  }

  .countdown {
    font-size: 1.5rem;
    font-weight: 700;
    color: #f57c00;
  }

  .timeout-message {
    text-align: center;
    color: #c62828;
    margin: 0.5rem 0;
  }

  .detected-form {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .detected-info {
    margin: 0;
    font-size: 0.85rem;
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
  }

  .btn-confirm {
    padding: 0.5rem;
    background: #2e7d32;
    color: #fff;
    border: none;
    border-radius: 6px;
    font-size: 0.85rem;
    font-weight: 500;
    cursor: pointer;
    font-family: inherit;
  }
  .btn-confirm:hover { background: #1b5e20; }
  .btn-confirm:disabled { background: #999; cursor: not-allowed; }

  .reg-message {
    margin: 0;
    font-size: 0.75rem;
    color: #888;
  }

  @media (prefers-color-scheme: dark) {
    .registration { background: #3a3520; border-color: #5a4a20; }
    .reg-header h4 { color: #eee; }
    .btn-cancel { background: #444; border-color: #555; color: #eee; }
    .countdown { color: #ffb74d; }
    .detected-info { color: #81c784; }
    .form-group label { color: #aaa; }
    .form-group input, .form-group select {
      background: #333; border-color: #555; color: #eee;
    }
  }
</style>
