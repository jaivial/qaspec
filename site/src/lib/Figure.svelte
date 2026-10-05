<script>
  /** A product screenshot with a caption. Images keep a 1px outline for consistent depth. */
  let { src, alt, caption = null, width, height, eager = false } = $props();
  const base = import.meta.env.BASE_URL;
</script>

<figure>
  <picture>
    <source type="image/webp" srcset="{base}screenshots/{src}.webp" />
    <img src="{base}screenshots/{src}.png" {alt} {width} {height} loading={eager ? 'eager' : 'lazy'} decoding="async" />
  </picture>
  {#if caption}<figcaption>{@html caption}</figcaption>{/if}
</figure>

<style>
  figure { margin: 0; min-width: 0; }
  img { display: block; width: 100%; height: auto; border-radius: var(--r-md); outline: 1px solid rgb(255 255 255 / 0.1); outline-offset: -1px; background: var(--surface); }
  figcaption { color: var(--text-2); font-size: 14px; margin-top: 10px; }
  figcaption :global(code) { font-size: 12.5px; }
</style>
