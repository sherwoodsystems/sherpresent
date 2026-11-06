<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title?: string;
    description?: string;
    /** Span every column when the page lays cards out in a grid */
    wide?: boolean;
    id?: string;
    /** Beside the title: a tag or a small action */
    aside?: Snippet;
    children: Snippet;
  }

  let { title, description, wide = false, id, aside, children }: Props = $props();
</script>

<section {id} class="card" class:wide>
  {#if title}
    <div class="card-head">
      <h3 class="section-title">{title}</h3>
      {@render aside?.()}
    </div>
  {/if}
  {#if description}
    <p class="hint description">{description}</p>
  {/if}
  {@render children()}
</section>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    min-width: 0;
    padding: 1rem;
    background: var(--surface);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-card);
  }

  .wide {
    grid-column: 1 / -1;
  }

  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .description {
    margin: -0.5rem 0 0;
  }
</style>
