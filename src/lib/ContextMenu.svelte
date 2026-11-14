<script lang="ts">
  import { closeMenu, contextMenu, type MenuItem } from "./menu.svelte";

  let box = $state<HTMLDivElement>();
  let left = $state(0);
  let top = $state(0);

  /** Simple 24x24 stroke icons, by key. */
  const ICONS: Record<string, string> = {
    open: "M5 12h14M13 6l6 6-6 6",
    pin: "M9 4h6l-1 6 3 3v2H7v-2l3-3-1-6zM12 15v5",
    unpin: "M9 4h6l-1 6 3 3v2H7v-2l3-3-1-6zM12 15v5M4 4l16 16",
    read: "M3 12.5l4 4 7-9M11 15l2 2 8-10",
    unread: "M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10z",
    archive: "M4 5h16v4H4zM5 9v10h14V9M10 13h4",
    unarchive: "M4 5h16v4H4zM5 9v10h14V9M12 17v-5M9.5 14.5 12 12l2.5 2.5",
    info: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11v6M12 7.5v.5",
    clear: "M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13M10 11v6M14 11v6",
    delete: "M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13",
    leave: "M10 4H5v16h5M14 8l4 4-4 4M18 12H9",
    reply: "M10 7 4 12l6 5v-3c5 0 8 1 10 5 0-6-3-10-10-10V7z",
    copy: "M8 8h11v12H8zM5 16V4h11",
    link: "M10 14a4 4 0 0 0 5.6 0l3-3a4 4 0 0 0-5.6-5.6l-1 1M14 10a4 4 0 0 0-5.6 0l-3 3a4 4 0 0 0 5.6 5.6l1-1",
    save: "M12 4v11m-5-5 5 5 5-5M5 20h14",
    folder: "M4 7a1 1 0 0 1 1-1h4l2 2h8a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1z",
    edit: "M4 20h4L19 9l-4-4L4 16v4zM13 7l4 4",
    forward: "M14 7l6 5-6 5v-3c-5 0-8 1-10 5 0-6 3-10 10-10V7z",
    mute: "M11 5 6 9H3v6h3l5 4V5zM16 9l5 6M21 9l-5 6",
    select: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM8 12l3 3 5-6",
    unmute: "M11 5 6 9H3v6h3l5 4V5zM15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13",
  };

  // Keep the menu inside the window when opened near an edge.
  $effect(() => {
    const { x, y, items, strip } = contextMenu;
    if (!box || (items.length === 0 && !strip)) return;
    left = Math.max(4, Math.min(x, window.innerWidth - box.offsetWidth - 4));
    top = Math.max(4, Math.min(y, window.innerHeight - box.offsetHeight - 4));
  });

  function run(item: MenuItem) {
    closeMenu();
    item.action();
  }

  function pick(emoji: string) {
    const strip = contextMenu.strip;
    closeMenu();
    strip?.pick(emoji);
  }
</script>

<svelte:window
  onkeydown={(event) => event.key === "Escape" && closeMenu()}
  onblur={closeMenu}
  onresize={closeMenu}
/>
<!-- `capture`: scrolling anywhere, in any pane, dismisses the menu. -->
<svelte:document onpointerdowncapture={(event) => !box?.contains(event.target as Node) && closeMenu()} onscrollcapture={closeMenu} />

{#if contextMenu.items.length > 0 || contextMenu.strip}
  <div class="menu" role="menu" bind:this={box} style:left="{left}px" style:top="{top}px">
    {#if contextMenu.strip}
      <div class="strip">
        {#each contextMenu.strip.emojis as emoji (emoji)}
          <button
            class="emoji"
            class:on={emoji === contextMenu.strip.active}
            onclick={() => pick(emoji)}>{emoji}</button
          >
        {/each}
      </div>
    {/if}
    {#each contextMenu.items as item}
      <button role="menuitem" class:danger={item.danger} onclick={() => run(item)}>
        <span class="ico" aria-hidden="true">
          {#if item.icon && ICONS[item.icon]}
            <svg viewBox="0 0 24 24" width="18" height="18">
              <path
                d={ICONS[item.icon]}
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          {/if}
        </span>
        {item.label}
      </button>
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 30;
    min-width: 190px;
    display: flex;
    flex-direction: column;
    padding: 5px;
    border-radius: 10px;
    background: #1f2c3a;
    box-shadow: 0 6px 24px rgb(0 0 0 / 45%);
  }

  button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px 7px 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }

  button:hover {
    background: rgb(255 255 255 / 8%);
  }

  .ico {
    flex: none;
    width: 18px;
    height: 18px;
    display: flex;
    color: var(--text-dim);
  }

  .danger,
  .danger .ico {
    color: var(--danger);
  }

  .strip {
    display: flex;
    gap: 2px;
    padding: 2px 2px 6px;
    margin-bottom: 4px;
    border-bottom: 1px solid rgb(255 255 255 / 8%);
  }

  .emoji {
    flex: 1;
    justify-content: center;
    padding: 4px 5px;
    border-radius: 8px;
    font-size: 20px;
    line-height: 1.2;
  }

  .emoji.on {
    background: rgb(255 255 255 / 14%);
  }
</style>
