<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import Notice from '$lib/components/ui/Notice.svelte';
  import { untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  interface Props {
    adapter: string;
    selectedPresentation: string;
    onselect: (name: string) => void;
  }

  let { adapter, selectedPresentation, onselect }: Props = $props();

  let presentations = $state<string[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function refreshPresentations() {
    loading = true;
    error = null;
    try {
      presentations = await invoke('get_open_presentations', { adapter });
    } catch (e) {
      error = e as string;
      presentations = [];
    } finally {
      loading = false;
    }
  }

  // Refresh when adapter changes
  $effect(() => {
    void adapter; // Track dependency
    refreshPresentations();
  });

  // The backend can select a presentation by itself (the only running
  // show), possibly one opened after the list was fetched.
  $effect(() => {
    if (
      selectedPresentation &&
      !untrack(() => loading || presentations.includes(selectedPresentation))
    ) {
      refreshPresentations();
    }
  });
</script>

<div class="control-row">
  <select
    class="input grow"
    aria-label="Presentation"
    value={selectedPresentation}
    onchange={(e) => onselect(e.currentTarget.value)}
    disabled={loading}
  >
    <option value="">Select a presentation...</option>
    {#each presentations as pres (pres)}
      <option value={pres}>{pres}</option>
    {/each}
  </select>
  <Button
    onclick={refreshPresentations}
    disabled={loading}
    title="Refresh presentation list"
    aria-label="Refresh presentation list"
  >
    {loading ? '…' : '↻'}
  </Button>
</div>
{#if error}
  <Notice tone="error">{error}</Notice>
{/if}
{#if !loading && presentations.length === 0 && !error}
  <p class="empty">
    No presentations open in {adapter === 'powerpoint'
      ? 'PowerPoint'
      : adapter === 'libreoffice'
        ? 'LibreOffice Impress'
        : 'Keynote'}
  </p>
{/if}
