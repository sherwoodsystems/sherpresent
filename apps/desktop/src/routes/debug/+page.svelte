<script lang="ts">
  import { appStore } from '$lib/state.svelte';
  import LatencyView from '$lib/components/LatencyView.svelte';
</script>

<LatencyView
  events={appStore.latencyEvents}
  onclear={() => appStore.clearLatencyEvents()}
/>
