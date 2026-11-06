<script lang="ts">
  import Checkbox from '$lib/components/ui/Checkbox.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import StatusLine from '$lib/components/ui/StatusLine.svelte';
  import type { Snippet } from 'svelte';
  import type { OutputStatus, SyphonOutputConfig } from '../types';

  interface Props {
    /** Prefix for element ids, unique per page */
    id: string;
    label: string;
    config: SyphonOutputConfig;
    status: OutputStatus | undefined;
    /** Used when the name field is cleared */
    defaultName: string;
    onchange: (config: SyphonOutputConfig) => void;
    /** Extra fields shown while the output is enabled */
    children?: Snippet;
  }

  let { id, label, config, status, defaultName, onchange, children }: Props = $props();

  const summary = $derived.by(() => {
    if (!status) return '';
    switch (status.state) {
      case 'running': {
        const receivers = status.hasClients ? 'Receiver connected' : 'No receivers';
        return status.message ? `${status.message} · ${receivers}` : receivers;
      }
      case 'starting':
        return 'Starting…';
      case 'error':
        return status.message ?? 'Error';
      default:
        return 'Stopped';
    }
  });

  const statusTone = $derived.by(() => {
    if (status?.state === 'error') return 'bad';
    if (status?.state !== 'running') return 'warn';
    return status.hasClients ? 'ok' : 'idle';
  });
</script>

<div class="config-grid">
  <Field span>
    <Checkbox
      {label}
      checked={config.enabled}
      onchange={(enabled) => onchange({ ...config, enabled })}
    />
  </Field>

  {#if config.enabled}
    <Field label="Source Name" for="{id}-name">
      <input
        id="{id}-name"
        class="input"
        value={config.serverName}
        onchange={(e) =>
          onchange({ ...config, serverName: e.currentTarget.value.trim() || defaultName })}
      />
    </Field>
    {@render children?.()}
    {#if status}
      <div class="status-field">
        <StatusLine tone={statusTone}>{summary}</StatusLine>
      </div>
    {/if}
  {/if}
</div>

<style>
  /* Lines up with the input beside it. */
  .status-field {
    display: flex;
    align-items: center;
    align-self: end;
    min-height: var(--control-height);
  }
</style>
