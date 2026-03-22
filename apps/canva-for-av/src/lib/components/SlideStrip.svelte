<script lang="ts">
  import type { Slide } from "$lib/types";
  import { convertFileSrc } from "@tauri-apps/api/core";

  let {
    slides,
    currentIndex = 0,
    onselect,
  }: {
    slides: Slide[];
    currentIndex?: number;
    onselect?: (index: number) => void;
  } = $props();
</script>

<div class="strip">
  {#each slides as slide, i}
    <button
      class="thumb"
      class:active={i === currentIndex}
      onclick={() => onselect?.(i)}
    >
      {#if slide.thumbnail_local}
        <img src={convertFileSrc(slide.thumbnail_local)} alt="Slide {i + 1}" />
      {:else}
        <div class="thumb-placeholder">
          <span>{i + 1}</span>
        </div>
      {/if}
      <span class="thumb-number">{i + 1}</span>
    </button>
  {/each}
</div>

<style>
  .strip {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    padding: 8px 0;
  }

  .thumb {
    flex-shrink: 0;
    width: 120px;
    height: 68px;
    border: 2px solid transparent;
    border-radius: 4px;
    cursor: pointer;
    position: relative;
    overflow: hidden;
    background: #16213e;
    padding: 0;
  }

  .thumb.active {
    border-color: #4fc3f7;
  }

  .thumb:hover {
    border-color: #546e7a;
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #546e7a;
    font-size: 18px;
    font-weight: 600;
  }

  .thumb-number {
    position: absolute;
    bottom: 2px;
    right: 4px;
    font-size: 10px;
    color: #90a4ae;
    background: rgba(0, 0, 0, 0.6);
    padding: 1px 4px;
    border-radius: 2px;
  }
</style>
