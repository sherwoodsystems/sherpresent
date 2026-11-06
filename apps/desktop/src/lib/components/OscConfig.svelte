<script lang="ts">
  import Field from '$lib/components/ui/Field.svelte';
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

{#if localIp}
  <ConnectionInfo label="Remote clients connect to:" value="{localIp}:{config.receivePort}" />
{/if}

<div class="config-grid">
  <Field label="Receive Port" for="receive-port" span>
    <input
      id="receive-port"
      type="number"
      class="input mono"
      value={config.receivePort}
      min="1"
      max="65535"
      onchange={(e) => updateField('receivePort', parseInt(e.currentTarget.value) || 9000)}
    />
  </Field>

  <Field
    label="Extra Feedback Host"
    for="feedback-host"
    hint="Only for devices that listen without sending. Anything that sends commands here gets feedback automatically."
  >
    <input
      id="feedback-host"
      type="text"
      class="input mono"
      value={config.feedbackHost}
      onchange={(e) => updateField('feedbackHost', e.currentTarget.value)}
    />
  </Field>

  <Field label="Extra Feedback Port" for="feedback-port">
    <input
      id="feedback-port"
      type="number"
      class="input mono"
      value={config.feedbackPort}
      min="1"
      max="65535"
      onchange={(e) => updateField('feedbackPort', parseInt(e.currentTarget.value) || 9001)}
    />
  </Field>
</div>
