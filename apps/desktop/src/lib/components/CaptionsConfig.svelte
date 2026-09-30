<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import type {
    AppleCaptionSupport,
    CaptionOutputsStatus,
    SyphonOutputConfig,
    CaptionApiKeyProviderId,
    CaptionsConfig,
    CaptionProviderId
  } from '../types';

  interface Props {
    config: CaptionsConfig;
    onchange: (config: CaptionsConfig) => void;
  }

  let { config, onchange }: Props = $props();

  function update<K extends keyof CaptionsConfig>(field: K, value: CaptionsConfig[K]) {
    onchange({ ...config, [field]: value });
  }

  /**
   * Overlay styling: push to open overlays on every input tick, ahead of the
   * debounced save, so the overlay tracks the control in real time. Previews
   * are fire-and-forget — a dropped one is superseded by the next tick or the
   * save that follows.
   */
  function live<K extends keyof CaptionsConfig>(field: K, value: CaptionsConfig[K]) {
    const next = { ...config, [field]: value };
    invoke('preview_caption_overlay', { captions: next }).catch(() => {});
    onchange(next);
  }

  type OverlayField = 'fontSize' | 'maxLines' | 'safeArea' | 'width' | 'clearAfter';
  const overlaySliders: {
    field: OverlayField;
    label: string;
    min: number;
    max: number;
    step: number;
    format: (v: number) => string;
  }[] = [
    { field: 'fontSize', label: 'Font Size', min: 12, max: 240, step: 2, format: (v) => `${v}px` },
    { field: 'maxLines', label: 'Lines', min: 1, max: 6, step: 1, format: (v) => `${v}` },
    { field: 'safeArea', label: 'Bottom Margin', min: 0, max: 40, step: 0.5, format: (v) => `${v}%` },
    { field: 'width', label: 'Width', min: 20, max: 100, step: 1, format: (v) => `${v}%` },
    { field: 'clearAfter', label: 'Clear After', min: 0, max: 30, step: 1, format: (v) => (v === 0 ? 'Never' : `${v}s`) }
  ];

  function updateSyphon(patch: Partial<SyphonOutputConfig>) {
    onchange({
      ...config,
      outputs: { ...config.outputs, syphon: { ...config.outputs.syphon, ...patch } }
    });
  }

  // Output status is pushed from Rust as receivers connect and disconnect.
  let outputs = $state<CaptionOutputsStatus | null>(null);
  $effect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    invoke<CaptionOutputsStatus>('get_caption_outputs_status')
      .then((s) => (outputs = s))
      .catch(() => {});
    listen<CaptionOutputsStatus>('caption-outputs-status', (e) => (outputs = e.payload)).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  const syphon = $derived(outputs?.syphon);
  const syphonSummary = $derived.by(() => {
    if (!syphon) return '';
    switch (syphon.state) {
      case 'running':
        return syphon.hasClients ? 'Receiver connected' : 'No receivers';
      case 'starting':
        return 'Starting…';
      case 'error':
        return syphon.message ?? 'Error';
      default:
        return 'Stopped';
    }
  });

  function updateKey(provider: CaptionApiKeyProviderId, value: string) {
    onchange({ ...config, apiKeys: { ...config.apiKeys, [provider]: value.trim() } });
  }

  // Keys are stored in plaintext in config.json, so the field is masked in the
  // UI only to keep them off a projector during setup — not as protection.
  let showKeys = $state(false);

  const providers: { id: CaptionProviderId; label: string }[] = [
    { id: 'apple', label: 'Apple On-Device' },
    { id: 'gemini', label: 'Google Gemini Live' },
    { id: 'openai', label: 'OpenAI Realtime (not implemented)' }
  ];

  const isApple = $derived(config.provider === 'apple');
  const needsKey = $derived(config.provider === 'gemini' || config.provider === 'openai');

  const languageModes = [
    { id: 'en-fr', source: 'en-US', target: 'fr', label: 'English → French' },
    { id: 'fr-en', source: 'fr', target: 'en-US', label: 'French → English' },
    { id: 'en-only', source: 'en-US', target: 'en-US', label: 'English (captions only)' },
    { id: 'fr-only', source: 'fr', target: 'fr', label: 'French (captions only)' }
  ];
  const selectedLanguageMode = $derived(
    languageModes.find((m) => m.source === config.sourceLanguage && m.target === config.targetLanguage)
      ?.id ?? 'en-fr'
  );
  function updateLanguageMode(id: string) {
    const mode = languageModes.find((m) => m.id === id);
    if (mode) onchange({ ...config, sourceLanguage: mode.source, targetLanguage: mode.target });
  }

  let apple = $state<AppleCaptionSupport | null>(null);
  let probing = $state(false);

  /**
   * Ask the speech helper what it supports.
   *
   * Debounced because the language fields are free text and re-probing on every
   * keystroke would spawn a process per character.
   */
  let probeTimer: ReturnType<typeof setTimeout> | undefined;
  function probeApple(source: string | null, target: string) {
    clearTimeout(probeTimer);
    probeTimer = setTimeout(async () => {
      probing = true;
      try {
        apple = await invoke<AppleCaptionSupport>('check_apple_captions_support', {
          source,
          target
        });
      } catch (e) {
        apple = {
          osSupported: false,
          osVersion: null,
          archSupported: false,
          speechLocaleSupported: false,
          speechModelInstalled: false,
          supportedLocales: [],
          translationStatus: 'unsupported',
          message: String(e)
        };
      } finally {
        probing = false;
      }
    }, 400);
  }

  // Re-probe whenever the pair changes: pack status is per language pair, so a
  // stale answer would be worse than none.
  $effect(() => {
    probeApple(config.sourceLanguage, config.targetLanguage);
  });

  const appleUsable = $derived(
    !!apple && apple.osSupported && apple.archSupported && apple.translationStatus === 'installed'
  );

  // Kept visible-but-disabled rather than hidden: "why can't I pick Apple?" has
  // to have an answer on screen.
  const appleDisabledReason = $derived.by(() => {
    if (!apple) return probing ? 'Checking…' : null;
    if (!apple.osSupported)
      return `Requires macOS 26 (Tahoe) or later${apple.osVersion ? ` — this Mac runs ${apple.osVersion}` : ''}`;
    if (!apple.archSupported) return 'Requires an Apple Silicon Mac';
    return null;
  });

  async function openTranslationSettings() {
    try {
      await invoke('open_translation_settings');
    } catch (e) {
      console.error('Could not open translation settings', e);
    }
  }
