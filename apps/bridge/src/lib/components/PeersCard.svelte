<script lang="ts">
  import type { DiscoveredPeer, KeyAction } from '$lib/types';

  let {
    peers,
    sendingTest,
    onTest,
  }: {
    peers: DiscoveredPeer[];
    /** Keyed by `${instanceId}-${command}` — true while a test send is in flight. */
    sendingTest: Record<string, boolean>;
    onTest: (peer: DiscoveredPeer, command: KeyAction) => void;
  } = $props();
</script>

<section class="card">
  <h2>Discovered Desktops <span class="count">{peers.length}</span></h2>
  {#if peers.length === 0}
    <p class="empty">
      No SherPresent desktops found on the LAN.
      <span class="muted">Ensure desktop apps are running and on the same subnet.</span>
    </p>
  {:else}
    <ul class="peer-list">
      {#each peers as peer (peer.instanceId)}
        <li class="peer">
          <span class="peer-dot" aria-hidden="true"></span>
          <span class="peer-name">{peer.displayName ?? '(unnamed)'}</span>
          <span class="peer-host">{peer.host}:{peer.port}</span>
          <span class="peer-id">#{peer.displayId}</span>
          <div class="peer-actions">
            <button
              class="btn-tiny"
              disabled={sendingTest[`${peer.instanceId}-prev`]}
              onclick={() => onTest(peer, 'prev')}
            >
              Test Prev
            </button>
            <button
              class="btn-tiny"
              disabled={sendingTest[`${peer.instanceId}-next`]}
              onclick={() => onTest(peer, 'next')}
            >
              Test Next
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>
