<script lang="ts">
  import { onMount } from 'svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { appStore } from '$lib/state.svelte';
  import ConnectionInfo from '$lib/components/ConnectionInfo.svelte';

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

  const hasKey = $derived(
    captions.provider === 'openai'
      ? captions.apiKeys.openai.length > 0
      : captions.apiKeys.gemini.length > 0
  );

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
</script>

<main class="container">
  <header class="header">
    <h1>Live Captions</h1>
    <p class="subtitle">Speech in, translated overlay out</p>
  </header>

  <section class="section">
    <div class="control-row">
      <div class="field grow">
        <label class="label" for="cap-device">Audio Input</label>
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
      </div>

      <button
        class="refresh"
        onclick={() => appStore.loadAudioDevices()}
        disabled={appStore.captionsRunning}
      >
        Rescan
      </button>

      {#if appStore.captionsRunning}
        <button class="action stop" onclick={stop} disabled={busy}>Stop</button>
      {:else}
        <button class="action start" onclick={start} disabled={busy || !hasKey}>Start</button>
      {/if}
    </div>

    {#if !hasKey}
      <p class="notice">
        No API key for {captions.provider === 'openai' ? 'OpenAI' : 'Gemini'} —
        add one in <a href="/settings">Settings</a> first.
      </p>
    {/if}

    {#if startError}
      <p class="notice error">{startError}</p>
    {/if}
    {#if errorMessage}
      <p class="notice error">{errorMessage}</p>
    {/if}

    <div class="stats">
      <div class="stat">
        <span class="stat-label">State</span>
        <span class="stat-value state-{engineState}">{engineState}</span>
      </div>
      <div class="stat">
        <span class="stat-label">Elapsed</span>
        <span class="stat-value">{elapsedLabel}</span>
      </div>
      <div class="stat">
        <span class="stat-label">Est. Cost</span>
        <span class="stat-value">${estimatedCost}</span>
      </div>
      <div class="stat">
        <span class="stat-label">Reconnects</span>
        <span class="stat-value">{status?.reconnects ?? 0}</span>
      </div>
    </div>
    <p class="hint">
      Reconnects are normal — the provider recycles the session every few minutes.
    </p>
  </section>

  {#if appStore.captionsUrl}
    <section class="section">
      <ConnectionInfo label="Overlay available at:" value={appStore.captionsUrl} />
      <div class="overlay-actions">
        <button class="secondary" onclick={openOverlay}>Open Overlay</button>
      </div>
      <p class="hint">
        Add <code>?bg=transparent</code> for an OBS browser source, or
        <code>?size=80&amp;lines=1&amp;safe=8</code> to retune on the fly.
      </p>
    </section>
  {/if}

  <section class="section monitor">
    <h3 class="section-title">
      Monitor <span class="lang-tag">{captions.targetLanguage}</span>
    </h3>

    {#if displayed.length === 0}
      <p class="empty">
        {appStore.captionsRunning ? 'Listening…' : 'Not running.'}
      </p>
    {:else}
      <ul class="lines">
        {#each displayed as seg (seg.id)}
          <li class="line" class:interim={!seg.final}>
            <span class="translated">{seg.translated || '…'}</span>
            {#if seg.source}
              <span class="source">{seg.source}</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</main>

<style>
  .container {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .header {
    text-align: center;
  }

  .header h1 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 700;
    color: #333;
  }

  .subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.875rem;
    color: #888;
  }

  .section {
    background: #fff;
    border-radius: 12px;
    padding: 1rem;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .section-title {
    font-size: 0.875rem;
    font-weight: 600;
    color: #333;
    margin: 0;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .lang-tag {
    font-size: 0.7rem;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #1a73e8;
    background: #e8f4fd;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
  }

  .control-row {
    display: flex;
    align-items: flex-end;
    gap: 0.5rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .grow {
    flex: 1;
    min-width: 0;
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
    width: 100%;
  }

  .input:focus {
    outline: none;
    border-color: #007aff;
  }

  .input:disabled {
    opacity: 0.6;
  }

  .refresh,
  .secondary {
    padding: 0.5rem 0.85rem;
    border: 2px solid #ddd;
    border-radius: 6px;
    background: #fff;
    color: #555;
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
  }

  .refresh:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .action {
    padding: 0.5rem 1.25rem;
    border: none;
    border-radius: 6px;
    color: #fff;
    font-size: 0.875rem;
    font-weight: 600;
    cursor: pointer;
  }

  .action:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .start {
    background: #1a9c4a;
  }

  .stop {
    background: #d93025;
  }

  .notice {
    margin: 0;
    font-size: 0.8rem;
    color: #8a6d00;
    background: #fff8e1;
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
  }

  .notice.error {
    color: #a3271d;
    background: #fdecea;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.5rem;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    background: #f6f6f6;
    border-radius: 8px;
    padding: 0.5rem 0.75rem;
  }

  .stat-label {
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #888;
  }

  .stat-value {
    font-size: 0.95rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: #333;
  }

  .state-running {
    color: #1a9c4a;
  }

  .state-reconnecting,
  .state-starting {
    color: #b8860b;
  }

  .state-error {
    color: #d93025;
  }

  .hint {
    margin: 0;
    font-size: 0.7rem;
    color: #888;
  }

  .hint code {
    background: #f0f0f0;
    padding: 0.05rem 0.25rem;
    border-radius: 3px;
  }

  .overlay-actions {
    display: flex;
    gap: 0.5rem;
  }

  .monitor {
    min-height: 12rem;
  }

  .empty {
    margin: 0;
    color: #888;
    font-style: italic;
    font-size: 0.875rem;
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
    background: #f6f6f6;
    border-radius: 8px;
    border-left: 3px solid transparent;
  }

  .line.interim {
    border-left-color: #1a73e8;
    opacity: 0.85;
  }

  .translated {
    font-size: 0.95rem;
    color: #222;
  }

  .source {
    font-size: 0.75rem;
    color: #888;
  }

  @media (prefers-color-scheme: dark) {
    .header h1,
    .section-title,
    .stat-value {
      color: #eee;
    }

    .subtitle,
    .hint,
    .empty,
    .source,
    .stat-label {
      color: #777;
    }

    .section {
      background: #2a2a2a;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
    }

    .label {
      color: #aaa;
    }

    .input,
    .refresh,
    .secondary {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .input:focus {
      border-color: #0a84ff;
    }

    .lang-tag {
      background: #1a3a5c;
      color: #6ab7ff;
    }

    .stat,
    .line,
    .hint code {
      background: #333;
    }

    .translated {
      color: #eee;
    }

    .notice {
      background: #3a3320;
      color: #d9a441;
    }

    .notice.error {
      background: #3d2320;
      color: #ef6b5f;
    }

    .state-running {
      color: #4ecb71;
    }

    .state-reconnecting,
    .state-starting {
      color: #d9a441;
    }

    .state-error {
      color: #ef6b5f;
    }
  }
</style>
