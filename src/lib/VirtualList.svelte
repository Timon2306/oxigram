<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  // Fixed-height rows: only the rows in view (plus a small margin) exist in
  // the DOM, so the list costs the same with 50 items or 50 000.
  let {
    items,
    rowHeight,
    key,
    row,
    onend,
  }: {
    items: T[];
    rowHeight: number;
    key: (item: T) => string | number;
    row: Snippet<[T]>;
    /** Called when the user scrolls near the end of the list. */
    onend?: () => void;
  } = $props();

  const OVERSCAN = 6;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let height = $state(0);

  const first = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN));
  const last = $derived(
    Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + OVERSCAN),
  );
  const visible = $derived(items.slice(first, last));

  $effect(() => {
    if (!viewport) return;
    const observer = new ResizeObserver(() => (height = viewport?.clientHeight ?? 0));
    observer.observe(viewport);
    return () => observer.disconnect();
  });

  // Fires on scroll and also when a short list does not fill the viewport.
  $effect(() => {
    if (height > 0 && last >= items.length - OVERSCAN) onend?.();
  });
</script>

<div
  class="viewport"
  bind:this={viewport}
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
>
  <div class="spacer" style:height="{items.length * rowHeight}px">
    <div class="rows" style:transform="translateY({first * rowHeight}px)">
      {#each visible as item (key(item))}
        <div class="row" style:height="{rowHeight}px">{@render row(item)}</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .viewport {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .spacer {
    position: relative;
  }

  .rows {
    position: absolute;
    inset: 0 0 auto 0;
    will-change: transform;
  }

  .row {
    overflow: hidden;
  }
</style>
