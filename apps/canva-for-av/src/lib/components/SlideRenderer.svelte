<script lang="ts">
  import type { Slide, FontRef } from "$lib/types";
  import { convertFileSrc } from "@tauri-apps/api/core";

  let {
    slide,
    fonts = [],
    scale = 0.5,
  }: { slide: Slide; fonts?: FontRef[]; scale?: number } = $props();

  let scaledWidth = $derived(1920 * scale);
  let scaledHeight = $derived(1080 * scale);

  $effect(() => {
    console.log(`[RENDERER] Rendering slide ${slide.index}: ${slide.elements.length} elements, scale=${scale}`);
    for (const [i, elem] of slide.elements.entries()) {
      console.log(`[RENDERER]   elem[${i}]:`, JSON.stringify(elem).slice(0, 200));
    }
    if (slide.elements.length === 0) {
      console.warn(`[RENDERER] Slide ${slide.index} has NO elements — slide will appear blank`);
    }
  });

  let fontFaceCss = $derived(
    fonts
      .filter((f) => f.local_path)
      .map(
        (f) =>
          `@font-face { font-family: "${f.family}"; src: url("${convertFileSrc(f.local_path!)}"); }`
      )
      .join("\n")
  );

  function assetSrc(localPath: string | null, remoteUrl: string): string {
    if (localPath) {
      return convertFileSrc(localPath);
    }
    return remoteUrl;
  }
</script>

{#if fontFaceCss}
  {@html `<style>${fontFaceCss}</style>`}
{/if}

<div
  class="slide-viewport"
  style="width: {scaledWidth}px; height: {scaledHeight}px;"
>
  <div
    class="slide-canvas"
    style="width: 1920px; height: 1080px; transform: scale({scale}); transform-origin: top left;"
  >
    {#each slide.elements as elem}
      {#if elem.type === "Text"}
        <div
          class="element text-element"
          style="
            left: {elem.x}px;
            top: {elem.y}px;
            width: {elem.width}px;
            height: {elem.height}px;
            {elem.font_family ? `font-family: "${elem.font_family}";` : ''}
            {elem.font_size ? `font-size: ${elem.font_size}px;` : ''}
            {elem.color ? `color: ${elem.color};` : ''}
            {elem.bold ? 'font-weight: bold;' : ''}
            {elem.italic ? 'font-style: italic;' : ''}
            {elem.rotation ? `transform: rotate(${elem.rotation}deg);` : ''}
          "
        >
          {elem.content}
        </div>
      {:else if elem.type === "Image"}
        <img
          class="element image-element"
          src={assetSrc(elem.local_path, elem.asset_url)}
          alt=""
          style="
            left: {elem.x}px;
            top: {elem.y}px;
            width: {elem.width}px;
            height: {elem.height}px;
            {elem.rotation ? `transform: rotate(${elem.rotation}deg);` : ''}
          "
        />
      {:else if elem.type === "Shape"}
        <div
          class="element shape-element"
          style="
            left: {elem.x}px;
            top: {elem.y}px;
            width: {elem.width}px;
            height: {elem.height}px;
            {elem.fill_color ? `background: ${elem.fill_color};` : ''}
            {elem.stroke_color ? `border: 2px solid ${elem.stroke_color};` : ''}
            {elem.rotation ? `transform: rotate(${elem.rotation}deg);` : ''}
          "
        >
          {#if elem.svg_path}
            <svg viewBox="0 0 {elem.width} {elem.height}" width="100%" height="100%">
              <path d={elem.svg_path} fill={elem.fill_color ?? 'none'} stroke={elem.stroke_color ?? 'none'} />
            </svg>
          {/if}
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .slide-viewport {
    overflow: hidden;
    background: #fff;
    border-radius: 4px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .slide-canvas {
    position: relative;
  }

  .element {
    position: absolute;
    overflow: hidden;
  }

  .text-element {
    color: #000;
    word-wrap: break-word;
    white-space: pre-wrap;
  }

  .image-element {
    object-fit: cover;
  }

  .shape-element {
    overflow: visible;
  }
</style>
