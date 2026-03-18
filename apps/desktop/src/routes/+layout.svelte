<script lang="ts">
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
    <a href="/bridges" class="nav-link" class:active={page.url.pathname === '/bridges'}>Bridges</a>
    <a href="/settings" class="nav-link" class:active={page.url.pathname === '/settings'}>Settings</a>
  </nav>
  <div class="page-content">
    {@render children()}
  </div>

  <div class="help-corner">
    {#if helpOpen}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="help-backdrop" onclick={() => helpOpen = false} onkeydown={() => {}}></div>
      <div class="help-popover">
        <button class="help-item" onclick={goDebug}>Debug</button>
        <a class="help-item" href="mailto:cameron@sherwoodsystems.com">Contact Support</a>
      </div>
    {/if}
    <button class="help-btn" onclick={toggleHelp} class:active={helpOpen}>?</button>
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen,
      Ubuntu, Cantarell, 'Open Sans', 'Helvetica Neue', sans-serif;
    background: #fafafa;
    color: #333;
  }

  .app-shell {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
  }

  .nav-bar {
    display: flex;
    gap: 0;
    background: #fff;
    border-bottom: 1px solid #e0e0e0;
    padding: 0 1rem;
  }

  .nav-link {
    padding: 0.75rem 1rem;
    text-decoration: none;
    font-size: 0.875rem;
    font-weight: 500;
    color: #666;
    border-bottom: 2px solid transparent;
    transition: all 0.15s ease;
  }

  .nav-link:hover {
    color: #333;
  }

  .nav-link.active {
    color: #1a73e8;
    border-bottom-color: #1a73e8;
  }

  .page-content {
    flex: 1;
    max-width: 900px;
    margin: 0 auto;
    width: 100%;
    padding: 1.5rem;
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
    border: 1px solid #ddd;
    background: #fff;
    color: #666;
    font-size: 0.9rem;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .help-btn:hover, .help-btn.active {
    background: #f0f0f0;
    border-color: #ccc;
    color: #333;
  }

  .help-backdrop {
    position: fixed;
    inset: 0;
  }

  .help-popover {
    position: absolute;
    bottom: 2.5rem;
    right: 0;
    background: #fff;
    border: 1px solid #ddd;
    border-radius: 8px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.1);
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
    color: #333;
    cursor: pointer;
    transition: background 0.1s ease;
    font: inherit;
    box-sizing: border-box;
  }

  .help-item:hover {
    background: #f5f5f5;
  }

  @media (prefers-color-scheme: dark) {
    :global(body) {
      background: #1a1a1a;
      color: #eee;
      color-scheme: dark;
    }

    .nav-bar {
      background: #2a2a2a;
      border-bottom-color: #444;
    }

    .nav-link {
      color: #888;
    }

    .nav-link:hover {
      color: #eee;
    }

    .nav-link.active {
      color: #6ab7ff;
      border-bottom-color: #6ab7ff;
    }

    .help-btn {
      background: #333;
      border-color: #555;
      color: #aaa;
    }

    .help-btn:hover, .help-btn.active {
      background: #444;
      border-color: #666;
      color: #eee;
    }

    .help-popover {
      background: #2a2a2a;
      border-color: #444;
      box-shadow: 0 4px 12px rgba(0,0,0,0.4);
    }

    .help-item {
      color: #ccc;
    }

    .help-item:hover {
      background: #333;
    }
  }
</style>
