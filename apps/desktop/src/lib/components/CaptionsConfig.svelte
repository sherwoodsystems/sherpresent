<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { appStore } from '$lib/state.svelte';
  import SyphonOutputField from './SyphonOutputField.svelte';
  import { API_KEY_LABELS, CAPTION_LANGUAGES, apiKeyProvider, translates } from '../captions';
  import type { AppleCaptionSupport, CaptionsConfig, CaptionProviderId } from '../types';

  interface Props {
    config: CaptionsConfig;
    onchange: (config: CaptionsConfig) => void;
  }

  let { config, onchange }: Props = $props();

  function update<K extends keyof CaptionsConfig>(field: K, value: CaptionsConfig[K]) {
    onchange({ ...config, [field]: value });
  }

  /**
   * Overlay styling: push to open overlays ahead of the debounced save, so the
   * overlay tracks the control in real time. Coalesced to one preview per
   * animation frame — a drag fires far more input events than the overlay can
   * show — and fire-and-forget, since the next frame or the save supersedes it.
   */
  let pendingPreview: CaptionsConfig | null = null;
  function live<K extends keyof CaptionsConfig>(field: K, value: CaptionsConfig[K]) {
    const next = { ...config, [field]: value };
    if (pendingPreview === null) {
      requestAnimationFrame(() => {
        invoke('preview_caption_overlay', { captions: pendingPreview }).catch(() => {});
        pendingPreview = null;
      });
    }
    pendingPreview = next;
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

  const syphonStatus = $derived(appStore.outputsStatus?.captions);

  const keyProvider = $derived(apiKeyProvider(config.provider));
  function updateKey(value: string) {
    if (keyProvider) onchange({ ...config, apiKeys: { ...config.apiKeys, [keyProvider]: value.trim() } });
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

  // Straight captions are stored as target = source, which is what every
  // backend already treats as "don't translate" (`CaptionsConfig::translates`).
  const translating = $derived(translates(config));
  const NO_TRANSLATION = '';

  /** The offered languages, plus whatever an older config holds, so it isn't
   *  silently replaced just by opening Settings. */
  function languageOptions(current: string | null) {
    return !current || CAPTION_LANGUAGES.some((l) => l.code === current)
      ? CAPTION_LANGUAGES
      : [...CAPTION_LANGUAGES, { code: current, label: current }];
  }

  function updateSource(source: string) {
    // Keep straight captions straight when the spoken language changes.
    const targetLanguage = translating ? config.targetLanguage : source;
    onchange({ ...config, sourceLanguage: source, targetLanguage });
  }

  function updateTarget(target: string) {
    const source = config.sourceLanguage ?? CAPTION_LANGUAGES[0].code;
    onchange({ ...config, sourceLanguage: source, targetLanguage: target || source });
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
      <label class="label" for="cap-source">Spoken Language</label>
      <select
        id="cap-source"
        class="input"
        value={config.sourceLanguage ?? ''}
        onchange={(e) => updateSource(e.currentTarget.value)}
      >
        {#if config.sourceLanguage === null}
          <option value="" disabled>Auto-detect</option>
        {/if}
        {#each languageOptions(config.sourceLanguage) as l (l.code)}
          <option value={l.code}>{l.label}</option>
        {/each}
      </select>
    </div>

    <div class="field">
      <label class="label" for="cap-target">Captions In</label>
      <select
        id="cap-target"
        class="input"
        value={translating ? config.targetLanguage : NO_TRANSLATION}
        onchange={(e) => updateTarget(e.currentTarget.value)}
      >
        <option value={NO_TRANSLATION}>Same language (no translation)</option>
        {#each languageOptions(config.targetLanguage) as l (l.code)}
          {#if !config.sourceLanguage || l.code !== config.sourceLanguage}
            <option value={l.code}>{l.label}</option>
          {/if}
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
          {#if translating}
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
          {/if}
          {#if !appleUsable && apple.message}
            <span class="hint warn">{apple.message}</span>
          {/if}
        {/if}
      </div>
    {/if}

    {#if keyProvider}
      <div class="field span">
        <div class="key-header">
          <label class="label" for="cap-key">{API_KEY_LABELS[keyProvider]} API Key</label>
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
          placeholder={keyProvider === 'openai' ? 'sk-…' : 'AIza…'}
          value={config.apiKeys[keyProvider]}
          onchange={(e) => updateKey(e.currentTarget.value)}
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

    {#snippet colourField(field: 'chromaColor' | 'boxColor', label: string, fallback: string)}
      <div class="field">
        <label class="label" for="cap-{field}">{label}</label>
        <div class="colour-row">
          <input
            id="cap-{field}"
            type="color"
            class="swatch"
            value={config[field].startsWith('#') ? config[field] : fallback}
            oninput={(e) => live(field, e.currentTarget.value)}
          />
          <input
            type="text"
            class="input mono"
            aria-label="{label} hex value"
            value={config[field]}
            onchange={(e) => live(field, e.currentTarget.value.trim() || fallback)}
          />
        </div>
      </div>
    {/snippet}

    {@render colourField('chromaColor', 'Key Colour', '#00B140')}
    {#if config.background}
      {@render colourField('boxColor', 'Box Colour', '#000000')}
    {/if}

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
        <span class="label">Background box</span>
      </label>
    </div>
  </div>

  {#if syphonStatus?.supported}
    <h3 class="section-title">Outputs</h3>
    <SyphonOutputField
      id="cap-syphon"
      label="Syphon"
      config={config.outputs.syphon}
      status={syphonStatus}
      defaultName="SherPresent Captions"
      onchange={(syphon) => update('outputs', { ...config.outputs, syphon })}
    />
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
