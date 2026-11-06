<script lang="ts">
  import '../app.css';
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { appStore } from '$lib/state.svelte';

  let { children } = $props();
  let helpOpen = $state(false);

  onMount(() => {
    appStore.init();
  });

  onDestroy(() => {
    appStore.destroy();
  });

  function toggleHelp() {
    helpOpen = !helpOpen;
  }

  function goDebug() {
    helpOpen = false;
    goto('/debug');
  }
</script>

<div class="app-shell">
  <nav class="nav-bar">
    <a href="/" class="nav-link" class:active={page.url.pathname === '/'}>Control</a>
    <a href="/notes" class="nav-link" class:active={page.url.pathname === '/notes'}>Notes</a>
    <a href="/captions" class="nav-link" class:active={page.url.pathname === '/captions'}
      >Captions</a
    >
    <a href="/qlab" class="nav-link" class:active={page.url.pathname === '/qlab'}>QLab</a>
    <a href="/settings" class="nav-link" class:active={page.url.pathname === '/settings'}
      >Settings</a
    >
  </nav>
  <div class="page-content">
    {@render children()}
  </div>

  <div class="help-corner">
    {#if helpOpen}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="help-backdrop" onclick={() => (helpOpen = false)} onkeydown={() => {}}></div>
      <div class="help-popover">
        <button class="help-item" onclick={goDebug}>Debug</button>
        <a class="help-item" href="mailto:cameron@sherwoodsystems.com">Contact Support</a>
      </div>
    {/if}
    <button class="help-btn" onclick={toggleHelp} class:active={helpOpen}>?</button>
  </div>
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
  }

  .nav-bar {
    display: flex;
    gap: 0;
    background: var(--surface);
    border-bottom: 1px solid var(--divider);
    padding: 0 1rem;
  }

  .nav-link {
    padding: 0.75rem 1rem;
    text-decoration: none;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--text-muted);
    border-bottom: 2px solid transparent;
    transition: all 0.15s ease;
  }

  .nav-link:hover {
    color: var(--text);
  }

  .nav-link.active {
    color: var(--link);
    border-bottom-color: var(--link);
  }

  .page-content {
    flex: 1;
    max-width: 900px;
    margin: 0 auto;
    width: 100%;
    /* Room at the bottom so the ? button never sits on the last control. */
    padding: 1.5rem 1.5rem 4rem;
    box-sizing: border-box;
  }

  .help-corner {
    position: fixed;
    bottom: 1rem;
    right: 1rem;
    z-index: 100;
  }

  .help-btn {
    width: 2rem;
    height: 2rem;
    border-radius: 50%;
    border: 1px solid var(--border);
    background: var(--input-bg);
    color: var(--text-secondary);
    font-size: 0.9rem;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .help-btn:hover,
  .help-btn.active {
    background: var(--hover);
    border-color: var(--border-strong);
    color: var(--text);
  }

  .help-backdrop {
    position: fixed;
    inset: 0;
  }

  .help-popover {
    position: absolute;
    bottom: 2.5rem;
    right: 0;
    background: var(--surface);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    box-shadow: var(--shadow-card);
    overflow: hidden;
    min-width: 160px;
  }

  .help-item {
    display: block;
    width: 100%;
    padding: 0.6rem 1rem;
    font-size: 0.8rem;
    text-align: left;
    text-decoration: none;
    border: none;
    background: none;
    color: var(--text);
    cursor: pointer;
    transition: background 0.1s ease;
    font: inherit;
    box-sizing: border-box;
  }

  .help-item:hover {
    background: var(--hover);
  }
</style>
