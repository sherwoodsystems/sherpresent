<script lang="ts">
  import { appStore } from '$lib/state.svelte';
  import LatencyView from '$lib/components/LatencyView.svelte';
</script>

<div class="debug-page">
  <LatencyView
    events={appStore.latencyEvents}
    onclear={() => appStore.clearLatencyEvents()}
  />
</div>

<style>
  .debug-page {
    max-width: 800px;
    margin: 0 auto;
  }
</style>
