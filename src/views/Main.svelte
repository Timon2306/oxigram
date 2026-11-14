<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import {
    api,
    describeError,
    dialogTitle,
    type Dialog,
    type Me,
    type Message,
    type UiEvent,
    type TypingAction,
    type TypingState,
    type UiState,
  } from "../lib/ipc";
  import Avatar from "../lib/Avatar.svelte";
  import DialogList from "./DialogList.svelte";
  import Chat from "./Chat.svelte";
  import InfoPanel from "./InfoPanel.svelte";
  import Contacts from "./Contacts.svelte";
  import { saveSettings, settings as prefs } from "../lib/settings.svelte";
  import { forgetMediaUrls, formatFileSize } from "../lib/ipc";
  import { applyTheme, savedTheme, type Theme } from "../lib/theme";
  import type { Contact, Folder, FoundChat, FoundMessage } from "../lib/ipc";
  import Confirm from "../lib/Confirm.svelte";
  import ForwardPicker from "../lib/ForwardPicker.svelte";
  import Toast from "../lib/Toast.svelte";
  import { showToast } from "../lib/notify.svelte";

  let { me, onstate }: { me: Me; onstate: (next: UiState) => void } = $props();

  // `$state.raw`: these arrays are replaced wholesale, so Svelte does not
  // need to wrap every dialog in a deep reactive proxy.
  let dialogs = $state.raw<Dialog[]>([]);
  let done = $state(false);
  let loading = $state(false);
  let error = $state("");
  let activeId = $state<number | null>(null);
  let online = $state(true);
  let chat = $state<ReturnType<typeof Chat>>();

  /** Unsent texts by chat, kept for this computer only. */
  /** The "Экономия памяти" window. */
  let saver = $state(false);
  const SAVER = [
    {
      key: "avatars",
      title: "Фото на аватарках",
      note: "Без них — цветной кружок с буквой.",
    },
    {
      key: "autoloadImages",
      title: "Загружать картинки в чатах сами",
      note: "Без этого картинка и превью видео грузятся по клику.",
    },
    {
      key: "autoplayGifs",
      title: "Проигрывать GIF и видеостикеры",
      note: "Без этого они стоят на месте, пока не откроешь.",
    },
    {
      key: "customEmoji",
      title: "Кастомные эмодзи в реакциях",
      note: "Картинки таких реакций, по клику можно поставить такую же. Без этого — одна ✨ с общим числом.",
    },
    {
      key: "prefetchChats",
      title: "Подгружать верхние чаты заранее",
      note: "Десять верхних чатов открываются сразу, без ожидания сети.",
    },
  ] as const;

  const DRAFTS_KEY = "oxigram.drafts";
  /** How many chats from the top get their history fetched ahead of time. */
  const PREFETCH_CHATS = 10;
  /** Matches the first page a chat loads when opened. */
  const PREFETCH_PAGE = 50;
  let destroyed = false;
  $effect(() => () => (destroyed = true));
  function loadDrafts(): Record<number, string> {
    try {
      return JSON.parse(localStorage.getItem(DRAFTS_KEY) ?? "{}");
    } catch {
      return {};
    }
  }
  let drafts = $state.raw<Record<number, string>>(loadDrafts());

  function saveDraft(peerId: number, text: string) {
    const trimmed = text.trim() ? text : "";
    if ((drafts[peerId] ?? "") === trimmed) return;
    const { [peerId]: _, ...rest } = drafts;
    drafts = trimmed ? { ...rest, [peerId]: trimmed } : rest;
    try {
      localStorage.setItem(DRAFTS_KEY, JSON.stringify(drafts));
    } catch {
      // Not remembered after a restart; still kept for this session.
    }
  }

  /** Who is typing where; each entry clears itself after a few seconds. */
  let typing = $state.raw<TypingState>({});
  const typingTimers = new Map<number, ReturnType<typeof setTimeout>>();

  function ontyping(peerId: number, userId: number, action: TypingAction) {
    clearTimeout(typingTimers.get(peerId));
    if (action === "cancel") {
      const { [peerId]: _, ...rest } = typing;
      typing = rest;
      return;
    }
    typing = { ...typing, [peerId]: { userId, action } };
    // Telegram repeats the status every few seconds while it lasts.
    typingTimers.set(
      peerId,
      setTimeout(() => {
        const { [peerId]: _, ...rest } = typing;
        typing = rest;
      }, 6000),
    );
  }

  let drawer = $state(false);
  let showArchive = $state(false);
  let contactsOpen = $state(false);
  let theme = $state<Theme>(savedTheme());

  function toggleTheme() {
    theme = theme === "dark" ? "light" : "dark";
    applyTheme(theme);
  }

  /** Opens the private chat with a contact, even one with no messages yet. */
  function openContact(contact: Contact) {
    contactsOpen = false;
    showArchive = false;
    const known = dialogs.find((dialog) => dialog.id === contact.id);
    if (known) {
      open(known);
      return;
    }
    const stub: Dialog = {
      id: contact.id,
      kind: "user",
      title: contact.title,
      photoId: contact.photoId,
      lastText: "",
      lastDate: 0,
      lastOutgoing: false,
      lastId: 0,
      unread: 0,
      pinned: false,
      archived: false,
      readOutbox: 0,
      unreadReactions: 0,
      muted: false,
      contact: false,
      bot: false,
      presence: contact.presence,
    };
    dialogs = [stub, ...dialogs];
    open(stub);
  }
  /** A message to scroll to once its chat opens (from the global search). */
  let jumpTo = $state<number | null>(null);

  function openFound(hit: FoundMessage) {
    const known = dialogs.find((dialog) => dialog.id === hit.message.peerId);
    const target: Dialog = known ?? {
      id: hit.message.peerId,
      kind: hit.peer.kind,
      title: hit.peer.title,
      photoId: hit.peer.photoId,
      lastText: "",
      lastDate: 0,
      lastOutgoing: false,
      lastId: 0,
      unread: 0,
      pinned: false,
      archived: false,
      readOutbox: 0,
      unreadReactions: 0,
      muted: false,
      contact: false,
      bot: false,
      presence: null,
    };
    if (!known) dialogs = [target, ...dialogs];
    jumpTo = hit.message.id;
    open(target);
  }

  /** Opens a chat from a mention or a t.me link, adding it to the list if needed. */
  async function openChat(target: { username?: string; id?: number }) {
    try {
      let found: FoundChat | null = null;
      if (target.username) found = await api.resolveUsername(target.username);
      const id = found?.id ?? target.id;
      if (id === undefined) return;
      const known = dialogs.find((dialog) => dialog.id === id);
      if (known) {
        open(known);
        return;
      }
      const stub: Dialog = {
        id,
        kind: found?.kind ?? "user",
        title: found?.title ?? "",
        photoId: found?.photoId ?? null,
        lastText: "",
        lastDate: 0,
        lastOutgoing: false,
        lastId: 0,
        unread: 0,
        pinned: false,
        archived: false,
        readOutbox: 0,
        unreadReactions: 0,
        muted: false,
        contact: false,
        bot: false,
        presence: null,
      };
      dialogs = [stub, ...dialogs];
      open(stub);
    } catch (err) {
      showToast(describeError(err));
    }
  }

  let settings = $state(false);
  let cacheBytes = $state<number | null>(null);
  let autostart = $state(false);

  // Figures for the settings window, fetched each time it opens.
  $effect(() => {
    if (!settings) return;
    api.cacheSize().then((bytes) => (cacheBytes = bytes), () => (cacheBytes = null));
    api.autostartEnabled().then((on) => (autostart = on), () => {});
  });

  async function clearMedia() {
    try {
      cacheBytes = await api.clearCache();
      forgetMediaUrls();
      showToast("Кэш очищен");
    } catch (err) {
      showToast(describeError(err));
    }
  }

  async function toggleAutostart() {
    try {
      await api.setAutostart(!autostart);
      autostart = !autostart;
    } catch (err) {
      showToast(describeError(err));
    }
  }

  /** Asked once before signing out, since it cannot be undone. */
  let confirmSignOut = $state(false);
  let infoId = $state<number | null>(null);
  /** The panel shows the own profile, not the "Saved Messages" chat. */
  let infoProfile = $state(false);
  let version = $state("");

  /** Unix seconds, ticking so "в сети" expires and "N минут назад" moves on. */
  let now = $state(Math.floor(Date.now() / 1000));
  $effect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") now = Math.floor(Date.now() / 1000);
    }, 30_000);
    return () => clearInterval(timer);
  });

  const active = $derived(dialogs.find((dialog) => dialog.id === activeId) ?? null);

  // Resizable chat list. `want` is what the user asked for; the shown width
  // is derived from it so a narrow window never loses the preference.
  const SIDEBAR_KEY = "oxigram.sidebarWidth";
  const COMPACT_W = 76;
  const MIN_W = 260;
  const MAX_W = 460;
  const DEFAULT_W = 320;
  const SNAP_W = 200;
  const MIN_CHAT_W = 360;
  const KEY_STEP = 16;

  function loadWidth(): number {
    try {
      const value = Number(localStorage.getItem(SIDEBAR_KEY));
      if (Number.isFinite(value) && value >= COMPACT_W && value <= MAX_W) return value;
    } catch {
      // Storage may be unavailable; the default is fine.
    }
    return DEFAULT_W;
  }

  let want = $state(loadWidth());
  let winW = $state(window.innerWidth);
  let resizing = $state(false);

  /** Maps a requested width to a legal one: compact below the snap point. */
  function fit(raw: number, room: number): number {
    if (raw < SNAP_W) return COMPACT_W;
    const cap = Math.min(MAX_W, room - MIN_CHAT_W);
    const lo = Math.min(MIN_W, cap);
    return Math.max(COMPACT_W, Math.min(cap, Math.max(lo, raw)));
  }

  // Wide windows keep the chat's profile as a third column, as Telegram
  // Desktop does; closing it there is remembered.
  const DOCK_KEY = "oxigram.infoDocked";
  const DOCK_MIN_WINDOW = 1200;
  const DOCK_W = 340;

  function loadDock(): boolean {
    try {
      return localStorage.getItem(DOCK_KEY) !== "0";
    } catch {
      return true;
    }
  }

  let dockWanted = $state(loadDock());
  const wide = $derived(winW >= DOCK_MIN_WINDOW);
  /** What the docked column shows, or null when there is none. */
  const dockId = $derived(
    !wide
      ? null
      : (infoId ??
          (dockWanted && active && active.kind !== "saved" ? active.id : null)),
  );

  function setDock(on: boolean) {
    dockWanted = on;
    try {
      localStorage.setItem(DOCK_KEY, on ? "1" : "0");
    } catch {
      // Not persisted; ignore.
    }
  }

  /** The chat header was clicked: toggle the column, or open the overlay. */
  function toggleInfo() {
    if (!active) return;
    infoProfile = false;
    if (!wide) {
      infoId = active.id;
    } else if (infoId !== null) {
      infoId = null;
      setDock(true);
    } else {
      setDock(!(dockWanted && active.kind !== "saved"));
      if (active.kind === "saved") infoId = active.id;
    }
  }

  function closeDock() {
    if (infoId !== null) infoId = null;
    else setDock(false);
  }

  const sidebarW = $derived(fit(want, winW));
  const compact = $derived(sidebarW <= COMPACT_W);

  function saveWidth() {
    try {
      localStorage.setItem(SIDEBAR_KEY, String(want));
    } catch {
      // Not persisted; ignore.
    }
  }

  function dragStart(event: PointerEvent) {
    if (event.button !== 0) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    resizing = true;
  }

  function dragMove(event: PointerEvent) {
    if (resizing) want = event.clientX;
  }

  function dragEnd() {
    if (!resizing) return;
    resizing = false;
    want = sidebarW;
    saveWidth();
  }

  function resetWidth() {
    want = DEFAULT_W;
    saveWidth();
  }

  function handleKey(event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const delta = event.key === "ArrowRight" ? KEY_STEP : -KEY_STEP;
    if (compact) {
      if (delta > 0) want = MIN_W;
    } else if (sidebarW + delta < MIN_W) {
      want = COMPACT_W;
    } else {
      want = sidebarW + delta;
    }
    saveWidth();
  }

  async function loadPage(reset: boolean) {
    if (loading || (done && !reset)) return;
    loading = true;
    error = "";
    try {
      const page = await api.loadDialogs(reset);
      const known = new Set(reset ? [] : dialogs.map((dialog) => dialog.id));
      const fresh = page.dialogs.filter((dialog) => !known.has(dialog.id));
      dialogs = reset ? fresh : [...dialogs, ...fresh];
      done = page.done;
    } catch (err) {
      error = describeError(err);
    } finally {
      loading = false;
    }
  }

  let folders = $state.raw<Folder[]>([]);

  async function start() {
    getVersion().then(
      (value) => (version = value),
      () => {},
    );
    try {
      dialogs = await api.cachedDialogs();
    } catch {
      // The cache is only a head start; the network load below reports errors.
    }
    await loadPage(true);
    api.folders(me.id).then(
      (list) => (folders = list),
      () => {},
    );
    try {
      await api.subscribe(onevents);
    } catch (err) {
      error = describeError(err);
    }
    prefetch();
  }

  /**
   * Warms the message cache of the chats at the top of the list, one at a
   * time in the background, so opening them shows messages at once instead
   * of waiting for the network. Nothing is marked read.
   */
  async function prefetch() {
    if (!prefs.prefetchChats) return;
    const top = dialogs.filter((dialog) => !dialog.archived).slice(0, PREFETCH_CHATS);
    for (const dialog of top) {
      if (destroyed) return;
      try {
        await api.history(dialog.id, null, PREFETCH_PAGE);
      } catch {
        return; // Offline or rate-limited: opening a chat loads it anyway.
      }
    }
  }

  function open(dialog: Dialog) {
    activeId = dialog.id;
    infoId = null;
    infoProfile = false;
    // The list's status may be minutes old; ask for the current one.
    if (dialog.kind === "user") {
      api.peerPresence(dialog.id).then(
        (presence) => presence && patch(dialog.id, { presence }),
        () => {},
      );
    }
  }

  function patch(id: number, change: Partial<Dialog>) {
    dialogs = dialogs.map((dialog) => (dialog.id === id ? { ...dialog, ...change } : dialog));
  }

  function onread(id: number) {
    patch(id, { unread: 0 });
  }

  /** Bumped to rebuild the open chat (e.g. after its history was cleared). */
  let chatEpoch = $state(0);
  let pendingDelete = $state<{ kind: "clear" | "delete"; dialog: Dialog } | null>(null);
  let forwarding = $state<{ from: number; ids: number[] } | null>(null);

  async function togglePin(dialog: Dialog) {
    try {
      await api.pinDialog(dialog.id, !dialog.pinned);
      const updated = { ...dialog, pinned: !dialog.pinned };
      if (updated.pinned) dialogs = [updated, ...dialogs.filter((d) => d.id !== dialog.id)];
      else lift(updated);
    } catch (err) {
      showToast(describeError(err));
    }
  }

  async function markUnread(dialog: Dialog) {
    try {
      await api.markUnread(dialog.id, true);
      patch(dialog.id, { unread: Math.max(1, dialog.unread) });
    } catch (err) {
      showToast(describeError(err));
    }
  }

  async function toggleMute(dialog: Dialog) {
    try {
      await api.setMuted(dialog.id, !dialog.muted);
      patch(dialog.id, { muted: !dialog.muted });
    } catch (err) {
      showToast(describeError(err));
    }
  }

  // The backend rings for new messages; it needs to know which chat is open
  // and how many unread messages wait (title bar and tray tooltip).
  $effect(() => {
    api.setActiveChat(activeId).catch(() => {});
  });
  const unreadTotal = $derived(
    dialogs.reduce((sum, dialog) => (dialog.muted || dialog.archived ? sum : sum + dialog.unread), 0),
  );
  $effect(() => {
    api.setUnread(unreadTotal).catch(() => {});
  });

  async function toggleArchive(dialog: Dialog) {
    try {
      await api.archiveDialog(dialog.id, !dialog.archived);
      patch(dialog.id, { archived: !dialog.archived });
    } catch (err) {
      showToast(describeError(err));
    }
  }

  async function confirmPending() {
    const current = pendingDelete;
    pendingDelete = null;
    if (!current) return;
    const { kind, dialog } = current;
    try {
      if (kind === "clear") {
        await api.clearHistory(dialog.id);
        patch(dialog.id, { lastText: "", unread: 0, unreadReactions: 0 });
        if (dialog.id === activeId) chatEpoch += 1;
      } else {
        await api.deleteChat(dialog.id);
        dialogs = dialogs.filter((d) => d.id !== dialog.id);
        if (activeId === dialog.id) activeId = null;
      }
    } catch (err) {
      showToast(describeError(err));
    }
  }

  async function forwardTo(target: Dialog) {
    const job = forwarding;
    forwarding = null;
    if (!job) return;
    try {
      const sent = await api.forwardMessages(job.from, job.ids, target.id);
      const last = sent[sent.length - 1];
      if (last) showLatest(last);
      showToast(`Переслано в «${dialogTitle(target)}»`);
    } catch (err) {
      showToast(describeError(err));
    }
  }

  const pendingTexts = $derived.by(() => {
    if (!pendingDelete) return null;
    const { kind, dialog } = pendingDelete;
    const name = dialogTitle(dialog);
    if (kind === "clear") {
      return {
        title: "Очистить историю",
        text: `Вся переписка в чате «${name}» будет удалена. Это нельзя отменить.`,
        label: "Очистить",
      };
    }
    if (dialog.kind === "group" || dialog.kind === "channel") {
      const noun = dialog.kind === "group" ? "группу" : "канал";
      return { title: `Покинуть ${noun}`, text: `Вы покинете «${name}».`, label: "Покинуть" };
    }
    return {
      title: "Удалить чат",
      text: `Чат «${name}» и его история будут удалены у вас.`,
      label: "Удалить",
    };
  });

  async function markRead(dialog: Dialog) {
    try {
      await api.markRead(dialog.id);
      onread(dialog.id);
    } catch (err) {
      error = describeError(err);
    }
  }

  /** Puts `updated` first among the chats of its kind (pinned ones stay put). */
  function lift(updated: Dialog) {
    const rest = dialogs.filter((dialog) => dialog.id !== updated.id);
    if (updated.pinned) {
      const known = rest.length !== dialogs.length;
      dialogs = known
        ? dialogs.map((dialog) => (dialog.id === updated.id ? updated : dialog))
        : [updated, ...rest];
      return;
    }
    const firstUnpinned = rest.findIndex((dialog) => !dialog.pinned);
    const at = firstUnpinned === -1 ? rest.length : firstUnpinned;
    dialogs = [...rest.slice(0, at), updated, ...rest.slice(at)];
  }

  /** Reflects a new message in the list, adding the chat if it is not there yet. */
  function showLatest(message: Message, peer?: UiEvent & { type: "message" }) {
    const current = dialogs.find((dialog) => dialog.id === message.peerId);
    if (!current && !peer) return;
    // A chat that is not in the list and arrived without its details (short
    // updates carry none, e.g. a message sent from the phone): reload the
    // list instead of showing a nameless placeholder row.
    if (!current && !peer!.peer.title && peer!.peer.kind !== "saved") {
      loadPage(true);
      return;
    }
    const seen = message.outgoing || (message.peerId === activeId && document.hasFocus());
    const base: Dialog = current ?? {
      id: message.peerId,
      kind: peer!.peer.kind,
      title: peer!.peer.title,
      photoId: peer!.peer.photoId,
      lastText: "",
      lastDate: 0,
      lastOutgoing: false,
      lastId: 0,
      unread: 0,
      pinned: false,
      archived: false,
      readOutbox: 0,
      unreadReactions: 0,
      muted: false,
      contact: false,
      bot: false,
      presence: null,
    };
    // An older message arriving late must not replace a newer preview.
    if (current && message.date < current.lastDate) return;
    lift({
      ...base,
      lastText: message.text || `[${message.kind}]`,
      lastDate: message.date,
      lastOutgoing: message.outgoing,
      lastId: message.id,
      unread: seen ? base.unread : base.unread + 1,
    });
  }

  function onsent(message: Message) {
    showLatest(message);
  }

  function onevents(events: UiEvent[]) {
    for (const event of events) {
      if (event.type === "connection") {
        online = event.online;
        // Whatever happened while offline was missed: start from a fresh list.
        if (event.online) loadPage(true);
      } else if (event.type === "readOutbox") {
        const dialog = dialogs.find((d) => d.id === event.peerId);
        if (dialog && event.maxId > dialog.readOutbox) patch(event.peerId, { readOutbox: event.maxId });
      } else if (event.type === "typing") {
        ontyping(event.peerId, event.userId, event.action);
      } else if (event.type === "presence") {
        if (dialogs.some((d) => d.id === event.peerId)) patch(event.peerId, { presence: event.presence });
      } else {
        if (event.type === "message") {
          showLatest(event.message, event);
          if (!event.message.outgoing) ontyping(event.message.peerId, 0, "cancel");
        }
        chat?.receive(event);
      }
    }
  }

  function closeOverlays() {
    drawer = false;
    settings = false;
    saver = false;
    confirmSignOut = false;
  }

  function openSaved() {
    closeOverlays();
    const saved = dialogs.find((dialog) => dialog.kind === "saved");
    if (saved) open(saved);
    else error = "Чат «Избранное» ещё не загружен — прокрутите список чатов ниже.";
  }

  /**
   * Esc peels one layer at a time, as in Telegram Desktop: an open menu,
   * viewer or panel first (they close themselves), then the chat itself.
   * Runs in the capture phase, before those layers react and disappear.
   */
  function onescape(event: KeyboardEvent) {
    // Ctrl+Tab / Ctrl+Shift+Tab: next / previous chat, as in Telegram Desktop.
    if (event.ctrlKey && event.key === "Tab") {
      event.preventDefault();
      const list = dialogs.filter((dialog) => !dialog.archived);
      if (list.length === 0) return;
      const at = list.findIndex((dialog) => dialog.id === activeId);
      const next = at < 0 ? 0 : (at + (event.shiftKey ? -1 : 1) + list.length) % list.length;
      open(list[next]);
      return;
    }
    if (event.key !== "Escape" || event.repeat) return;
    if (contactsOpen) return;
    if (drawer || settings || saver) {
      closeOverlays();
      return;
    }
    if (document.querySelector(".menu[role=menu], .viewer, .confirm, .picker, .compose-bar, .sendbox, .select-bar, .emoji-panel, .chat-search")) return;
    if (infoId !== null) {
      // The overlay closes itself; a docked profile is closed here.
      if (wide) infoId = null;
      return;
    }
    activeId = null;
    // Otherwise the chat's row keeps its keyboard-focus outline.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  }

  async function signOut() {
    try {
      onstate(await api.signOut());
    } catch (err) {
      closeOverlays();
      error = describeError(err);
    }
  }

  start();
</script>

<svelte:window onkeydowncapture={onescape} bind:innerWidth={winW} />

<div
  class="layout"
  class:resizing
  style:grid-template-columns="{sidebarW}px 1fr{dockId !== null ? ` ${DOCK_W}px` : ''}"
>
  <aside>
    <DialogList
      bind:showArchive
      {compact}
      {dialogs}
      {activeId}
      {now}
      {typing}
      {drafts}
      {folders}
      onopenmessage={openFound}
      {loading}
      {error}
      {online}
      onopen={open}
      onend={() => loadPage(false)}
      onmenu={() => (drawer = true)}
      onmarkread={markRead}
      oninfo={(dialog) => ((infoId = dialog.id), (infoProfile = false))}
      onpin={togglePin}
      onunread={markUnread}
      onarchive={toggleArchive}
      onmute={toggleMute}
      onclear={(dialog) => (pendingDelete = { kind: "clear", dialog })}
      ondelete={(dialog) => (pendingDelete = { kind: "delete", dialog })}
    />
    <!-- A focusable separator is a WAI-ARIA "window splitter" widget. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="grip"
      role="separator"
      aria-orientation="vertical"
      aria-label="Ширина списка чатов"
      aria-valuenow={sidebarW}
      aria-valuemin={COMPACT_W}
      aria-valuemax={MAX_W}
      tabindex="0"
      onpointerdown={dragStart}
      onpointermove={dragMove}
      onpointerup={dragEnd}
      onpointercancel={dragEnd}
      ondblclick={resetWidth}
      onkeydown={handleKey}
    ></div>
  </aside>
  <section>
    {#if active}
      {#key `${active.id}:${chatEpoch}`}
        <Chat
          bind:this={chat}
          dialog={active}
          {now}
          typing={typing[active.id] ?? null}
          {onread}
          {onsent}
          oninfo={toggleInfo}
          onforward={(ids) => (forwarding = { from: active.id, ids })}
          onopenchat={openChat}
          initialDraft={drafts[active.id] ?? ""}
          {jumpTo}
          onjumped={() => (jumpTo = null)}
          ondraft={(text) => saveDraft(active.id, text)}
        />
      {/key}
    {:else}
      <p class="empty"><span>Выберите чат, чтобы начать общение</span></p>
    {/if}
    {#if infoId !== null && !wide}
      <InfoPanel
        peerId={infoId}
        profile={infoProfile}
        onclose={() => (infoId = null)}
        onmuted={(muted) => infoId !== null && patch(infoId, { muted })}
      />
    {/if}
  </section>
  {#if dockId !== null}
    <div class="dock">
      {#key dockId}
        <InfoPanel
          peerId={dockId}
          profile={infoId !== null && infoProfile}
          docked
          onclose={closeDock}
          onmuted={(muted) => patch(dockId, { muted })}
        />
      {/key}
    </div>
  {/if}
</div>

{#if pendingDelete && pendingTexts}
  <Confirm
    title={pendingTexts.title}
    text={pendingTexts.text}
    confirmLabel={pendingTexts.label}
    onconfirm={confirmPending}
    oncancel={() => (pendingDelete = null)}
  />
{/if}

{#if forwarding}
  <ForwardPicker {dialogs} onpick={forwardTo} onclose={() => (forwarding = null)} />
{/if}

<Toast />

{#if drawer}
  <div class="backdrop" onclick={closeOverlays} role="presentation"></div>
  <nav class="drawer" aria-label="Меню">
    <div class="profile">
      <Avatar peerId={me.id} photoId={me.photoId} title={me.name} size={52} />
      <span class="name">{me.name}</span>
      {#if me.phone}<span class="dim">+{me.phone}</span>{/if}
    </div>
    <button
      onclick={() => {
        closeOverlays();
        infoId = me.id;
        infoProfile = true;
      }}>Мой профиль</button
    >
    <button onclick={openSaved}>Избранное</button>
    <button
      onclick={() => {
        closeOverlays();
        showArchive = true;
      }}>Архив</button
    >
    <button
      onclick={() => {
        closeOverlays();
        contactsOpen = true;
      }}>Контакты</button
    >
    <button class="toggle" onclick={toggleTheme} role="switch" aria-checked={theme === "dark"}>
      <span>Ночной режим</span>
      <span class="switch" class:on={theme === "dark"}></span>
    </button>
    <button
      onclick={() => {
        closeOverlays();
        loadPage(true);
      }}>Обновить список чатов</button
    >
    <button
      onclick={() => {
        drawer = false;
        saver = true;
      }}>Экономия памяти</button
    >
    <button
      onclick={() => {
        drawer = false;
        settings = true;
      }}>Настройки</button
    >
    <span class="version dim">Oxigram {version}</span>
  </nav>
{/if}

{#if contactsOpen}
  <Contacts {now} onpick={openContact} onclose={() => (contactsOpen = false)} />
{/if}

{#if saver}
  <div class="backdrop" onclick={closeOverlays} role="presentation"></div>
  <div class="modal" role="dialog" aria-label="Экономия памяти">
    <h2>Экономия памяти</h2>
    <p class="dim hint">
      Что включено, то занимает память и трафик. Выключенное не загружается совсем.
    </p>
    <div class="prefs">
      {#each SAVER as option (option.key)}
        <label class="pref">
          <span class="pref-text">
            <span>{option.title}</span>
            <span class="dim">{option.note}</span>
          </span>
          <input
            type="checkbox"
            checked={prefs[option.key]}
            onchange={(event) => {
              prefs[option.key] = event.currentTarget.checked;
              saveSettings();
            }}
          />
        </label>
      {/each}
    </div>
    <div class="actions">
      <button class="ghost" onclick={closeOverlays}>Закрыть</button>
    </div>
  </div>
{/if}

{#if settings}
  <div class="backdrop" onclick={closeOverlays} role="presentation"></div>
  <div class="modal" role="dialog" aria-label="Настройки">
    <h2>Настройки</h2>
    <dl>
      <dt>Версия</dt>
      <dd>{version || "—"}</dd>
      <dt>Аккаунт</dt>
      <dd>{me.name}{me.username ? ` · @${me.username}` : ""}</dd>
      <dt>Ключи API</dt>
      <dd>Свои, хранятся только на этом компьютере. Сменить их можно после выхода из аккаунта.</dd>
    </dl>

    <div class="prefs">
      <div class="pref">
        <span>Размер текста</span>
        <div class="segmented" role="radiogroup" aria-label="Размер текста">
          {#each [["small", "Мелкий"], ["normal", "Обычный"], ["large", "Крупный"]] as [value, label] (value)}
            <button
              role="radio"
              aria-checked={prefs.textSize === value}
              class:on={prefs.textSize === value}
              onclick={() => {
                prefs.textSize = value as typeof prefs.textSize;
                saveSettings();
              }}>{label}</button
            >
          {/each}
        </div>
      </div>
      <label class="pref">
        <span>Запускать вместе с системой (сразу в трей)</span>
        <input type="checkbox" checked={autostart} onchange={toggleAutostart} />
      </label>
      <div class="pref">
        <span>Кэш медиа: {cacheBytes === null ? "…" : formatFileSize(cacheBytes)}</span>
        <button class="ghost" onclick={clearMedia} disabled={!cacheBytes}>Очистить</button>
      </div>
    </div>
    <div class="actions">
      {#if confirmSignOut}
        <span class="dim">Сессия и кэш на этом компьютере будут удалены.</span>
        <button class="ghost" onclick={() => (confirmSignOut = false)}>Отмена</button>
        <button class="danger" onclick={signOut}>Выйти</button>
      {:else}
        <button class="ghost" onclick={closeOverlays}>Закрыть</button>
        <button class="ghost danger-text" onclick={() => (confirmSignOut = true)}>
          Выйти из аккаунта
        </button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .layout {
    height: 100%;
    display: grid;
    grid-template-columns: 320px 1fr;
  }

  .layout.resizing {
    cursor: col-resize;
    user-select: none;
  }

  aside {
    position: relative;
    min-width: 0;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .grip {
    position: absolute;
    top: 0;
    bottom: 0;
    right: -3px;
    z-index: 5;
    width: 6px;
    cursor: col-resize;
    touch-action: none;
  }

  .grip:hover,
  .grip:focus-visible,
  .layout.resizing .grip {
    background: var(--accent);
    opacity: 0.5;
  }

  .grip:focus-visible {
    outline: none;
  }

  section {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--chat-bg);
  }

  .dock {
    min-width: 0;
    min-height: 0;
  }

  .empty {
    height: 100%;
    margin: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .empty span {
    padding: 5px 14px;
    border-radius: 14px;
    background: var(--bg-raised);
    color: var(--text-dim);
    font-size: 13px;
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 15;
    background: rgb(0 0 0 / 45%);
  }

  .drawer {
    position: fixed;
    inset: 0 auto 0 0;
    z-index: 16;
    width: 280px;
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    box-shadow: 4px 0 20px rgb(0 0 0 / 40%);
  }

  .profile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 20px 18px 14px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 6px;
  }

  .name {
    margin-top: 10px;
    font-weight: 600;
  }

  .drawer button {
    padding: 11px 18px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .switch {
    position: relative;
    width: 34px;
    height: 18px;
    border-radius: 9px;
    background: var(--bg-input);
    transition: background 0.15s;
  }

  .switch::after {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-dim);
    transition: transform 0.15s;
  }

  .switch.on {
    background: var(--accent);
  }

  .switch.on::after {
    transform: translateX(16px);
    background: #fff;
  }

  .drawer button:hover {
    background: var(--bg-hover);
  }

  .version {
    margin-top: auto;
    padding: 12px 18px;
    font-size: 12px;
  }

  .modal {
    position: fixed;
    inset: 50% auto auto 50%;
    z-index: 16;
    width: min(420px, calc(100% - 32px));
    transform: translate(-50%, -50%);
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 22px;
    border-radius: 12px;
    background: var(--bg-raised);
    box-shadow: 0 10px 40px rgb(0 0 0 / 50%);
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  dl {
    margin: 0;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 10px 16px;
  }

  dt {
    color: var(--text-dim);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .prefs {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .hint {
    margin: -4px 0 14px;
    font-size: 13px;
  }

  .pref-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .pref-text .dim {
    font-size: 12.5px;
  }

  .pref {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 14px;
  }

  .segmented {
    display: flex;
    border-radius: 8px;
    background: var(--bg-input);
    overflow: hidden;
  }

  .segmented button {
    padding: 5px 10px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text-dim);
    font-size: 13px;
    font-weight: 500;
  }

  .segmented button.on {
    background: var(--accent);
    color: #fff;
  }

  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    flex-wrap: wrap;
  }

  .actions span {
    flex: 1 1 100%;
    font-size: 13px;
  }

  .danger {
    background: var(--danger);
    color: #fff;
  }

  .danger-text {
    color: var(--danger);
  }
</style>
