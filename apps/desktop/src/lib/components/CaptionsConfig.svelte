<script lang="ts">
  import type { CaptionsConfig, CaptionProviderId } from '../types';

  interface Props {
    config: CaptionsConfig;
    onchange: (config: CaptionsConfig) => void;
  }

  let { config, onchange }: Props = $props();

  function update<K extends keyof CaptionsConfig>(field: K, value: CaptionsConfig[K]) {
    onchange({ ...config, [field]: value });
  }

  function updateKey(provider: CaptionProviderId, value: string) {
    onchange({ ...config, apiKeys: { ...config.apiKeys, [provider]: value.trim() } });
  }

  // Keys are stored in plaintext in config.json, so the field is masked in the
  // UI only to keep them off a projector during setup — not as protection.
  let showKeys = $state(false);

  const providers: { id: CaptionProviderId; label: string; note: string }[] = [
    { id: 'gemini', label: 'Google Gemini Live', note: 'Streaming translation in one hop — lowest latency' },
    { id: 'openai', label: 'OpenAI Realtime', note: 'Not implemented yet' }
  ];
</script>

<div class="captions-config">
  <h3 class="section-title">Live Captions</h3>
  <p class="description">Translates speech from a microphone into an overlay for your switcher</p>

  <div class="config-grid">
    <div class="field span">
      <label class="label" for="cap-provider">Provider</label>
      <select
        id="cap-provider"
        class="input"
        value={config.provider}
        onchange={(e) => update('provider', e.currentTarget.value as CaptionProviderId)}
      >
        {#each providers as p (p.id)}
          <option value={p.id}>{p.label}</option>
        {/each}
      </select>
      <span class="hint">{providers.find((p) => p.id === config.provider)?.note ?? ''}</span>
    </div>

    <div class="field span">
      <div class="key-header">
        <span class="label">API Keys</span>
        <button type="button" class="reveal" onclick={() => (showKeys = !showKeys)}>
          {showKeys ? 'Hide' : 'Show'}
        </button>
      </div>

      <label class="sub-label" for="cap-key-gemini">Google Gemini</label>
      <input
        id="cap-key-gemini"
        type={showKeys ? 'text' : 'password'}
        class="input"
        autocomplete="off"
        spellcheck="false"
        placeholder="AIza…"
        value={config.apiKeys.gemini}
        onchange={(e) => updateKey('gemini', e.currentTarget.value)}
      />

      <label class="sub-label" for="cap-key-openai">OpenAI</label>
      <input
        id="cap-key-openai"
        type={showKeys ? 'text' : 'password'}
        class="input"
        autocomplete="off"
        spellcheck="false"
        placeholder="sk-…"
        value={config.apiKeys.openai}
        onchange={(e) => updateKey('openai', e.currentTarget.value)}
      />
      <span class="hint warn">
        Stored unencrypted in config.json — don't sync or share that file
      </span>
    </div>

    <div class="field">
      <label class="label" for="cap-source-lang">Spoken Language</label>
      <input
        id="cap-source-lang"
        type="text"
        class="input"
        placeholder="auto"
        value={config.sourceLanguage ?? ''}
        onchange={(e) => update('sourceLanguage', e.currentTarget.value.trim() || null)}
      />
      <span class="hint">BCP-47 (e.g. en-US). Blank = auto-detect</span>
    </div>

    <div class="field">
      <label class="label" for="cap-target-lang">Caption Language</label>
      <input
        id="cap-target-lang"
        type="text"
        class="input"
        value={config.targetLanguage}
        onchange={(e) => update('targetLanguage', e.currentTarget.value.trim() || 'fr')}
      />
      <span class="hint">BCP-47 (e.g. fr for French)</span>
    </div>

    <div class="field">
      <label class="label" for="cap-font-size">Caption Font Size</label>
      <input
        id="cap-font-size"
        type="number"
        class="input"
        min="12"
        max="240"
        step="2"
        value={config.fontSize}
        onchange={(e) => update('fontSize', parseInt(e.currentTarget.value) || 56)}
      />
      <span class="hint">px at 1080p — scales with the frame</span>
    </div>

    <div class="field">
      <label class="label" for="cap-max-lines">Lines On Screen</label>
      <input
        id="cap-max-lines"
        type="number"
        class="input"
        min="1"
        max="6"
        value={config.maxLines}
        onchange={(e) => update('maxLines', parseInt(e.currentTarget.value) || 2)}
      />
    </div>

    <div class="field span">
      <label class="label" for="cap-chroma">Key Colour</label>
      <div class="colour-row">
        <input
          id="cap-chroma"
          type="color"
          class="swatch"
          value={config.chromaColor.startsWith('#') ? config.chromaColor : '#00B140'}
          onchange={(e) => update('chromaColor', e.currentTarget.value)}
        />
        <input
          type="text"
          class="input"
          aria-label="Key colour hex value"
          value={config.chromaColor}
          onchange={(e) => update('chromaColor', e.currentTarget.value.trim() || '#00B140')}
        />
      </div>
      <span class="hint">#00B140 is broadcast green. Use "transparent" for OBS browser sources</span>
    </div>
  </div>
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

  .description {
    font-size: 0.75rem;
    color: #888;
    margin: 0;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid #eee;
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

  .sub-label {
    font-size: 0.7rem;
    color: #888;
    margin-top: 0.25rem;
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

  .input {
    padding: 0.5rem 0.75rem;
    border: 2px solid #ddd;
    border-radius: 6px;
    background: #fff;
    font-size: 0.875rem;
    font-family: monospace;
    width: 100%;
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

    .description {
      color: #777;
      border-bottom-color: #444;
    }

    .hint {
      color: #777;
    }

    .hint.warn {
      color: #d9a441;
    }

    .label {
      color: #aaa;
    }

    .sub-label {
      color: #777;
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
