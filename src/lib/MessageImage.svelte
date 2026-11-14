<script lang="ts">
  import { formatDuration, messageFileUrl, messageImageUrl, type Message } from "./ipc";
  import { settings } from "./settings.svelte";

  let {
    message,
    cover = false,
    onopen,
  }: {
    message: Message;
    /** Fill the parent cell (album grid) instead of keeping the aspect ratio. */
    cover?: boolean;
    onopen?: (src: string) => void;
  } = $props();

  let box = $state<HTMLElement>();
  let src = $state<string | null>(null);
  let failed = $state(false);

  /**
   * GIFs and video stickers loop silently in place, but only while on
   * screen: off-screen ones are paused so they cost no decoding.
   */
  const loops = $derived(
    settings.autoplayGifs &&
      message.playable &&
      (message.kind === "gif" || message.kind === "sticker"),
  );
  let loopSrc = $state<string | null>(null);
  let video = $state<HTMLVideoElement>();
  let visible = $state(false);

  $effect(() => {
    if (!box || !loops) return;
    const { peerId, id } = message;
    let current = true;
    const observer = new IntersectionObserver(
      (entries) => {
        visible = entries.some((entry) => entry.isIntersecting);
        if (visible && !loopSrc) {
          messageFileUrl(peerId, id).then(
            (url) => current && (loopSrc = url),
            () => {},
          );
        }
      },
      { rootMargin: "100px" },
    );
    observer.observe(box);
    return () => {
      current = false;
      observer.disconnect();
    };
  });

  $effect(() => {
    if (!video) return;
    if (visible) video.play().catch(() => {});
    else video.pause();
  });

  // Download only when the picture scrolls near the viewport.
  $effect(() => {
    if (!box) return;
    const { peerId, id } = message;
    let current = true;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        messageImageUrl(peerId, id).then(
          (url) => current && (src = url),
          () => current && (failed = true),
        );
      },
      { rootMargin: "400px" },
    );
    observer.observe(box);
    return () => {
      current = false;
      observer.disconnect();
    };
  });

  /** Tallest a single picture is drawn, in CSS pixels. */
  const MAX_HEIGHT = 420;
  /** Narrow pictures are not drawn thinner than this (unless smaller already). */
  const MIN_WIDTH = 160;

  /**
   * The drawn width, decided up front: limiting the height afterwards would
   * shrink the picture but not the bubble around it.
   */
  const width = $derived.by(() => {
    const w = message.imageW || 320;
    const h = message.imageH || 240;
    const fitted = Math.min(w, (MAX_HEIGHT * w) / h);
    return Math.round(Math.max(fitted, Math.min(MIN_WIDTH, w)));
  });

  const playable = $derived(
    message.kind === "video" || message.kind === "gif" || message.kind === "round",
  );
</script>

<button
  class="image"
  class:cover
  class:round={message.kind === "round"}
  bind:this={box}
  style:aspect-ratio={cover ? undefined : `${message.imageW} / ${message.imageH}`}
  style:width={cover ? undefined : `${width}px`}
  disabled={!onopen || (!src && !playable)}
  onclick={() => (src || playable) && onopen?.(src ?? "")}
>
  {#if loopSrc}
    <!-- svelte-ignore a11y_media_has_caption -->
    <video bind:this={video} src={loopSrc} muted loop playsinline preload="auto" poster={src ?? undefined}></video>
  {:else if src}
    <img {src} alt="" draggable="false" />
  {:else if failed}
    <span class="state">Не удалось загрузить</span>
  {/if}
  {#if playable && !loopSrc}
    <span class="play" aria-hidden="true">▶</span>
  {/if}
  {#if message.duration > 0 && (message.kind === "video" || message.kind === "round")}
    <span class="duration">{formatDuration(message.duration)}</span>
  {:else if message.kind === "gif"}
    <span class="duration">GIF</span>
  {/if}
</button>

<style>
  .image {
    position: relative;
    display: block;
    max-width: 100%;
    padding: 0;
    border: none;
    border-radius: 10px;
    overflow: hidden;
    /* Reserved before the file arrives, so the chat does not jump. */
    background: var(--bg-input);
    cursor: zoom-in;
  }

  .image:disabled {
    opacity: 1;
    cursor: default;
  }

  .image.cover {
    width: 100%;
    height: 100%;
    border-radius: 0;
  }

  .image.round {
    border-radius: 50%;
  }

  video {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .duration {
    position: absolute;
    left: 8px;
    top: 8px;
    padding: 2px 7px;
    border-radius: 9px;
    background: rgb(0 0 0 / 50%);
    color: #fff;
    font-size: 12px;
    font-weight: 500;
    pointer-events: none;
  }

  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .state {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6px;
    color: var(--text-dim);
    font-size: 12px;
    font-weight: 400;
    text-align: center;
  }

  .play {
    position: absolute;
    inset: 50% auto auto 50%;
    width: 44px;
    height: 44px;
    transform: translate(-50%, -50%);
    display: flex;
    align-items: center;
    justify-content: center;
    padding-left: 3px;
    border-radius: 50%;
    background: rgb(0 0 0 / 55%);
    color: #fff;
    font-size: 16px;
  }
</style>
