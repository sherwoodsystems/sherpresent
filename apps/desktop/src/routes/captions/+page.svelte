<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import Card from '$lib/components/ui/Card.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import Notice from '$lib/components/ui/Notice.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import Stat from '$lib/components/ui/Stat.svelte';
  import { onMount } from 'svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { appStore } from '$lib/state.svelte';
  import ConnectionInfo from '$lib/components/ConnectionInfo.svelte';
  import CaptionsConfig from '$lib/components/CaptionsConfig.svelte';
  import { API_KEY_LABELS, apiKeyProvider, translates } from '$lib/captions';

  let startError = $state<string | null>(null);
  let busy = $state(false);

  onMount(async () => {
    await appStore.loadAudioDevices();
    await appStore.refreshCaptionStatus();
  });

  const captions = $derived(appStore.config.captions);
  const status = $derived(appStore.captionStatus);

  const engineState = $derived.by(() => {
    const s = status?.state;
    if (!s) return 'stopped';
    return typeof s === 'object' ? 'error' : s;
  });

  const errorMessage = $derived.by(() => {
    const s = status?.state;
    return s && typeof s === 'object' ? s.error : null;
  });

  // On-device providers need no key, so they must not be gated on one —
  // otherwise Start is permanently disabled.
  const keyProvider = $derived(apiKeyProvider(captions.provider));
  const hasKey = $derived(!keyProvider || captions.apiKeys[keyProvider].length > 0);
  const isOnDevice = $derived(keyProvider === null);

  // Gemini Live translate bills by audio token — roughly $0.037/min at the
  // preview rate. Shown so an operator can see burn without leaving the app.
  const COST_PER_MINUTE = 0.037;
  const elapsed = $derived(status?.elapsedSeconds ?? 0);
  const elapsedLabel = $derived.by(() => {
    const m = Math.floor(elapsed / 60);
    const s = elapsed % 60;
    return `${m}:${String(s).padStart(2, '0')}`;
  });
  const estimatedCost = $derived(((elapsed / 60) * COST_PER_MINUTE).toFixed(2));

  // Newest first reads better in a monitor: the live line stays pinned at top
  // instead of walking off the bottom of the panel.
  const displayed = $derived([...appStore.captionSegments].reverse());

  function setDevice(value: string) {
    appStore.updateCaptionsConfig({ ...captions, inputDevice: value || null });
  }

  async function start() {
    busy = true;
    startError = await appStore.startCaptions();
    busy = false;
  }

  async function stop() {
    busy = true;
    startError = null;
    await appStore.stopCaptions();
    busy = false;
  }

  async function openOverlay() {
    if (appStore.captionsUrl) await openUrl(appStore.captionsUrl);
  }

  const stateTone = $derived.by(() => {
    if (engineState === 'running') return 'ok';
    if (engineState === 'reconnecting' || engineState === 'starting') return 'warn';
    if (engineState === 'error') return 'bad';
    return undefined;
  });
</script>

<main class="page">
  <PageHeader title="Live Captions" />

  <Card>
    <div class="control-row">
      <div class="grow">
        <Field label="Audio Input" for="cap-device">
          <select
            id="cap-device"
            class="input"
            value={captions.inputDevice ?? ''}
            disabled={appStore.captionsRunning}
            onchange={(e) => setDevice(e.currentTarget.value)}
          >
            <option value="">System default</option>
            {#each appStore.audioDevices as d (d.name)}
              <option value={d.name}>
                {d.name} — {(d.sampleRate / 1000).toFixed(1)} kHz, {d.channels}ch
              </option>
            {/each}
          </select>
        </Field>
      </div>

      <Button onclick={() => appStore.loadAudioDevices()} disabled={appStore.captionsRunning}>
        Rescan
      </Button>

      {#if appStore.captionsRunning}
        <Button variant="danger" onclick={stop} disabled={busy}>Stop</Button>
      {:else}
        <Button variant="success" onclick={start} disabled={busy || !hasKey}>Start</Button>
      {/if}
    </div>

    {#if !hasKey}
      <Notice>
        No API key for {keyProvider ? API_KEY_LABELS[keyProvider] : ''} — add one in
        <a href="#caption-settings">Caption Settings</a> below first.
      </Notice>
    {/if}

    {#if startError}
      <Notice tone="error">{startError}</Notice>
    {/if}
    {#if errorMessage}
      <Notice tone="error">{errorMessage}</Notice>
    {/if}

    <div class="stats">
      <Stat label="State" tone={stateTone}><span class="state">{engineState}</span></Stat>
      <Stat label="Elapsed">{elapsedLabel}</Stat>
      <!-- A dollar figure for a local model would be actively misleading. -->
      <Stat label="Est. Cost">{isOnDevice ? 'On-device — free' : `$${estimatedCost}`}</Stat>
      <Stat label="Reconnects">{status?.reconnects ?? 0}</Stat>
    </div>
  </Card>

  {#if appStore.captionsUrl}
    <ConnectionInfo
      label="Overlay available at:"
      value={appStore.captionsUrl}
      onopen={openOverlay}
    />
  {/if}

  <Card title="Monitor">
    {#snippet aside()}
      <span class="lang-tag">
        {translates(captions)
          ? captions.targetLanguage
          : `${captions.sourceLanguage} · no translation`}
      </span>
    {/snippet}

    {#if displayed.length === 0}
      <p class="empty">
        {appStore.captionsRunning ? 'Listening…' : 'Not running.'}
      </p>
    {:else}
      <ul class="lines">
        {#each displayed as seg (seg.id)}
          <li class="line" class:interim={!seg.final}>
            <!-- Straight captions carry no translation: the source is the line. -->
            <span class="translated">{seg.translated || seg.source || '…'}</span>
            {#if seg.translated && seg.source}
              <span class="source">{seg.source}</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </Card>

  {#if appStore.configLoaded}
    <div id="caption-settings">
      <CaptionsConfig
        config={appStore.config.captions}
        onchange={(c) => appStore.updateCaptionsConfig(c)}
      />
    </div>
  {/if}
</main>

<style>
  .lang-tag {
    font-size: 0.7rem;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--link);
    background: var(--info-bg);
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
  }

  .state {
    text-transform: capitalize;
  }

  .lines {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    max-height: 24rem;
    overflow-y: auto;
  }

  .line {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.5rem 0.75rem;
    background: var(--subtle);
    border-radius: 8px;
    border-left: 3px solid transparent;
  }

  .line.interim {
    border-left-color: var(--link);
    opacity: 0.85;
  }

  .translated {
    font-size: 0.95rem;
    color: var(--text);
  }

  .source {
    font-size: 0.75rem;
    color: var(--text-muted);
  }
</style>
