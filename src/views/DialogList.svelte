<script lang="ts">
  import { dialogTitle, formatListDate, isOnline, previewText, type Dialog, typingText, type TypingState, api, kindLabel, type FoundMessage, inFolder, type Folder } from "../lib/ipc";
  import { openMenu } from "../lib/menu.svelte";
  import VirtualList from "../lib/VirtualList.svelte";
  import Avatar from "../lib/Avatar.svelte";

  let {
    dialogs,
    activeId,
    now,
    loading,
    error,
    online,
    onopen,
    onend,
    onmenu,
    onmarkread,
    oninfo,
    onpin,
    onunread,
    onarchive,
    onmute,
    typing = {},
    drafts = {},
    folders = [],
    onopenmessage,
    onclear,
    ondelete,
    compact = false,
    showArchive = $bindable(false),
  }: {
    onpin: (dialog: Dialog) => void;
    onunread: (dialog: Dialog) => void;
    onarchive: (dialog: Dialog) => void;
    onmute: (dialog: Dialog) => void;
    /** Who is typing where, to show instead of the last message. */
    typing?: TypingState;
    /** Unsent texts by chat, shown as "Черновик: …". */
    drafts?: Record<number, string>;
    /** The user's chat folders, shown as tabs above the list. */
    folders?: Folder[];
    /** A message from the global search was clicked. */
    onopenmessage: (found: FoundMessage) => void;
    onclear: (dialog: Dialog) => void;
    ondelete: (dialog: Dialog) => void;
    /** Avatar-only rows, for the narrowest sidebar. */
    compact?: boolean;
    /** The Archive folder is shown instead of the main list. */
    showArchive?: boolean;
    dialogs: Dialog[];
    activeId: number | null;
    /** Unix seconds, for expiring online dots. */
    now: number;
    loading: boolean;
    error: string;
    online: boolean;
    onopen: (dialog: Dialog) => void;
    onend: () => void;
    /** The hamburger button was pressed. */
    onmenu: () => void;
    onmarkread: (dialog: Dialog) => void;
    oninfo: (dialog: Dialog) => void;
  } = $props();

  const ROW_HEIGHT = 66;

  /** The Archive folder is a separate list, as in official clients. */
  let query = $state("");

  const archivedCount = $derived(dialogs.filter((dialog) => dialog.archived).length);
  // Searching also looks through messages in all chats, after a short pause.
  let found = $state.raw<FoundMessage[]>([]);
  let searching = $state(false);
  $effect(() => {
    const needle = query.trim();
    found = [];
    if (needle.length < 2) return;
    searching = true;
    let live = true;
    const timer = setTimeout(() => {
      api.searchGlobal(needle).then(
        (list) => live && (found = list),
        () => {},
      ).finally(() => live && (searching = false));
    }, 450);
    return () => {
      live = false;
      clearTimeout(timer);
      searching = false;
    };
  });

  // Chat folders as tabs; the choice is remembered.
  const FOLDER_KEY = "oxigram.folder";
  let folderId = $state<number>(loadFolder());
  function loadFolder(): number {
    try {
      return Number(localStorage.getItem(FOLDER_KEY)) || 0;
    } catch {
      return 0;
    }
  }
  function chooseFolder(id: number) {
    folderId = id;
    showArchive = false;
    try {
      localStorage.setItem(FOLDER_KEY, String(id));
    } catch {
      // Not remembered; fine.
    }
  }
  const folder = $derived(folders.find((f) => f.id === folderId) ?? null);
  /** Unread chats per folder, for the tab badges. */
  const folderUnread = $derived(
    Object.fromEntries(
      folders.map((f) => [
        f.id,
        dialogs.filter((d) => d.unread > 0 && !d.muted && inFolder(d, f)).length,
      ]),
    ),
  );

  /** A vertical mouse wheel scrolls the tab strip sideways. */
  function sideways(strip: HTMLElement) {
    const onwheel = (event: WheelEvent) => {
      if (strip.scrollWidth <= strip.clientWidth || event.deltaX !== 0) return;
      event.preventDefault();
      strip.scrollLeft += event.deltaY;
    };
    strip.addEventListener("wheel", onwheel, { passive: false });
    return () => strip.removeEventListener("wheel", onwheel);
  }

  const shown = $derived.by(() => {
    const needle = query.trim().toLocaleLowerCase();
    // Searching looks through every chat, archived or not.
    if (needle) {
      return dialogs.filter((dialog) => dialogTitle(dialog).toLocaleLowerCase().includes(needle));
    }
    if (folder && !showArchive) return dialogs.filter((dialog) => inFolder(dialog, folder));
    return dialogs.filter((dialog) => dialog.archived === showArchive);
  });

  function chatsWord(count: number): string {
    const tens = count % 100;
    const ones = count % 10;
    if (tens >= 11 && tens <= 14) return "чатов";
    if (ones === 1) return "чат";
    if (ones >= 2 && ones <= 4) return "чата";
    return "чатов";
  }

  function menuFor(event: MouseEvent, dialog: Dialog) {
    openMenu(event, [
      { label: "Открыть", icon: "open", action: () => onopen(dialog) },
      {
        label: dialog.pinned ? "Открепить" : "Закрепить",
        icon: dialog.pinned ? "unpin" : "pin",
        action: () => onpin(dialog),
      },
      dialog.unread > 0
        ? { label: "Отметить как прочитанное", icon: "read", action: () => onmarkread(dialog) }
        : { label: "Отметить как непрочитанное", icon: "unread", action: () => onunread(dialog) },
      {
        label: dialog.archived ? "Из архива" : "В архив",
        icon: dialog.archived ? "unarchive" : "archive",
        action: () => onarchive(dialog),
      },
      ...(dialog.kind === "saved"
        ? []
        : [
            {
              label: dialog.muted ? "Включить уведомления" : "Выключить уведомления",
              icon: dialog.muted ? "unmute" : "mute",
              action: () => onmute(dialog),
            },
          ]),
      { label: "Информация", icon: "info", action: () => oninfo(dialog) },
      ...(dialog.kind === "channel"
        ? []
        : [
            {
              label: "Очистить историю",
              icon: "clear",
              danger: true,
              action: () => onclear(dialog),
            },
          ]),
      {
        label:
          dialog.kind === "group"
            ? "Покинуть группу"
            : dialog.kind === "channel"
              ? "Покинуть канал"
              : "Удалить чат",
        icon: dialog.kind === "user" || dialog.kind === "saved" ? "delete" : "leave",
        danger: true,
        action: () => ondelete(dialog),
      },
    ]);
  }