</script>

<div class="captions-config">
  <h3 class="section-title">Source</h3>
  <div class="config-grid">
    <div class="field">
      <label class="label" for="cap-provider">Provider</label>
      <select
        id="cap-provider"
        class="input"
        value={config.provider}
        onchange={(e) => update('provider', e.currentTarget.value as CaptionProviderId)}
      >
        {#each providers as p (p.id)}
          <option value={p.id} disabled={p.id === 'apple' && !!appleDisabledReason}>
            {p.label}
          </option>
        {/each}
      </select>
    </div>

    <div class="field">
      <label class="label" for="cap-language-mode">Language</label>
      <select
        id="cap-language-mode"
        class="input"
        value={selectedLanguageMode}
        onchange={(e) => updateLanguageMode(e.currentTarget.value)}
      >
        {#each languageModes as m (m.id)}
          <option value={m.id}>{m.label}</option>
        {/each}
      </select>
    </div>

    {#if isApple}
      <div class="field span">
        {#if probing && !apple}
          <span class="hint">Checking this Mac…</span>
        {:else if !apple || !apple.osSupported || !apple.archSupported}
          <span class="hint warn">
            {apple?.message ?? appleDisabledReason ?? 'Not available on this machine.'}
          </span>
        {:else if config.sourceLanguage}
          <div class="status-row">
            <span class="status-dot" class:ok={apple.speechModelInstalled}></span>
            <span class="status-text">
              Speech model:
              {#if !apple.speechLocaleSupported}
                language not supported
              {:else if apple.speechModelInstalled}
                installed
              {:else}
                downloads on first start
              {/if}
            </span>
          </div>
          <div class="status-row">
            <span class="status-dot" class:ok={apple.translationStatus === 'installed'}></span>
            <span class="status-text">
              Translation pack:
              {#if apple.translationStatus === 'installed'}
                installed
              {:else if apple.translationStatus === 'notInstalled'}
                not installed
              {:else}
                language pair not supported
              {/if}
            </span>
            {#if apple.translationStatus === 'notInstalled'}
              <button type="button" class="reveal" onclick={openTranslationSettings}>
                Install…
              </button>
            {/if}
          </div>
          {#if !appleUsable && apple.message}
            <span class="hint warn">{apple.message}</span>
          {/if}
        {/if}
      </div>
    {/if}

    {#if needsKey}
      <div class="field span">
        <div class="key-header">
          <label class="label" for="cap-key">
            {config.provider === 'openai' ? 'OpenAI' : 'Gemini'} API Key
          </label>
          <button type="button" class="reveal" onclick={() => (showKeys = !showKeys)}>
            {showKeys ? 'Hide' : 'Show'}
          </button>
        </div>
        <input
          id="cap-key"
          type={showKeys ? 'text' : 'password'}
          class="input mono"
          autocomplete="off"
          spellcheck="false"
          placeholder={config.provider === 'openai' ? 'sk-…' : 'AIza…'}
          value={config.provider === 'openai' ? config.apiKeys.openai : config.apiKeys.gemini}
          onchange={(e) =>
            updateKey(config.provider === 'openai' ? 'openai' : 'gemini', e.currentTarget.value)}
        />
        <span class="hint">Stored unencrypted in config.json</span>
      </div>
    {/if}
  </div>

  <h3 class="section-title">Appearance</h3>
  <div class="config-grid">
    {#each overlaySliders as s (s.field)}
      <div class="field">
        <div class="slider-header">
          <label class="label" for="cap-{s.field}">{s.label}</label>
          <span class="slider-value">{s.format(config[s.field])}</span>
        </div>
        <input
          id="cap-{s.field}"
          type="range"
          class="slider"
          min={s.min}
          max={s.max}
          step={s.step}
          value={config[s.field]}
          oninput={(e) => live(s.field, e.currentTarget.valueAsNumber)}
        />
      </div>
    {/each}

    <div class="field">
      <label class="label" for="cap-chroma">Key Colour</label>
      <div class="colour-row">
        <input
          id="cap-chroma"
          type="color"
          class="swatch"
          value={config.chromaColor.startsWith('#') ? config.chromaColor : '#00B140'}
          oninput={(e) => live('chromaColor', e.currentTarget.value)}
        />
        <input
          type="text"
          class="input mono"
          aria-label="Key colour hex value"
          value={config.chromaColor}
          onchange={(e) => live('chromaColor', e.currentTarget.value.trim() || '#00B140')}
        />
      </div>
    </div>

    <div class="field span check-group">
      <label class="check-row">
        <input
          type="checkbox"
          checked={config.shadow}
          onchange={(e) => live('shadow', e.currentTarget.checked)}
        />
        <span class="label">Drop shadow</span>
      </label>
      <label class="check-row">
        <input
          type="checkbox"
          checked={config.background}
          onchange={(e) => live('background', e.currentTarget.checked)}
        />
        <span class="label">Black background box</span>
      </label>
    </div>
  </div>

  {#if syphon?.supported}
    <h3 class="section-title">Outputs</h3>
    <div class="config-grid">
      <div class="field span">
        <label class="check-row">
          <input
            type="checkbox"
            checked={config.outputs.syphon.enabled}
            onchange={(e) => updateSyphon({ enabled: e.currentTarget.checked })}
          />
          <span class="label">Syphon</span>
        </label>
      </div>

      {#if config.outputs.syphon.enabled}
        <div class="field">
          <label class="label" for="cap-syphon-name">Source Name</label>
          <input
            id="cap-syphon-name"
            class="input"
            value={config.outputs.syphon.serverName}
            onchange={(e) =>
              updateSyphon({ serverName: e.currentTarget.value.trim() || 'SherPresent Captions' })}
          />
        </div>
        <div class="field status-field">
          <div class="status-row">
            <span
              class="status-dot"
              class:ok={syphon.state === 'running' && syphon.hasClients}
              class:idle={syphon.state === 'running' && !syphon.hasClients}
              class:bad={syphon.state === 'error'}
            ></span>
            <span class="status-text">{syphonSummary}</span>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .captions-config {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .section-title {
    font-size: 0.875rem;
    font-weight: 600;
    color: #333;
    margin: 0;
    padding-bottom: 0.25rem;
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

  .span {
    grid-column: 1 / -1;
  }

  .label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #666;
  }


  .key-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .reveal {
    background: none;
    border: none;
    padding: 0;
    font-size: 0.7rem;
    color: #007aff;
    cursor: pointer;
  }

  .hint {
    font-size: 0.7rem;
    color: #888;
    margin-top: 0.125rem;
  }

  .hint.warn {
    color: #b8860b;
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }

  .status-dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #b8860b;
  }

  .status-dot.idle {
    background: #8e8e93;
  }

  .status-dot.bad {
    background: #ff3b30;
  }


  .status-dot.ok {
    background: #34c759;
  }

  .status-text {
    font-size: 0.75rem;
    color: #555;
  }

  .input {
    padding: 0.5rem 0.75rem;
    border: 2px solid #ddd;
    border-radius: 6px;
    background: #fff;
    font-size: 0.875rem;
    font-family: inherit;
    width: 100%;
    box-sizing: border-box;
  }

  .mono {
    font-family: monospace;
  }

  .input:focus {
    outline: none;
    border-color: #007aff;
  }

  .colour-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .check-group {
    flex-direction: row;
    flex-wrap: wrap;
    gap: 0.5rem 1.5rem;
  }

  .status-field {
    justify-content: flex-end;
  }

  .slider-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .slider-value {
    font-size: 0.75rem;
    font-family: monospace;
    font-variant-numeric: tabular-nums;
    color: #333;
  }

  .slider {
    width: 100%;
    accent-color: #007aff;
  }

  .swatch {
    width: 3rem;
    height: 2.25rem;
    padding: 2px;
    border: 2px solid #ddd;
    border-radius: 6px;
    background: #fff;
    flex-shrink: 0;
  }

  @media (prefers-color-scheme: dark) {
    .section-title {
      color: #eee;
    }


    .hint {
      color: #777;
    }

    .hint.warn {
      color: #d9a441;
    }

    .status-text {
      color: #bbb;
    }

    .slider-value {
      color: #ddd;
    }


    .status-dot {
      background: #d9a441;
    }

    .label {
      color: #aaa;
    }


    .reveal {
      color: #0a84ff;
    }

    .input,
    .swatch {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .input:focus {
      border-color: #0a84ff;
    }
  }
</style>
