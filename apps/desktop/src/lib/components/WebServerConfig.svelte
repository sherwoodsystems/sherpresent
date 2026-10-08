<script lang="ts">
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

<div class="ws-config">
  <h3 class="section-title">Stage View Server</h3>
  <p class="description">Always running — serves notes and timer to browsers on your network</p>

  {#if appStore.webServerRunning && appStore.webServerUrl}
    <ConnectionInfo label="Stage view available at:" value={appStore.webServerUrl} />
  {/if}

  <div class="config-grid">
    <div class="field">
      <label class="label" for="ws-port">Port</label>
      <input
        id="ws-port"
        type="number"
        class="input"
        value={config.port}
        min="1024"
        max="65535"
        onchange={(e) => updateField('port', parseInt(e.currentTarget.value) || 8080)}
      />
    </div>

    <div class="field">
      <label class="label" for="ws-font-size">Stage Font Size</label>
      <input
        id="ws-font-size"
        type="number"
        class="input"
        value={config.fontSize}
        min="16"
        max="96"
        step="2"
        onchange={(e) => updateField('fontSize', parseInt(e.currentTarget.value) || 32)}
      />
      <span class="hint">px — text size for notes in the stage view</span>
    </div>

    <div class="field">
      <label class="label" for="ontime-host">Ontime Host</label>
      <input
        id="ontime-host"
        type="text"
        class="input"
        value={config.ontimeHost}
        placeholder="e.g. 192.168.1.50"
        onchange={(e) => updateField('ontimeHost', e.currentTarget.value)}
      />
      <span class="hint">IP of the machine running Ontime</span>
    </div>

    <div class="field">
      <label class="label" for="ontime-port">Ontime Port</label>
      <input
        id="ontime-port"
        type="number"
        class="input"
        value={config.ontimePort}
        min="1"
        max="65535"
        onchange={(e) => updateField('ontimePort', parseInt(e.currentTarget.value) || 4001)}
      />
    </div>
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
</div>

<style>
  .ws-config {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .section-title {
    padding-bottom: 0.25rem;
  }

  .description {
    font-size: 0.75rem;
    color: var(--text-muted);
    margin: 0;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid var(--border-subtle);
  }

  .input {
    font-family: monospace;
  }
</style>