</script>

{#snippet dialogRow(dialog: Dialog)}
  <button
    class="dialog"
    class:active={dialog.id === activeId}
    title={compact ? dialogTitle(dialog) : undefined}
    onclick={() => onopen(dialog)}
    oncontextmenu={(event) => menuFor(event, dialog)}
  >
    <span class="ava">
      <Avatar
        peerId={dialog.id}
        photoId={dialog.photoId}
        title={dialogTitle(dialog)}
        saved={dialog.kind === "saved"}
        size={48}
      />
      {#if isOnline(dialog.presence, now)}<span class="online"></span>{/if}
      {#if compact && dialog.unread > 0}
        <span class="unread badge">{dialog.unread > 99 ? "99+" : dialog.unread}</span>
      {/if}
    </span>
    {#if !compact}
    <span class="body">
      <span class="top">
        <span class="title">{dialogTitle(dialog)}</span>
        {#if dialog.muted}
          <svg class="muted-icon" viewBox="0 0 24 24" width="14" height="14" aria-label="Без звука">
            <path
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M11 5 6 9H3v6h3l5 4V5zM16 9l5 6M21 9l-5 6"
            />
          </svg>
        {/if}
        {#if dialog.lastOutgoing && (dialog.kind === "user" || dialog.kind === "group")}
          {@const read = dialog.lastId > 0 && dialog.lastId <= dialog.readOutbox}
          <svg
            class="ticks"
            class:read
            viewBox="0 0 18 12"
            width="16"
            height="11"
            aria-label={read ? "Прочитано" : "Отправлено"}
          >
            <path d="M1.5 6.5 5 10l6.5-8" />
            {#if read}<path d="M8 10l6.5-8" />{/if}
          </svg>
        {/if}
        <span class="date">{formatListDate(dialog.lastDate)}</span>
      </span>
      <span class="bottom">
        <span class="preview">
          {#if typing[dialog.id]}<span class="typing">{typingText(typing[dialog.id].action)}</span
            >{:else if drafts[dialog.id] && dialog.id !== activeId}<span class="draft">Черновик: </span
            >{drafts[dialog.id].replace(/\s+/g, " ")}{:else}
          {#if dialog.lastOutgoing && dialog.kind !== "saved" && dialog.kind !== "user"}<span class="you"
              >{"Вы: "}</span
            >{/if}{previewText(dialog.lastText)}
          {/if}
        </span>
        {#if dialog.unreadReactions > 0}
          <span class="reaction-badge" aria-label="Новые реакции">
            <svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"
              ><path
                fill="currentColor"
                d="M12 21s-7.5-4.6-9.6-9.2C.9 8.4 3 4.5 6.7 4.5c2.1 0 3.6 1.2 5.3 3 1.7-1.8 3.2-3 5.3-3 3.7 0 5.8 3.9 4.3 7.3C19.5 16.4 12 21 12 21z"
              /></svg
            >
          </span>
        {/if}
        {#if dialog.unread > 0}
          <span class="unread" class:quiet={dialog.muted}>{dialog.unread > 999 ? "999+" : dialog.unread}</span>
        {:else if dialog.pinned}
          <svg class="pin" viewBox="0 0 24 24" width="14" height="14" aria-label="Закреплён">
            <path
              fill="currentColor"
              d="M14 3l7 7-3 1-3 3 .5 4.5L14 20l-4-4-5 5v-1.5L9.5 15l-4-4 1.5-1.5L11.5 10l3-3 .5-3z"
            />
          </svg>
        {/if}
      </span>
    </span>
    {/if}
  </button>
{/snippet}

<div class="pane" class:compact>
  <header>
    {#if showArchive && !query}
      <button class="icon" onclick={() => (showArchive = false)} aria-label="Назад к чатам">
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <path
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M15 6l-6 6 6 6"
          />
        </svg>
      </button>
    {:else}
      <button class="icon" onclick={onmenu} aria-label="Меню">
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <path
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            d="M4 7h16M4 12h16M4 17h16"
          />
        </svg>
      </button>
    {/if}
    {#if !compact}
      <input
        class="search"
        type="search"
        bind:value={query}
        placeholder={showArchive ? "Поиск в архиве" : "Поиск"}
        spellcheck="false"
      />
    {/if}
  </header>

  {#if folders.length > 0 && !compact && !query.trim()}
    <div class="folders" {@attach sideways}>
      <button class="tab" class:on={folderId === 0} onclick={() => chooseFolder(0)}>Все чаты</button>
      {#each folders as f (f.id)}
        <button class="tab" class:on={folderId === f.id} onclick={() => chooseFolder(f.id)}>
          {f.title}{#if folderUnread[f.id] > 0}<span class="tab-badge">{folderUnread[f.id]}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}

  {#if !online}
    <p class="offline">{compact ? "Нет сети" : "Нет соединения, переподключение…"}</p>
  {/if}
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if archivedCount > 0 && !showArchive && !query}
    <button class="archive" title={compact ? "Архив" : undefined} onclick={() => (showArchive = true)}>
      <span class="archive-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="22" height="22">
          <path
            fill="currentColor"
            d="M4 4h16a1 1 0 0 1 1 1v3H3V5a1 1 0 0 1 1-1zm0 6h16v9a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9zm5 3v2h6v-2H9z"
          />
        </svg>
      </span>
      {#if !compact}
        <span class="body">
          <span class="title">Архив</span>
          <span class="preview">{archivedCount} {chatsWord(archivedCount)}</span>
        </span>
      {/if}
    </button>
  {/if}

  <div class="list">
    {#if query.trim()}
      <div class="results">
        {#each shown.slice(0, 30) as dialog (dialog.id)}
          <div class="result-row">{@render dialogRow(dialog)}</div>
        {/each}
        {#if found.length > 0 || searching}
          <p class="section">Сообщения</p>
        {/if}
        {#each found as hit (hit.message.peerId + ":" + hit.message.id)}
          <button class="dialog hit" onclick={() => onopenmessage(hit)}>
            <Avatar
              peerId={hit.message.peerId}
              photoId={hit.peer.photoId}
              title={hit.peer.title}
              saved={hit.peer.kind === "saved"}
              size={48}
            />
            <span class="body">
              <span class="top">
                <span class="title">{hit.peer.kind === "saved" ? "Избранное" : hit.peer.title || "Без названия"}</span>
                <span class="date">{formatListDate(hit.message.date)}</span>
              </span>
              <span class="bottom">
                <span class="preview">{previewText(hit.message.text || kindLabel(hit.message.kind) || "")}</span>
              </span>
            </span>
          </button>
        {/each}
        {#if searching && found.length === 0}
          <p class="dim placeholder">Поиск сообщений…</p>
        {:else if !searching && shown.length === 0 && found.length === 0}
          <p class="dim placeholder">Ничего не найдено</p>
        {/if}
      </div>
    {:else if shown.length === 0}
      <p class="dim placeholder">
        {query ? "Ничего не найдено" : loading ? "Загрузка чатов…" : "Чатов нет"}
      </p>
    {:else}
      <VirtualList items={shown} rowHeight={ROW_HEIGHT} key={(dialog) => dialog.id} {onend}>
        {#snippet row(dialog)}{@render dialogRow(dialog)}{/snippet}
      </VirtualList>
    {/if}
  </div>
</div>

<style>
  .pane {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 52px;
    padding: 0 10px;
  }

  .icon {
    flex: none;
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
  }

  .icon:hover {
    background: var(--bg-hover);
  }

  .search {
    flex: 1;
    min-width: 0;
    height: 34px;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: 17px;
    background: var(--bg-input);
  }

  .offline {
    flex: none;
    margin: 0;
    padding: 5px 10px;
    background: #4a3a17;
    color: #f1d38a;
    font-size: 12px;
    text-align: center;
  }

  .error {
    flex: none;
    margin: 0;
    padding: 6px 12px;
  }

  .list {
    flex: 1;
    min-height: 0;
  }

  .placeholder {
    margin: 0;
    padding: 20px 12px;
    text-align: center;
  }

  .dialog,
  .archive {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 10px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }

  .dialog {
    height: 100%;
  }

  .archive {
    flex: none;
    height: 66px;
  }

  .archive-icon {
    flex: none;
    width: 48px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: var(--bg-input);
    color: var(--text-dim);
  }

  .dialog:hover,
  .archive:hover {
    background: var(--bg-hover);
  }

  .dialog.active {
    background: var(--bg-active);
  }

  .dialog:focus-visible,
  .archive:focus-visible {
    outline-offset: -2px;
  }

  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .top,
  .bottom {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .title,
  .preview {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    font-weight: 600;
  }

  .preview,
  .date {
    color: var(--text-dim);
  }

  .ticks {
    flex: none;
    margin-right: -4px;
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .dialog.active .ticks {
    stroke: #fff;
  }

  .date {
    flex: none;
    font-size: 12px;
  }

  .you {
    color: var(--accent);
  }

  .dialog.active .preview,
  .dialog.active .date,
  .dialog.active .you,
  .dialog.active .pin {
    color: rgb(255 255 255 / 75%);
  }

  .folders {
    flex: none;
    display: flex;
    gap: 2px;
    padding: 0 6px;
    overflow-x: auto;
    border-bottom: 1px solid var(--border);
    scrollbar-width: none;
  }

  .tab {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 9px 10px 8px;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    color: var(--text-dim);
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
  }

  .tab.on {
    border-bottom-color: var(--accent);
    color: var(--accent);
  }

  .tab-badge {
    min-width: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--accent);
    color: #fff;
    font-size: 11px;
    line-height: 18px;
    text-align: center;
  }

  .tab:not(.on) .tab-badge {
    background: var(--text-dim);
  }

  .results {
    height: 100%;
    overflow-y: auto;
  }

  .result-row {
    height: 66px;
  }

  .hit {
    height: 66px;
  }

  .section {
    margin: 0;
    padding: 8px 12px 6px;
    background: var(--bg);
    color: var(--text-dim);
    font-size: 13px;
    font-weight: 600;
  }

  .typing {
    color: var(--accent);
  }

  .draft {
    color: var(--danger);
  }

  .dialog.active .draft {
    color: #fff;
  }

  .dialog.active .typing {
    color: #fff;
  }

  .unread.quiet {
    background: var(--text-dim);
  }

  .muted-icon {
    flex: none;
    margin-left: -4px;
    color: var(--text-dim);
  }

  .unread {
    flex: none;
    min-width: 22px;
    padding: 2px 7px;
    border-radius: 11px;
    background: var(--accent);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    text-align: center;
  }

  .dialog.active .unread {
    background: #fff;
    color: var(--bg-active);
  }

  .ava {
    position: relative;
    flex: none;
    display: flex;
  }

  .online {
    position: absolute;
    right: 1px;
    bottom: 1px;
    width: 12px;
    height: 12px;
    border: 2px solid var(--bg-raised);
    border-radius: 50%;
    background: #4fce5d;
  }

  .dialog:hover .online {
    border-color: var(--bg-hover);
  }

  .dialog.active .online {
    border-color: var(--bg-active);
  }

  .reaction-badge {
    flex: none;
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: #e5484d;
    color: #fff;
  }

  .pin {
    flex: none;
    color: var(--text-dim);
  }

  .compact header {
    justify-content: center;
    padding: 0;
  }

  .compact .dialog,
  .compact .archive {
    justify-content: center;
    padding: 0;
  }

  .compact .offline {
    padding: 4px 2px;
    font-size: 10px;
  }

  .compact .error {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 6px 4px;
    font-size: 11px;
  }

  .compact .placeholder {
    padding: 20px 4px;
    font-size: 11px;
  }

  .badge {
    position: absolute;
    top: -3px;
    right: -6px;
    min-width: 18px;
    padding: 1px 5px;
    border-radius: 9px;
    font-size: 11px;
    line-height: 16px;
    box-shadow: 0 0 0 2px var(--bg-raised);
  }

  .dialog:hover .badge {
    box-shadow: 0 0 0 2px var(--bg-hover);
  }

  .dialog.active .badge {
    box-shadow: 0 0 0 2px var(--bg-active);
  }
</style>
