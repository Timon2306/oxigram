<script lang="ts">
  import { customEmojiUrl } from "./ipc";

  /** A custom (premium) emoji drawn as a still picture; a sparkle until it loads. */
  let { id, size = 18 }: { id: string; size?: number } = $props();

  let src = $state<string | null>(null);

  $effect(() => {
    src = null;
    let current = true;
    customEmojiUrl(id).then(
      (url) => current && (src = url),
      () => {},
    );
    return () => (current = false);
  });
</script>

{#if src}
  <img {src} alt="" width={size} height={size} draggable="false" />
{:else}
  ✨
{/if}

<style>
  img {
    display: block;
    object-fit: contain;
  }
</style>
