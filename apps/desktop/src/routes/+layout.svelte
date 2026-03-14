<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/state';
  import { appStore } from '$lib/state.svelte';

  let { children } = $props();

  onMount(() => {
    appStore.init();
  });

  onDestroy(() => {
    appStore.destroy();
  });
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
  }
</style>
