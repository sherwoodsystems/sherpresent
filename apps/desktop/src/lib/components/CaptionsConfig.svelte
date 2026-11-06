<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import Card from '$lib/components/ui/Card.svelte';
  import Checkbox from '$lib/components/ui/Checkbox.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import StatusLine from '$lib/components/ui/StatusLine.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { appStore } from '$lib/state.svelte';
  import SyphonOutputField from './SyphonOutputField.svelte';
  import {
    API_KEY_LABELS,
    CAPTION_LANGUAGES,
    apiKeyProvider,
    sameLanguage,
    translates
  } from '../captions';
  import { defaultConfig } from '../types';
  import type {
    AppleCaptionSupport,
    CaptionOutputsConfig,
    CaptionsConfig,
    CaptionProviderId,
    CaptionSyphonConfig,
    CaptionText
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
    {
      field: 'safeArea',
      label: 'Bottom Margin',
      min: 0,
      max: 40,
      step: 0.5,
      format: (v) => `${v}%`
    },
    { field: 'width', label: 'Width', min: 20, max: 100, step: 1, format: (v) => `${v}%` },
    {
      field: 'clearAfter',
      label: 'Clear After',
      min: 0,
      max: 30,
      step: 1,
      format: (v) => (v === 0 ? 'Never' : `${v}s`)
    }
  ];

  const syphonStatus = $derived(appStore.outputsStatus?.captions);

  // Two sources so each language can be keyed and placed on its own.
  const syphonOutputs = [
    { key: 'syphon', status: 'captions', label: 'Syphon' },
    { key: 'syphon2', status: 'captions2', label: 'Second Syphon' }
  ] as const;

  function setOutput(key: keyof CaptionOutputsConfig, patch: Partial<CaptionSyphonConfig>) {
    update('outputs', { ...config.outputs, [key]: { ...config.outputs[key], ...patch } });
  }

  const keyProvider = $derived(apiKeyProvider(config.provider));
  function updateKey(value: string) {
    if (keyProvider)
      onchange({ ...config, apiKeys: { ...config.apiKeys, [keyProvider]: value.trim() } });
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

<div class="page">
  <Card title="Source">
    <div class="config-grid">
      <Field label="Provider" for="cap-provider" span>
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
      </Field>

      <Field label="Spoken Language" for="cap-source">
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
      </Field>

      <Field label="Captions In" for="cap-target">
        <select
          id="cap-target"
          class="input"
          value={translating ? config.targetLanguage : NO_TRANSLATION}
          onchange={(e) => updateTarget(e.currentTarget.value)}
        >
          <option value={NO_TRANSLATION}>Same language (no translation)</option>
          {#each languageOptions(config.targetLanguage) as l (l.code)}
            {#if !config.sourceLanguage || !sameLanguage(l.code, config.sourceLanguage)}
              <option value={l.code}>{l.label}</option>
            {/if}
          {/each}
        </select>
      </Field>

      {#if isApple}
        <Field span>
          {#if probing && !apple}
            <span class="hint">Checking this Mac…</span>
          {:else if !apple || !apple.osSupported || !apple.archSupported}
            <span class="hint warn">
              {apple?.message ?? appleDisabledReason ?? 'Not available on this machine.'}
            </span>
          {:else if config.sourceLanguage}
            <StatusLine tone={apple.speechModelInstalled ? 'ok' : 'warn'}>
              Speech model:
              {#if !apple.speechLocaleSupported}
                language not supported
              {:else if apple.speechModelInstalled}
                installed
              {:else}
                downloads on first start
              {/if}
            </StatusLine>
            {#if translating}
              <div class="translation-row">
                <StatusLine tone={apple.translationStatus === 'installed' ? 'ok' : 'warn'}>
                  Translation pack:
                  {#if apple.translationStatus === 'installed'}
                    installed
                  {:else if apple.translationStatus === 'notInstalled'}
                    not installed
                  {:else}
                    language pair not supported
                  {/if}
                </StatusLine>
                {#if apple.translationStatus === 'notInstalled'}
                  <Button variant="link" onclick={openTranslationSettings}>Install…</Button>
                {/if}
              </div>
            {/if}
            {#if !appleUsable && apple.message}
              <span class="hint warn">{apple.message}</span>
            {/if}
          {/if}
        </Field>
      {/if}

      {#if keyProvider}
        <Field
          label="{API_KEY_LABELS[keyProvider]} API Key"
          for="cap-key"
          hint="Stored unencrypted in config.json"
          span
        >
          {#snippet aside()}
            <Button variant="link" onclick={() => (showKeys = !showKeys)}>
              {showKeys ? 'Hide' : 'Show'}
            </Button>
          {/snippet}
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
        </Field>
      {/if}
    </div>
  </Card>

  <Card title="Appearance">
    <div class="config-grid">
      {#each overlaySliders as s (s.field)}
        <Field label={s.label} for="cap-{s.field}">
          {#snippet aside()}
            <span class="slider-value">{s.format(config[s.field])}</span>
          {/snippet}
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
        </Field>
      {/each}

      {#snippet colourField(field: 'chromaColor' | 'boxColor', label: string, fallback: string)}
        <Field {label} for="cap-{field}">
          <div class="control-row">
            <input
              id="cap-{field}"
              type="color"
              class="swatch"
              value={config[field].startsWith('#') ? config[field] : fallback}
              oninput={(e) => live(field, e.currentTarget.value)}
            />
            <input
              type="text"
              class="input mono grow"
              aria-label="{label} hex value"
              value={config[field]}
              onchange={(e) => live(field, e.currentTarget.value.trim() || fallback)}
            />
          </div>
        </Field>
      {/snippet}

      {@render colourField('chromaColor', 'Key Colour', '#00B140')}
      {#if config.background}
        {@render colourField('boxColor', 'Box Colour', '#000000')}
      {/if}

      <div class="check-group">
        <Checkbox
          label="Drop shadow"
          checked={config.shadow}
          onchange={(checked) => live('shadow', checked)}
        />
        <Checkbox
          label="All caps"
          checked={config.uppercase}
          onchange={(checked) => live('uppercase', checked)}
        />
        <Checkbox
          label="Background box"
          checked={config.background}
          onchange={(checked) => live('background', checked)}
        />
      </div>
    </div>
  </Card>

  {#if syphonStatus?.supported}
    <Card title="Outputs">
      {#each syphonOutputs as out (out.key)}
        {@const cfg = config.outputs[out.key]}
        <SyphonOutputField
          id="cap-{out.key}"
          label={out.label}
          config={cfg}
          status={appStore.outputsStatus?.[out.status]}
          defaultName={defaultConfig.captions.outputs[out.key].serverName}
          onchange={(syphon) => setOutput(out.key, syphon)}
        >
          <Field label="Show" for="cap-{out.key}-text">
            <select
              id="cap-{out.key}-text"
              class="input"
              value={cfg.text}
              onchange={(e) => setOutput(out.key, { text: e.currentTarget.value as CaptionText })}
            >
              <option value="translated">Translation</option>
              <option value="source">Original</option>
              <option value="both">Original + translation</option>
            </select>
          </Field>
        </SyphonOutputField>
      {/each}
    </Card>
  {/if}
</div>

<style>
  .hint.warn {
    color: var(--warning);
  }

  .translation-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .check-group {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1.5rem;
  }

  .slider-value {
    font-size: 0.75rem;
    font-family: monospace;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }

  .slider {
    width: 100%;
    accent-color: var(--accent);
  }

  .swatch {
    flex: none;
    box-sizing: border-box;
    width: 3rem;
    height: var(--control-height);
    padding: 2px;
    border: 2px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--input-bg);
  }

  .grow {
    flex: 1;
    min-width: 0;
  }
</style>
