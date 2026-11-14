<script lang="ts">
  import { avatarUrl } from "./ipc";
  import { settings } from "./settings.svelte";

  let {
    peerId,
    photoId,
    title,
    saved = false,
    size = 42,
  }: {
    peerId: number;
    photoId: string | null;
    title: string;
    /** The chat with oneself gets a bookmark instead of a photo. */
    saved?: boolean;
    size?: number;
  } = $props();

  const COLORS = ["#e17076", "#7bc862", "#e5ca77", "#65aadd", "#a695e7", "#ee7aae", "#6ec9cb"];

  let src = $state<string | null>(null);

  // Rows are virtualized, so being mounted means being on screen: the photo
  // is requested only for chats the user actually sees.
  $effect(() => {
    src = null;
    if (saved || !photoId || !settings.avatars) return;
    let current = true;
    avatarUrl(peerId, photoId).then(
      (url) => current && (src = url),
      () => {},
    );
    return () => (current = false);
  });

  const initial = $derived((Array.from(title.trim())[0] ?? "?").toUpperCase());
  const color = $derived(saved ? "#4c9ffe" : COLORS[Math.abs(peerId) % COLORS.length]);
</script>

<span class="avatar" style:width="{size}px" style:height="{size}px" style:background={color}>
  {#if src}
    <img {src} alt="" draggable="false" />
  {:else if saved}
    <svg viewBox="0 0 24 24" width={size * 0.5} height={size * 0.5} aria-hidden="true">
      <path fill="#fff" d="M6 3h12a1 1 0 0 1 1 1v17l-7-4-7 4V4a1 1 0 0 1 1-1z" />
    </svg>
  {:else}
    <span style:font-size="{size * 0.4}px">{initial}</span>
  {/if}
</span>

<style>
  .avatar {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    overflow: hidden;
    color: #fff;
    font-weight: 600;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
