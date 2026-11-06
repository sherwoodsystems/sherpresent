<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import Stat from '$lib/components/ui/Stat.svelte';
  import type { LiveStatus } from '../types';

  interface Props {
    status: LiveStatus | null;
    /** The current slide's presenter notes */
    notes?: string;
    adapter: string;
    onprev?: () => void;
    onnext?: () => void;
    ongoto?: (slide: number) => void;
    /** Next screenful of the current slide's notes in the notes views */
    onpagenotes?: () => void;
  }

  let { status, notes, adapter, onprev, onnext, ongoto, onpagenotes }: Props = $props();

  let gotoValue = $state('');

  const supportsZoom = $derived(adapter === 'powerpoint');

  const adapterLabels: Record<string, string> = {
    powerpoint: 'PowerPoint',
    keynote: 'Keynote',
    libreoffice: 'LibreOffice Impress',
    canva: 'Canva'
  };
  const adapterLabel = $derived(adapterLabels[adapter] ?? adapter);
</script>

{#if !status}
  <p class="empty">Waiting for status...</p>
{:else if !status.is_open}
  <p class="empty">{adapterLabel} is not open — open it to control your presentation</p>
{:else if !status.is_presenting}
  <p class="empty">Not presenting</p>
{:else}
  <div class="stats">
    <Stat label="Slide">
      {status.current_slide} / {status.total_slides}
      {#snippet below()}
        {#if status?.total_builds && status.total_builds > 0}
          <span class="build-indicator">
            {#each { length: status.total_builds }, i (i)}
              <span class="build-dot" class:filled={i < (status.current_build ?? 0)}></span>
            {/each}
            <span class="build-count">({status.current_build ?? 0}/{status.total_builds})</span>
          </span>
        {/if}
      {/snippet}
    </Stat>

    <Stat label="Status" tone="ok">Presenting</Stat>

    {#if supportsZoom && status.zoom_level !== null}
      <Stat label="Zoom">{status.zoom_level}%</Stat>
    {/if}
  </div>

  <div class="controls">
    <Button onclick={onprev} disabled={!status.current_slide || status.current_slide <= 1}>
      ← Prev
    </Button>
    <Button
      onclick={onnext}
      disabled={!status.current_slide || status.current_slide >= status.total_slides}
    >
      Next →
    </Button>
    <Button
      onclick={onpagenotes}
      title="Next screenful of notes in the stage view and Syphon notes; wraps to the top"
    >
      Page Notes ↓
    </Button>

    {#if ongoto}
      <div class="goto-controls">
        <input
          type="number"
          class="input goto-input"
          aria-label="Go to slide"
          min="1"
          max={status.total_slides}
          bind:value={gotoValue}
          placeholder="#"
          onkeydown={(e) => {
            if (e.key === 'Enter' && gotoValue) {
              ongoto(Number(gotoValue));
              gotoValue = '';
            }
          }}
        />
        <Button
          onclick={() => {
            if (gotoValue) {
              ongoto(Number(gotoValue));
              gotoValue = '';
            }
          }}
          disabled={!gotoValue}
        >
          Go
        </Button>
      </div>
    {/if}
  </div>

  {#if notes}
    <div class="presenter-notes">
      <span class="label">Notes</span>
      <p class="notes-text">{notes}</p>
    </div>
  {/if}
{/if}

<style>
  .controls {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.5rem;
  }

  .goto-controls {
    display: flex;
    gap: 0.25rem;
  }

  .goto-input {
    width: 3.5rem;
    padding: 0 0.25rem;
    text-align: center;
    appearance: textfield;
  }

  .goto-input::-webkit-inner-spin-button,
  .goto-input::-webkit-outer-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }

  .build-indicator {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    margin-top: 0.125rem;
  }

  .build-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--border);
    transition: background 0.15s;
  }

  .build-dot.filled {
    background: var(--accent);
  }

  .build-count {
    font-size: 0.6875rem;
    color: var(--text-muted);
    margin-left: 0.125rem;
  }

  .presenter-notes {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--border-subtle);
  }

  .notes-text {
    margin: 0;
    font-size: 0.875rem;
    color: var(--text);
    white-space: pre-wrap;
    line-height: 1.4;
  }
</style>
