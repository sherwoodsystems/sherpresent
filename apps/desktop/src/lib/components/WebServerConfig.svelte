<script lang="ts">
  import Field from '$lib/components/ui/Field.svelte';
  import type { WebServerConfig } from '../types';
  import { appStore } from '$lib/state.svelte';
  import ConnectionInfo from './ConnectionInfo.svelte';
  import SyphonOutputField from './SyphonOutputField.svelte';

  interface Props {
    config: WebServerConfig;
    onchange: (config: WebServerConfig) => void;
  }

  let { config, onchange }: Props = $props();

  const syphonStatus = $derived(appStore.outputsStatus?.notes);
  const slideshowStatus = $derived(appStore.outputsStatus?.slideshow);

  function updateField(field: keyof WebServerConfig, value: string | number | boolean) {
    const updated = { ...config, [field]: value };
    onchange(updated);
  }
</script>

{#if appStore.webServerRunning && appStore.webServerUrl}
  <ConnectionInfo label="Stage view available at:" value={appStore.webServerUrl} />
{/if}

<div class="config-grid">
  <Field label="Port" for="ws-port">
    <input
      id="ws-port"
      type="number"
      class="input mono"
      value={config.port}
      min="1024"
      max="65535"
      onchange={(e) => updateField('port', parseInt(e.currentTarget.value) || 8080)}
    />
  </Field>

  <Field
    label="Stage Font Size"
    for="ws-font-size"
    hint="px — text size for notes in the stage view"
  >
    <input
      id="ws-font-size"
      type="number"
      class="input mono"
      value={config.fontSize}
      min="16"
      max="96"
      step="2"
      onchange={(e) => updateField('fontSize', parseInt(e.currentTarget.value) || 32)}
    />
  </Field>

  <Field label="Ontime Host" for="ontime-host" hint="IP of the machine running Ontime">
    <input
      id="ontime-host"
      type="text"
      class="input mono"
      value={config.ontimeHost}
      placeholder="e.g. 192.168.1.50"
      onchange={(e) => updateField('ontimeHost', e.currentTarget.value)}
    />
  </Field>

  <Field label="Ontime Port" for="ontime-port">
    <input
      id="ontime-port"
      type="number"
      class="input mono"
      value={config.ontimePort}
      min="1"
      max="65535"
      onchange={(e) => updateField('ontimePort', parseInt(e.currentTarget.value) || 4001)}
    />
  </Field>
</div>

{#if syphonStatus?.supported}
  <SyphonOutputField
    id="ws-syphon"
    label="Syphon notes output"
    config={config.syphon}
    status={syphonStatus}
    defaultName="SherPresent Notes"
    onchange={(syphon) => onchange({ ...config, syphon })}
  />
  <span class="hint">
    Current slide's notes, plus the Ontime timer when a host is set, as a 1920×1080 source
  </span>
{/if}

{#if slideshowStatus?.supported}
  <SyphonOutputField
    id="ws-slideshow-syphon"
    label="Syphon slideshow output"
    config={config.slideshowSyphon}
    status={slideshowStatus}
    defaultName="SherPresent Slideshow"
    onchange={(slideshowSyphon) => onchange({ ...config, slideshowSyphon })}
  />
  <span class="hint">
    PowerPoint's slide show window, captured only while presenting; holds the last slide between
    shows. Needs Screen Recording permission.
  </span>
{/if}
