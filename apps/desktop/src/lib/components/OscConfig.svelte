<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { OscConfig } from '../types';
  import ConnectionInfo from './ConnectionInfo.svelte';

  interface Props {
    config: OscConfig;
    onchange: (config: OscConfig) => void;
  }

  let { config, onchange }: Props = $props();
  let localIp = $state<string | null>(null);

  onMount(async () => {
    try {
      localIp = await invoke<string | null>('get_local_ip');
    } catch (e) {
      console.error('Failed to get local IP:', e);
    }
  });

  function updateField(field: keyof OscConfig, value: string | number) {
    onchange({ ...config, [field]: value });
  }
</script>

<div class="osc-config">
  <h3 class="section-title">OSC Configuration</h3>

  {#if localIp}
    <ConnectionInfo label="Remote clients connect to:" value="{localIp}:{config.receivePort}" />
  {/if}

  <div class="config-grid">
    <div class="field">
      <label class="label" for="receive-port">Receive Port</label>
      <input
        id="receive-port"
        type="number"
        class="input"
        value={config.receivePort}
        min="1"
        max="65535"
        onchange={(e) => updateField('receivePort', parseInt(e.currentTarget.value) || 9000)}
      />
    </div>

    <div class="field">
      <label class="label" for="feedback-port">Feedback Port</label>
      <input
        id="feedback-port"
        type="number"
        class="input"
        value={config.feedbackPort}
        min="1"
        max="65535"
        onchange={(e) => updateField('feedbackPort', parseInt(e.currentTarget.value) || 9001)}
      />
    </div>

    <div class="field">
      <label class="label" for="feedback-host">Feedback Host</label>
      <input
        id="feedback-host"
        type="text"
        class="input"
        value={config.feedbackHost}
        onchange={(e) => updateField('feedbackHost', e.currentTarget.value)}
      />
      <span class="hint">IP of the OSC controller for feedback</span>
    </div>
  </div>
</div>

<style>
  .osc-config {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .section-title {
    font-size: 0.875rem;
    font-weight: 600;
    color: #333;
    margin: 0;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid #eee;
  }


  .hint {
    font-size: 0.7rem;
    color: #888;
    margin-top: 0.125rem;
  }

  .config-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
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
    font-family: monospace;
  }

  .input:focus {
    outline: none;
    border-color: #007aff;
  }

  @media (prefers-color-scheme: dark) {
    .section-title {
      color: #eee;
      border-bottom-color: #444;
    }

    .hint {
      color: #777;
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
  }
</style>
