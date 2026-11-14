<script lang="ts">
  import { tick } from "svelte";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import {
    api,
    videoInfo,
    dayKey,
    describeError,
    dialogTitle,
    formatDay,
    formatDuration,
    toMarkup,
    formatFileSize,
    formatTime,
    hasOwnMessageIds,
    isOnline,
    presenceText,
    kindLabel,
    messageFileUrl,
    type Dialog,
    type Message,
    type TypingAction,
    typingText,
    type UiEvent,
  } from "../lib/ipc";
  import Avatar from "../lib/Avatar.svelte";
  import CustomEmoji from "../lib/CustomEmoji.svelte";
  import { settings as prefs } from "../lib/settings.svelte";
  import MessageImage from "../lib/MessageImage.svelte";
  import Progress from "../lib/Progress.svelte";
  import SendFiles from "../lib/SendFiles.svelte";
  import RichText from "../lib/RichText.svelte";
  import EmojiPicker from "../lib/EmojiPicker.svelte";
  import Confirm from "../lib/Confirm.svelte";
  import { showToast } from "../lib/notify.svelte";
  import { openMenu, type MenuItem } from "../lib/menu.svelte";

  let {
    dialog,
    onread,
    onsent,
    oninfo,
    onforward,
    onopenchat,
    now,
    typing = null,
    initialDraft = "",
    ondraft,
    jumpTo = null,
    onjumped,
  }: {
    /** Scroll to this message once the history is loaded. */
    jumpTo?: number | null;
    onjumped: () => void;
    /** Unsent text left in this chat earlier. */
    initialDraft?: string;
    /** Called with the unsent text when the chat is closed. */
    ondraft: (text: string) => void;
    /** Open another chat: by @username (mentions, t.me links) or by ID. */
    onopenchat: (target: { username?: string; id?: number }) => void;
    /** The user wants to forward these messages of this chat. */
    onforward: (ids: number[]) => void;
    dialog: Dialog;
    /** Unix seconds, for the "был(а) …" line. */
    now: number;
    /** Someone is typing here right now. */
    typing?: { userId: number; action: TypingAction } | null;
    onread: (peerId: number) => void;
    onsent: (message: Message) => void;
    oninfo: () => void;
  } = $props();

  const PAGE = 50;

  /** How the backend names a custom emoji reaction: this, then its document ID. */
  const CUSTOM = "custom:";
  /** A paid (Telegram Stars) reaction: shown, but never set from here. */
  const PAID = "paid";

  /**
   * The reactions to draw. Unless custom emoji are switched on, they are not
   * fetched at all and are counted together as one sparkle.
   */
  function shownReactions(list: Message["reactions"]): Message["reactions"] {
    if (prefs.customEmoji) return list;
    const plain = list.filter((reaction) => reaction.emoji && !reaction.emoji.startsWith(CUSTOM));
    const custom = list.filter((reaction) => !reaction.emoji || reaction.emoji.startsWith(CUSTOM));
    if (custom.length === 0) return plain;
    return [
      ...plain,
      {
        emoji: "",
        count: custom.reduce((sum, reaction) => sum + reaction.count, 0),
        mine: custom.some((reaction) => reaction.mine),
      },
    ];
  }
  /** Start fetching older messages this close to the top, in pixels. */
  const LOAD_MARGIN = 300;
  /** Closer than this to the end counts as "at the bottom". */
  const BOTTOM_MARGIN = 80;
  const URL_PATTERN = /(https?:\/\/[^\s<>"']+[^\s<>"'.,;:!?)\]])/g;

  // Oldest first, the order they are drawn in.
  let messages = $state.raw<Message[]>([]);
  let loading = $state(true);
  let loadingOlder = $state(false);
  /** The server has nothing older than what is shown. */
  let reachedStart = $state(false);
  let error = $state("");
  // svelte-ignore state_referenced_locally
  let draft = $state(initialDraft);
  // Whatever is left unsent stays with this chat, as in Telegram.
  $effect(() => () => ondraft(draft));
  /** First unread message when the chat was opened, marked with a divider. */
  let unreadFrom = $state<number | null>(null);
  let sending = $state(false);
  let atBottom = $state(true);
  /** Messages that arrived while the user was reading older ones. */
  let unseen = $state(0);
  /** The full-window viewer for a photo or a video. */
  type Viewer = {
    video: boolean;
    /** What is shown right now: the thumbnail first, then the full file. */
    src: string;
    /** The full file is still downloading. */
    loading: boolean;
    error: string;
    size: number;
    message: Message;
  };
  // Raw: `view` compares it by identity to drop the result of a viewer
  // that was closed meanwhile, which a deep proxy would break.
  let viewer = $state.raw<Viewer | null>(null);
  /** Bytes of the viewed file downloaded so far. */
  let viewerBytes = $state(0);
  /** Files being saved or saved to Downloads, by message ID. */
  let saves = $state<Record<number, { path: string; busy: boolean; error: string }>>({});
  /** Attachment kinds that are a file one can keep. */
  const SAVABLE = new Set(["photo", "video", "gif", "round", "voice", "audio", "document", "sticker"]);
  /** Audio players that were started, by message ID. */
  let audio = $state<Record<number, { src: string; loading: boolean; error: string }>>({});

  /** Quick reactions offered in the message menu. */
  const QUICK = ["👍", "❤", "🔥", "😂", "😮", "😢", "🙏"];
  /** Reactions this chat allows: `null` any, a list only these; unknown at first. */
  let allowed = $state.raw<string[] | null>(null);

  /** The quick reactions for the menu: the usual ones the chat allows first. */
  function quickReactions(): string[] {
    if (!allowed) return QUICK;
    const usual = QUICK.filter((emoji) => allowed?.includes(emoji));
    const others = allowed.filter((emoji) => !QUICK.includes(emoji));
    return [...usual, ...others].slice(0, QUICK.length);
  }
  /** The message the next send replies to. */
  let replyTo = $state.raw<Message | null>(null);
  /** The message whose text is being edited in the input. */
  let editing = $state.raw<Message | null>(null);
  /** What was typed before editing began; restored afterwards. */
  let savedDraft = "";
  let confirmDel = $state<{ ids: number[]; checkbox: string } | null>(null);

  let scroller = $state<HTMLDivElement>();
  let content = $state<HTMLDivElement>();
  let input = $state<HTMLTextAreaElement>();

  // This component is re-created per chat (`{#key}` in Main), so the props
  // read here never change during its lifetime.
  // svelte-ignore state_referenced_locally
  const peerId = dialog.id;
  // svelte-ignore state_referenced_locally
  const showSenders = dialog.kind === "group";

  /** A bubble: one message, or several that form an album. */
  type Item = {
    key: number;
    first: Message;
    parts: Message[];
    text: string;
    /** Drawn on the right: own messages, and everything in «Избранное». */
    out: boolean;
    /** Continues the run of the previous bubble (small gap above). */
    joined: boolean;
    /** Last of its run: gets the tail corner. */
    tail: boolean;
  };

  // Same reasoning as above: constant for the lifetime of the component.
  // svelte-ignore state_referenced_locally
  const saved = dialog.kind === "saved";
  /** Longest pause inside one run of a group member's messages, in seconds. */
  const RUN_GAP = 300;
  /** Name colours by sender, Telegram's order, tuned for the dark background. */
  const NAME_COLORS = ["#ff7a7a", "#ffa94d", "#b794f6", "#6fd388", "#4fd1d9", "#6ab3f3", "#ff85b8"];

  function nameColor(senderId: number | null): string {
    return NAME_COLORS[Math.abs(senderId ?? 0) % NAME_COLORS.length];
  }

  function hasImage(message: Message): boolean {
    return message.imageW > 0 && message.imageH > 0;
  }

  /** Has a bubble to join to a run (stickers and service lines do not). */
  function bubbled(message: Message): boolean {
    return message.kind !== "service" && !(message.kind === "sticker" && hasImage(message));
  }

  function continues(prev: Item, next: Item): boolean {
    const a = prev.parts[prev.parts.length - 1];
    const b = next.first;
    if (!bubbled(a) || !bubbled(b) || prev.out !== next.out) return false;
    if (dayKey(a.date) !== dayKey(b.date)) return false;
    if (showSenders && !next.out) return a.senderId === b.senderId && b.date - a.date <= RUN_GAP;
    return true;
  }

  const items = $derived.by(() => {
    const result: Item[] = [];
    for (const message of messages) {
      const last = result[result.length - 1];
      if (last && message.groupedId && last.first.groupedId === message.groupedId) {
        last.parts.push(message);
        if (message.text) last.text = last.text ? `${last.text}\n${message.text}` : message.text;
      } else {
        result.push({
          key: message.id,
          first: message,
          parts: [message],
          text: message.text,
          out: saved || message.outgoing,
          joined: false,
          tail: true,
        });
      }
    }
    for (let i = 1; i < result.length; i++) {
      if (continues(result[i - 1], result[i])) {
        result[i].joined = true;
        result[i - 1].tail = false;
      }
    }
    return result;
  });

  /** Splits text into plain runs and links. */
  /** t.me links open the chat here; anything else goes to the browser. */
  function openLink(url: string) {
    const match = /^(?:https?:\/\/)?(?:www\.)?(?:t\.me|telegram\.me)\/([A-Za-z][A-Za-z0-9_]{3,31})\/?(?:\?.*)?$/.exec(url);
    if (match && !["joinchat", "addstickers", "share", "proxy", "socks"].includes(match[1].toLowerCase())) {
      onopenchat({ username: match[1] });
    } else {
      openUrl(url).catch(() => {});
    }
  }

  function segments(text: string): { text: string; link: boolean }[] {
    return text
      .split(URL_PATTERN)
      .filter((part) => part !== "")
      .map((part, index) => ({ text: part, link: index % 2 === 1 }));
  }

  // Bubbles get their real height only once drawn and pictures arrive later,
  // so while the user is at the bottom every growth re-pins the view there.
  $effect(() => {
    if (!content) return;
    const observer = new ResizeObserver(() => {
      if (atBottom && scroller) scroller.scrollTop = scroller.scrollHeight;
    });
    observer.observe(content);
    return () => observer.disconnect();
  });

  /** Opens a picture or video full-window, fetching the real file behind the thumbnail. */
  async function view(message: Message, thumbnail: string) {
    const video = message.kind === "video" || message.kind === "gif" || message.kind === "round";
    const opened: Viewer = {
      video,
      src: thumbnail,
      loading: message.playable,
      error: video && !message.playable ? "Этот формат видео пока не воспроизводится." : "",
      size: message.fileSize,
      message,
    };
    viewer = opened;
    viewerBytes = 0;
    if (!message.playable) return;
    try {
      const src = await messageFileUrl(message.peerId, message.id, (bytes) => {
        if (viewer === opened) viewerBytes = bytes;
      });
      // Ignore the result if the user closed the viewer or opened another file.
      if (viewer === opened) viewer = { ...opened, src, loading: false };
    } catch (err) {
      if (viewer === opened) viewer = { ...opened, loading: false, error: describeError(err) };
    }
  }

  async function play(message: Message) {
    if (audio[message.id]) return;
    audio[message.id] = { src: "", loading: true, error: "" };
    try {
      const src = await messageFileUrl(message.peerId, message.id);
      audio[message.id] = { src, loading: false, error: "" };
    } catch (err) {
      audio[message.id] = { src: "", loading: false, error: describeError(err) };
    }
  }

  async function save(message: Message) {
    const current = saves[message.id];
    if (current?.busy) return;
    if (current?.path) return reveal(current.path);
    saves[message.id] = { path: "", busy: true, error: "" };
    try {
      const path = await api.saveFile(message.peerId, message.id);
      saves[message.id] = { path, busy: false, error: "" };
    } catch (err) {
      saves[message.id] = { path: "", busy: false, error: describeError(err) };
    }
  }

  /** Puts the picture open in the viewer on the clipboard, as PNG. */
  async function copyImage(src: string) {
    try {
      const bitmap = await createImageBitmap(await (await fetch(src)).blob());
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      canvas.getContext("2d")?.drawImage(bitmap, 0, 0);
      bitmap.close();
      const png = await canvas.convertToBlob({ type: "image/png" });
      await navigator.clipboard.write([new ClipboardItem({ "image/png": png })]);
      showToast("Картинка скопирована");
    } catch {
      showToast("Не удалось скопировать картинку");
    }
  }

  /** Right click in the full-screen viewer. */
  function viewerMenu(event: MouseEvent) {
    const opened = viewer;
    if (!opened || opened.loading) return event.preventDefault();
    const { message, src } = opened;
    const items: MenuItem[] = [];
    if (src && !opened.video) {
      items.push({ label: "Копировать картинку", icon: "copy", action: () => copyImage(src) });
    }
    if (message && SAVABLE.has(message.kind)) {
      items.push({
        label: saves[message.id]?.path ? "Показать в папке" : "Сохранить в «Загрузки»",
        icon: "save",
        action: () => save(message),
      });
    }
    if (message) {
      items.push({
        label: "Переслать",
        icon: "forward",
        action: () => {
          viewer = null;
          onforward([message.id]);
        },
      });
      if (message.text) {
        items.push({
          label: "Копировать подпись",
          icon: "copy",
          action: () => navigator.clipboard.writeText(message.text),
        });
      }
    }
    openMenu(event, items);
  }

  function reveal(path: string) {
    revealItemInDir(path).catch((err) => (error = describeError(err)));
  }

  const byId = $derived(new Map(messages.map((message) => [message.id, message])));

  function senderOf(message: Message): string {
    return message.outgoing ? "Вы" : message.senderName || dialogTitle(dialog);
  }

  /** One line describing a message, for quotes and the reply bar. */
  function snippet(message: Message): string {
    const line = (message.text || kindLabel(message.kind) || "Сообщение").split("\n")[0];
    return line;
  }

  function startReply(message: Message) {
    if (editing) cancelCompose();
    replyTo = message;
    input?.focus();
  }

  function startEdit(message: Message) {
    if (!editing) savedDraft = draft;
    replyTo = null;
    editing = message;
    draft = toMarkup(message.text, message.entities);
    input?.focus();
  }

  function cancelCompose() {
    if (editing) draft = savedDraft;
    editing = null;
    replyTo = null;
  }

  function scrollToMessage(id: number) {
    const target = items.find((item) => item.parts.some((part) => part.id === id));
    const element = target && content?.querySelector<HTMLElement>(`[data-key="${target.key}"]`);
    if (!element) return;
    element.scrollIntoView({ block: "center", behavior: "smooth" });
    element.classList.add("flash");
    setTimeout(() => element.classList.remove("flash"), 1300);
  }

  /** Sets (or, if already chosen, removes) the own reaction; shown at once. */
  function react(target: Message, emoji: string) {
    const old = target.reactions;
    const next = old.some((r) => r.mine && r.emoji === emoji) ? "" : emoji;
    let reactions = old
      .map((r) => (r.mine ? { ...r, count: r.count - 1, mine: false } : r))
      .filter((r) => r.count > 0);
    if (next) {
      reactions = reactions.some((r) => r.emoji === next)
        ? reactions.map((r) => (r.emoji === next ? { ...r, count: r.count + 1, mine: true } : r))
        : [...reactions, { emoji: next, count: 1, mine: true }];
    }
    const set = (value: Message["reactions"]) =>
      (messages = messages.map((m) => (m.id === target.id ? { ...m, reactions: value } : m)));
    set(reactions);
    api.react(peerId, target.id, next).catch((err) => {
      set(old);
      error = describeError(err);
    });
  }

  let emojiOpen = $state(false);

  /** Puts an emoji where the cursor is in the input. */
  async function insertEmoji(emoji: string) {
    const field = input;
    const start = field?.selectionStart ?? draft.length;
    const end = field?.selectionEnd ?? draft.length;
    draft = draft.slice(0, start) + emoji + draft.slice(end);
    await tick();
    field?.focus();
    field?.setSelectionRange(start + emoji.length, start + emoji.length);
  }

  /** Arrow Up in an empty input edits the last own text message, as in Telegram. */
  function editLast() {
    for (let i = messages.length - 1; i >= 0; i--) {
      const message = messages[i];
      if (message.outgoing && message.kind === "text" && !message.groupedId) {
        startEdit(message);
        return true;
      }
    }
    return false;
  }

  // Searching inside the chat: the magnifier in the header.
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchResults = $state.raw<Message[]>([]);
  let searchBusy = $state(false);
  let searchInput = $state<HTMLInputElement>();

  $effect(() => {
    const needle = searchQuery.trim();
    searchResults = [];
    if (!searchOpen || needle.length < 1) return;
    searchBusy = true;
    let live = true;
    const timer = setTimeout(() => {
      api.searchChat(peerId, needle).then(
        (list) => live && (searchResults = list),
        (err) => live && (error = describeError(err)),
      ).finally(() => live && (searchBusy = false));
    }, 350);
    return () => {
      live = false;
      clearTimeout(timer);
      searchBusy = false;
    };
  });

  async function openSearch() {
    searchOpen = true;
    await tick();
    searchInput?.focus();
  }

  function closeSearch() {
    searchOpen = false;
    searchQuery = "";
  }

  /** Brings a message into view, loading older pages until it is there. */
  async function jump(id: number) {
    for (let i = 0; i < 30 && !messages.some((m) => m.id === id) && !reachedStart; i++) {
      await loadOlder();
    }
    await tick();
    scrollToMessage(id);
  }

  // Selecting several messages: "Выделить" in the menu, then clicks toggle.
  let selected = $state<Set<number>>(new Set());
  const selecting = $derived(selected.size > 0);
  const selectedItems = $derived(items.filter((item) => selected.has(item.key)));

  function toggleSelect(item: Item) {
    const next = new Set(selected);
    if (next.has(item.key)) next.delete(item.key);
    else next.add(item.key);
    selected = next;
  }

  function clearSelection() {
    selected = new Set();
  }

  /** While selecting, a click anywhere on a bubble toggles it instead. */
  function selectClick(event: MouseEvent, item: Item) {
    if (!selecting) return;
    event.preventDefault();
    event.stopPropagation();
    toggleSelect(item);
  }

  function selectedIds(): number[] {
    return selectedItems.flatMap((item) => item.parts.map((part) => part.id)).sort((a, b) => a - b);
  }

  function copySelected() {
    const text = selectedItems
      .map((item) => item.text)
      .filter((text) => text)
      .join("\n\n");
    if (text) navigator.clipboard.writeText(text);
    showToast(text ? "Текст скопирован" : "В выделенных сообщениях нет текста");
    clearSelection();
  }

  function forwardSelected() {
    onforward(selectedIds());
    clearSelection();
  }

  function deleteSelected() {
    const own = selectedItems.every((item) => item.parts.every((part) => part.outgoing));
    const checkbox =
      !hasOwnMessageIds(peerId) && own && !saved
        ? dialog.kind === "user"
          ? "Удалить также у собеседника"
          : "Удалить также у всех"
        : "";
    confirmDel = { ids: selectedIds(), checkbox };
    clearSelection();
  }

  function askDelete(item: Item) {
    const own = item.parts.every((part) => part.outgoing);
    const checkbox =
      !hasOwnMessageIds(peerId) && own && !saved
        ? dialog.kind === "user"
          ? "Удалить также у собеседника"
          : "Удалить также у всех"
        : "";
    confirmDel = { ids: item.parts.map((part) => part.id), checkbox };
  }

  async function doDelete(checked: boolean) {
    const job = confirmDel;
    confirmDel = null;
    if (!job) return;
    const forEveryone = hasOwnMessageIds(peerId) || (job.checkbox !== "" && checked);
    try {
      await api.deleteMessages(peerId, job.ids, forEveryone);
      const gone = new Set(job.ids);
      messages = messages.filter((message) => !gone.has(message.id));
      if (replyTo && gone.has(replyTo.id)) replyTo = null;
      if (editing && gone.has(editing.id)) cancelCompose();
    } catch (err) {
      error = describeError(err);
    }
  }

  /** The newest pinned message, shown as a bar under the header. */
  let pinned = $state<Message | null>(null);
  let pinnedHidden = $state(false);

  async function loadPinned() {
    try {
      const [latest] = await api.sharedMedia(peerId, "pinned", null, 1);
      pinned = latest ?? null;
      pinnedHidden = false;
    } catch {
      // Not essential: the bar simply stays as it was.
    }
  }

  // In groups the typing line names who types, when their name is known.
  const typingLine = $derived.by(() => {
    if (!typing) return "";
    const text = typingText(typing.action);
    if (dialog.kind === "user") return text;
    let who = "";
    for (let i = messages.length - 1; i >= 0 && !who; i--) {
      if (messages[i].senderId === typing.userId) who = messages[i].senderName;
    }
    return who ? `${who.split(" ")[0]} ${text}` : text;
  });

  /** When the typing status was last sent, to repeat it at most every 5 s. */
  let typingSentAt = 0;
  function announceTyping() {
    if (dialog.kind === "channel" || dialog.kind === "saved" || editing) return;
    const now = Date.now();
    if (now - typingSentAt < 5000) return;
    typingSentAt = now;
    api.setTyping(peerId).catch(() => {});
  }

  async function pinMessage(message: Message) {
    try {
      await api.pinMessage(peerId, message.id, true);
      pinned = message;
      pinnedHidden = false;
      showToast("Сообщение закреплено");
    } catch (err) {
      error = describeError(err);
    }
  }

  function messageMenu(event: MouseEvent, item: Item) {
    const first = item.first;
    if (first.kind === "service") return;
    const items: MenuItem[] = [{ label: "Ответить", icon: "reply", action: () => startReply(first) }];
    for (const media of item.parts) {
      if (!SAVABLE.has(media.kind)) continue;
      const done = saves[media.id]?.path;
      items.push(
        done
          ? { label: "Показать в папке", icon: "folder", action: () => reveal(done) }
          : {
              icon: "save",
              label: item.parts.filter((m) => SAVABLE.has(m.kind)).length > 1 ? "Сохранить все в «Загрузки»" : "Сохранить в «Загрузки»",
              action: () => item.parts.filter((m) => SAVABLE.has(m.kind)).forEach(save),
            },
      );
      break;
    }
    const selection = window.getSelection()?.toString() ?? "";
    if (selection) {
      items.push({
        label: "Копировать выделенное",
        icon: "copy",
        action: () => navigator.clipboard.writeText(selection),
      });
    }
    if (item.text) {
      items.push({
        label: "Копировать текст",
        icon: "copy",
        action: () => navigator.clipboard.writeText(item.text),
      });
    }
    const link = (event.target as HTMLElement).closest("a")?.getAttribute("href");
    if (link) {
      items.push({
        label: "Копировать ссылку",
        icon: "link",
        action: () => navigator.clipboard.writeText(link),
      });
      items.push({ label: "Открыть ссылку", icon: "link", action: () => openUrl(link) });
    }
    if (first.outgoing && first.kind === "text" && item.parts.length === 1) {
      items.push({ label: "Изменить", icon: "edit", action: () => startEdit(first) });
    }
    items.push({ label: "Закрепить", icon: "pin", action: () => pinMessage(first) });
    items.push({
      label: "Переслать",
      icon: "forward",
      action: () => onforward(item.parts.map((part) => part.id)),
    });
    items.push({ label: "Выделить", icon: "select", action: () => toggleSelect(item) });
    items.push({ label: "Удалить", icon: "delete", danger: true, action: () => askDelete(item) });

    const target = item.parts.find((part) => part.reactions.length > 0) ?? first;
    const quick = quickReactions();
    openMenu(
      event,
      items,
      quick.length > 0
        ? {
            emojis: quick,
            active: target.reactions.find((r) => r.mine)?.emoji ?? null,
            pick: (emoji) => react(target, emoji),
          }
        : null,
    );
  }

  function albumColumns(count: number): number {
    return count === 2 || count === 4 ? 2 : 3;
  }

  async function scrollToBottom() {
    await tick();
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
    atBottom = true;
    unseen = 0;
  }

  async function markRead() {
    try {
      await api.markRead(peerId);
      onread(peerId);
    } catch {
      // Not worth interrupting the user: the next open tries again.
    }
  }

  async function open() {
    loadPinned();
    api.allowedReactions(peerId).then(
      (list) => (allowed = list),
      () => {},
    );
    try {
      messages = (await api.cachedHistory(peerId, null, PAGE)).reverse();
      await scrollToBottom();
    } catch {
      // The cache is only a head start; the network load below reports errors.
    }
    try {
      const fresh = (await api.history(peerId, null, PAGE)).reverse();
      messages = fresh;
      reachedStart = fresh.length < PAGE;
      // With unread messages, start at the first of them, as Telegram does.
      const incoming = fresh.filter((message) => !message.outgoing);
      if (dialog.unread > 0 && incoming.length > 0) {
        unreadFrom = incoming[Math.max(0, incoming.length - dialog.unread)].id;
      }
      if (unreadFrom !== null && dialog.unread < incoming.length + 1) {
        await tick();
        const divider = content?.querySelector<HTMLElement>(".unread-divider");
        if (divider && scroller) {
          scroller.scrollTop = divider.offsetTop - 12;
          atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < BOTTOM_MARGIN;
        } else {
          await scrollToBottom();
        }
      } else {
        await scrollToBottom();
      }
      if (dialog.unread > 0) markRead();
    } catch (err) {
      error = describeError(err);
    } finally {
      loading = false;
    }
  }

  // A message to show from the global search, now or once history is in.
  $effect(() => {
    if (jumpTo === null || loading) return;
    const target = jumpTo;
    onjumped();
    jump(target);
  });

  async function loadOlder() {
    if (loading || loadingOlder || reachedStart || messages.length === 0 || !scroller) return;
    loadingOlder = true;
    error = "";
    try {
      const older = (await api.history(peerId, messages[0].id, PAGE)).reverse();
      reachedStart = older.length < PAGE;
      if (older.length > 0) {
        // Keep the message under the user's eyes where it was.
        const before = scroller.scrollHeight - scroller.scrollTop;
        messages = [...older, ...messages];
        await tick();
        scroller.scrollTop = scroller.scrollHeight - before;
      }
    } catch (err) {
      error = describeError(err);
    } finally {
      loadingOlder = false;
    }
  }

  function onscroll() {
    if (!scroller) return;
    atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < BOTTOM_MARGIN;
    if (atBottom) unseen = 0;
    if (scroller.scrollTop < LOAD_MARGIN) loadOlder();
  }

  function append(message: Message) {
    if (messages.some((existing) => existing.id === message.id)) return false;
    messages = [...messages, message];
    return true;
  }

  /** Live events from Telegram, routed here by Main while this chat is open. */
  // Sending files: chosen with the clip button, pasted, or dropped.
  let picked = $state<File[] | null>(null);
  /** Formats Telegram accepts as a compressed photo. */
  const PHOTO_TYPES = new Set(["image/jpeg", "image/png", "image/webp", "image/bmp"]);
  /** Telegram's limit of items in one album. */
  const ALBUM_MAX = 10;
  let dragging = $state(false);
  let fileInput = $state<HTMLInputElement>();
  /** Files being uploaded right now, shown as a strip above the input. */
  let uploads = $state<{ token: string; name: string; size: number; done: number }[]>([]);

  function pick(files: FileList | File[] | null | undefined) {
    const list = [...(files ?? [])].filter((file) => file.size > 0);
    // More files while the send box is open join the ones already there.
    if (list.length > 0) picked = picked ? [...picked, ...list] : list;
  }

  async function sendFiles(files: File[], caption: string, asPhoto: boolean) {
    picked = null;
    const reply = replyTo?.id ?? null;
    replyTo = null;
    // What each file is sent as: photos and videos share albums, plain
    // files form their own, as in Telegram.
    const entries = await Promise.all(
      files.map(async (file, index) => {
        const isPicture = PHOTO_TYPES.has(file.type) && file.size <= 10 * 1024 * 1024;
        const photo = asPhoto && isPicture;
        return {
          file,
          token: `${Date.now()}-${index}-${Math.random().toString(36).slice(2)}`,
          asPhoto: photo,
          video: photo ? "" : await videoInfo(file),
        };
      }),
    );
    type Entry = (typeof entries)[number];
    const visual = entries.filter((entry) => entry.asPhoto || entry.video);
    const plain = entries.filter((entry) => !entry.asPhoto && !entry.video);
    const groups: Entry[][] = [];
    for (const list of [visual, plain]) {
      for (let at = 0; at < list.length; at += ALBUM_MAX) groups.push(list.slice(at, at + ALBUM_MAX));
    }
    uploads = [
      ...uploads,
      ...entries.map(({ file, token }) => ({ token, name: file.name || "Файл", size: file.size, done: 0 })),
    ];

    for (const [index, group] of groups.entries()) {
      // The caption goes under the first message only, as in Telegram.
      const text = index === 0 ? caption : "";
      const replyId = index === 0 ? reply : null;
      try {
        let sent: Message[];
        if (group.length === 1) {
          const [{ file, token, asPhoto: photo, video }] = group;
          sent = [await api.sendFile(peerId, file, { caption: text, asPhoto: photo, video, replyTo: replyId, token })];
        } else {
          for (const { file, token } of group) await api.uploadFile(file, token);
          sent = await api.sendAlbum(
            peerId,
            group.map(({ token, asPhoto: photo, video }) => ({ token, asPhoto: photo, video })),
            text,
            replyId,
          );
        }
        for (const message of sent) {
          append(message);
          onsent(message);
        }
        await scrollToBottom();
      } catch (err) {
        error = describeError(err);
      } finally {
        const done = new Set(group.map((entry) => entry.token));
        uploads = uploads.filter((upload) => !done.has(upload.token));
      }
    }
  }

  function onpaste(event: ClipboardEvent) {
    const files = event.clipboardData?.files;
    if (files && files.length > 0) {
      event.preventDefault();
      pick(files);
    }
  }

  function ondragover(event: DragEvent) {
    if (!event.dataTransfer?.types.includes("Files")) return;
    event.preventDefault();
    dragging = true;
  }

  function ondrop(event: DragEvent) {
    dragging = false;
    if (!event.dataTransfer?.files.length) return;
    event.preventDefault();
    pick(event.dataTransfer.files);
  }

  export function receive(event: UiEvent) {
    if (event.type === "upload") {
      uploads = uploads.map((upload) =>
        upload.token === event.token ? { ...upload, done: event.bytes } : upload,
      );
      return;
    }
    if (event.type === "pinned") {
      if (event.peerId === peerId) loadPinned();
      return;
    }
    if (event.type === "message" && event.message.peerId === peerId) {
      if (!append(event.message)) return;
      if (atBottom) scrollToBottom();
      else unseen += 1;
      // Reading happens only while the user is really looking at the chat.
      if (!event.message.outgoing && atBottom && document.hasFocus()) markRead();
    } else if (event.type === "edited" && event.message.peerId === peerId) {
      const edited = event.message;
      messages = messages.map((message) => (message.id === edited.id ? edited : message));
    } else if (event.type === "reactions" && event.peerId === peerId) {
      messages = messages.map((message) =>
        message.id === event.id ? { ...message, reactions: event.reactions } : message,
      );
    } else if (event.type === "deleted") {
      const here = event.peerId === peerId || (event.peerId === null && !hasOwnMessageIds(peerId));
      if (!here) return;
      const gone = new Set(event.ids);
      messages = messages.filter((message) => !gone.has(message.id));
    }
  }

  async function send() {
    const text = draft.trim();
    if (!text || sending) return;
    sending = true;
    error = "";
    try {
      if (editing) {
        const target = editing;
        if (text !== target.text) {
          await api.editMessage(peerId, target.id, text);
          messages = messages.map((m) => (m.id === target.id ? { ...m, text, edited: true } : m));
        }
        cancelCompose();
        return;
      }
      const sent = await api.sendMessage(peerId, text, replyTo?.id ?? null);
      draft = "";
      replyTo = null;
      append(sent);
      onsent(sent);
      await scrollToBottom();
    } catch (err) {
      error = describeError(err);
    } finally {
      sending = false;
      input?.focus();
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      send();
    } else if (event.key === "ArrowUp" && draft === "" && !editing && editLast()) {
      event.preventDefault();
    }
  }

  open();
</script>

{#snippet ticks(message: Message)}
  {#if message.outgoing && (dialog.kind === "user" || dialog.kind === "group")}
    {@const read = message.id <= dialog.readOutbox}
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
{/snippet}

<!-- Capture phase: runs before a menu or modal closes itself and leaves the DOM. -->
<svelte:window
  onkeydowncapture={(event) => {
    // Ctrl+F searches this chat, as in Telegram Desktop.
    if ((event.ctrlKey || event.metaKey) && event.code === "KeyF" && !event.shiftKey) {
      event.preventDefault();
      openSearch();
      return;
    }
    if (event.key !== "Escape") return;
    if (viewer) viewer = null;
    else if (document.querySelector(".menu[role=menu], .confirm, .picker")) return;
    else if (selecting) clearSelection();
    else if (searchOpen) closeSearch();
    else cancelCompose();
  }}
/>

<div
  class="chat"
  role="region"
  aria-label="Чат"
  {ondragover}
  ondragleave={(event) => {
    // Only leaving the chat itself counts, not moving between its children.
    if (!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node)) dragging = false;
  }}
  {ondrop}
>
  {#if dragging}
    <div class="dropzone" aria-hidden="true"><span>Отпустите, чтобы отправить</span></div>
  {/if}
  {#if selecting}
    <div class="select-bar">
      <button class="sb-x" onclick={clearSelection} aria-label="Отменить выделение">✕</button>
      <span class="sb-count">Выбрано: {selectedItems.length}</span>
      <button class="ghost" onclick={copySelected}>Копировать</button>
      <button class="ghost" onclick={forwardSelected}>Переслать</button>
      <button class="ghost danger-text" onclick={deleteSelected}>Удалить</button>
    </div>
  {/if}
  <header class:hidden={selecting}>
    <button class="peer" onclick={oninfo} aria-label="Информация о чате">
      <Avatar
        peerId={dialog.id}
        photoId={dialog.photoId}
        title={dialogTitle(dialog)}
        saved={dialog.kind === "saved"}
        size={36}
      />
      <span class="names">
        <span class="title">{dialogTitle(dialog)}</span>
        {#if typing}
          <span class="status online">{typingLine}</span>
        {:else if dialog.kind === "user" && dialog.presence}
          <span class="status" class:online={isOnline(dialog.presence, now)}
            >{presenceText(dialog.presence, now)}</span
          >
        {/if}
      </span>
    </button>
    <button class="search-btn" onclick={openSearch} aria-label="Поиск в чате">
      <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
        <path fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" d="M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4" />
      </svg>
    </button>
  </header>

  {#if searchOpen}
    <div class="chat-search">
      <input
        bind:this={searchInput}
        bind:value={searchQuery}
        type="search"
        placeholder="Поиск в чате"
        spellcheck="false"
      />
      <span class="cs-count">{searchBusy ? "…" : searchQuery.trim() ? searchResults.length : ""}</span>
      <button class="cs-x" onclick={closeSearch} aria-label="Закрыть поиск">✕</button>
    </div>
    {#if searchResults.length > 0}
      <div class="cs-results">
        {#each searchResults as hit (hit.id)}
          <button class="cs-hit" onclick={() => jump(hit.id)}>
            <span class="cs-who">{hit.outgoing ? "Вы" : hit.senderName || dialogTitle(dialog)}</span>
            <span class="cs-text">{snippet(hit)}</span>
            <span class="cs-date">{formatDay(hit.date)}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}

  {#if pinned && !pinnedHidden}
    <div class="pinned">
      <button class="pin-body" onclick={() => pinned && scrollToMessage(pinned.id)}>
        <span class="pin-title">Закреплённое сообщение</span>
        <span class="pin-text">{snippet(pinned)}</span>
      </button>
      <button class="pin-x" onclick={() => (pinnedHidden = true)} aria-label="Скрыть">✕</button>
    </div>
  {/if}

  <div class="scroll-area">
    <div class="messages" bind:this={scroller} {onscroll}>
      <div class="content" bind:this={content}>
      {#if loadingOlder}
        <p class="note">Загрузка…</p>
      {:else if reachedStart && messages.length > 0}
        <p class="note">Начало переписки</p>
      {/if}

      {#each items as item, index (item.key)}
        {@const message = item.first}
        {@const previous = items[index - 1]?.first}
        {#if !previous || dayKey(previous.date) !== dayKey(message.date)}
          <p class="day"><span>{formatDay(message.date)}</span></p>
        {/if}
        {#if unreadFrom !== null && item.parts.some((part) => part.id === unreadFrom)}
          <p class="unread-divider"><span>Непрочитанные сообщения</span></p>
        {/if}

        {#if message.kind === "service"}
          <p class="note service"><span>{message.text || kindLabel(message.kind)}</span></p>
        {:else if message.kind === "sticker" && hasImage(message)}
          <div
            class="sticker"
            role="article"
            data-key={item.key}
            class:selected={selected.has(item.key)}
            onclickcapture={(event) => selectClick(event, item)}
            oncontextmenu={(event) => messageMenu(event, item)}
            class:out={item.out}
          >
            <MessageImage {message} />
            <span class="meta">{formatTime(message.date)}{@render ticks(message)}</span>
          </div>
        {:else}
          {@const pictures = item.parts.filter(hasImage)}
          {@const files = item.parts.filter((part) => !hasImage(part) && part.kind !== "text")}
          {@const reacts = shownReactions(item.parts.find((part) => part.reactions.length > 0)?.reactions ?? [])}
          <div
            class="message"
            role="article"
            data-key={item.key}
            class:selected={selected.has(item.key)}
            onclickcapture={(event) => selectClick(event, item)}
            oncontextmenu={(event) => messageMenu(event, item)}
            class:out={item.out}
            class:media={pictures.length > 0}
            class:media-only={pictures.length > 0 && !item.text && files.length === 0}
            class:joined={item.joined}
            class:tail={item.tail}
          >
            {#if showSenders && !item.out && !item.joined && message.senderName}
              <span class="sender" style:color={nameColor(message.senderId)}>{message.senderName}</span>
            {/if}

            {#if message.replyToId != null}
              {@const replyId = message.replyToId}
              {@const ref = byId.get(replyId)}
              <button class="quote" onclick={() => scrollToMessage(replyId)}>
                {#if ref}<span class="q-name">{senderOf(ref)}</span>{/if}
                <span class="q-text">{ref ? snippet(ref) : "Сообщение"}</span>
              </button>
            {/if}

            {#if pictures.length === 1}
              <div class="single">
                <MessageImage message={pictures[0]} onopen={(src) => view(pictures[0], src)} />
              </div>
            {:else if pictures.length > 1}
              <div class="album" class:two={albumColumns(pictures.length) === 2}>
                {#each pictures as picture (picture.id)}
                  <div class="cell">
                    <MessageImage message={picture} cover onopen={(src) => view(picture, src)} />
                  </div>
                {/each}
              </div>
            {/if}

            {#each files as file (file.id)}
              {@const sound = file.playable && (file.kind === "voice" || file.kind === "audio")}
              {@const player = audio[file.id]}
              {@const saving = saves[file.id]}
              {@const savable = SAVABLE.has(file.kind)}
              <div class="file">
                {#if sound && player?.src}
                  <!-- svelte-ignore a11y_media_has_caption -->
                  <audio src={player.src} controls autoplay></audio>
                {:else}
                <button
                  class="file-icon"
                  disabled={(!file.playable && !savable) || player?.loading || saving?.busy}
                  onclick={() =>
                    sound ? play(file) : file.playable ? view(file, "") : save(file)}
                  aria-label={file.playable
                    ? "Воспроизвести"
                    : saving?.path
                      ? "Показать в папке"
                      : savable
                        ? "Сохранить в «Загрузки»"
                        : "Файл"}
                >
                  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
                    {#if file.playable}
                      <path fill="currentColor" d="M8 5v14l11-7L8 5z" />
                    {:else if saving?.path}
                      <path fill="none" stroke="currentColor" stroke-width="2" d="M4 7a1 1 0 0 1 1-1h4l2 2h8a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1z" />
                    {:else if saving?.busy}
                      <circle class="spin" cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="2.5" stroke-dasharray="30 60" />
                    {:else}
                      <path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" d="M12 4v11m-5-5 5 5 5-5M5 20h14" />
                    {/if}
                  </svg>
                </button>
                <span class="file-body">
                  <span class="file-name">{file.fileName || kindLabel(file.kind)}</span>
                  <span class="file-size">
                    {#if player?.loading}
                      Загрузка…
                    {:else if player?.error}
                      {player.error}
                    {:else if saving?.busy}
                      Сохранение…
                    {:else if saving?.error}
                      {saving.error}
                    {:else if saving?.path}
                      <button class="link small" onclick={() => reveal(saving.path)}>
                        Сохранено · показать в папке
                      </button>
                    {:else}
                      {file.duration > 0 ? `${formatDuration(file.duration)} · ` : ""}{file.fileName
                        ? `${kindLabel(file.kind)} · `
                        : ""}{file.fileSize > 0 ? formatFileSize(file.fileSize) : ""}
                    {/if}
                  </span>
                </span>
                {/if}
              </div>
            {/each}

            {#if item.text || files.length > 0 || pictures.length === 0 || reacts.length > 0}
              <div class="body">
                {#if item.text}{@const textPart = item.parts.find((part) => part.text === item.text)}<span
                    class="text"
                    ><RichText
                      text={item.text}
                      entities={textPart?.entities ?? []}
                      onlink={openLink}
                      onmention={(username) => onopenchat({ username })}
                      onmentionid={(id) => onopenchat({ id })}
                    /></span
                  >{/if}{#if item.parts.find((part) => part.web)?.web}{@const web = item.parts.find((part) => part.web)!.web!}<button
                    class="web"
                    onclick={() => openLink(web.url)}
                    >{#if web.site}<span class="web-site">{web.site}</span>{/if}{#if web.title}<span class="web-title"
                        >{web.title}</span
                      >{/if}{#if web.description}<span class="web-desc">{web.description}</span>{/if}</button
                  >{/if}{#if reacts.length > 0}
                  <div class="foot">
                    <div class="reactions">
                      {#each reacts as reaction, at (`${at}:${reaction.emoji}`)}
                        <button
                          class="reaction"
                          class:mine={reaction.mine}
                          disabled={reaction.emoji === "" || reaction.emoji === PAID}
                          onclick={() => {
                            const target = item.parts.find((part) => part.reactions.length > 0);
                            if (target) react(target, reaction.emoji);
                          }}
                          >{#if reaction.emoji.startsWith(CUSTOM)}<CustomEmoji
                              id={reaction.emoji.slice(CUSTOM.length)}
                            />{:else if reaction.emoji === PAID}⭐{:else}{reaction.emoji || "✨"}{/if}{#if reaction.count > 1}<b>{reaction.count}</b>{/if}</button
                        >
                      {/each}
                    </div>
                    <span class="meta"
                      >{message.edited ? "изменено · " : ""}{formatTime(message.date)}{@render ticks(message)}</span
                    >
                  </div>
                {:else}<span class="meta"
                  >{message.edited ? "изменено · " : ""}{formatTime(message.date)}{@render ticks(message)}</span
                >{/if}
              </div>
            {:else}
              <span class="meta"
                >{message.edited ? "изменено · " : ""}{formatTime(message.date)}{@render ticks(message)}</span
              >
            {/if}
          </div>
        {/if}
      {/each}

      {#if messages.length === 0}
        <p class="note">{loading ? "Загрузка сообщений…" : "Сообщений нет"}</p>
      {/if}
      </div>
    </div>

    {#if !atBottom}
      <button class="to-bottom" onclick={scrollToBottom} aria-label="К последним сообщениям">
        {#if unseen > 0}<span class="badge">{unseen}</span>{/if}
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <path
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M6 9l6 6 6-6"
          />
        </svg>
      </button>
    {/if}
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  {#if editing || replyTo}
    {@const target = (editing ?? replyTo)!}
    <div class="compose-bar">
      <div class="cb-body">
        <span class="cb-title">{editing ? "Редактирование" : `Ответ: ${senderOf(target)}`}</span>
        <span class="cb-text">{snippet(target)}</span>
      </div>
      <button class="cb-x" onclick={cancelCompose} aria-label="Отменить">✕</button>
    </div>
  {/if}

  {#each uploads as upload (upload.token)}
    <div class="upload">
      <span class="up-name">Отправка: {upload.name}</span>
      <span class="up-bar"><span style:width="{upload.size > 0 ? Math.min(100, (upload.done / upload.size) * 100) : 0}%"></span></span>
      <span class="up-pct">{upload.size > 0 ? Math.min(100, Math.floor((upload.done / upload.size) * 100)) : 0}%</span>
    </div>
  {/each}

  <footer class:has-bar={editing || replyTo}>
    <button class="attach" onclick={() => fileInput?.click()} aria-label="Прикрепить файл" disabled={!!editing}>
      <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
        <path
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M20.5 11.5l-8.1 8.1a5.2 5.2 0 0 1-7.4-7.4l8.4-8.4a3.5 3.5 0 0 1 4.9 4.9l-8.4 8.4a1.7 1.7 0 0 1-2.4-2.4l7.7-7.7"
        />
      </svg>
    </button>
    <input
      bind:this={fileInput}
      class="file-input"
      type="file"
      multiple
      onchange={(event) => {
        const target = event.currentTarget as HTMLInputElement;
        pick(target.files);
        target.value = "";
      }}
    />
    <button class="attach" onclick={() => (emojiOpen = !emojiOpen)} aria-label="Эмодзи">
      <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
        <path
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM8.5 14.5a4.5 4.5 0 0 0 7 0M9 9.5v.5M15 9.5v.5"
        />
      </svg>
    </button>
    {#if emojiOpen}
      <EmojiPicker onpick={insertEmoji} onclose={() => (emojiOpen = false)} />
    {/if}
    <textarea
      oninput={announceTyping}
      {onpaste}
      bind:this={input}
      bind:value={draft}
      {onkeydown}
      rows="1"
      placeholder="Сообщение"
      maxlength="4096"
    ></textarea>
    <button
      class="send"
      disabled={sending || draft.trim().length === 0}
      onclick={send}
      aria-label="Отправить"
    >
      <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
        <path fill="currentColor" d="M3 20.5v-7l10-1.5-10-1.5v-7L22 12 3 20.5z" />
      </svg>
    </button>
  </footer>
</div>

{#if picked}
  <SendFiles
    files={picked}
    onsend={sendFiles}
    onadd={() => fileInput?.click()}
    onpick={pick}
    onremove={(index) => {
      const rest = (picked ?? []).filter((_, at) => at !== index);
      picked = rest.length > 0 ? rest : null;
    }}
    onclose={() => (picked = null)}
  />
{/if}

{#if confirmDel}
  <Confirm
    title={confirmDel.ids.length > 1 ? `Удалить ${confirmDel.ids.length} сообщ.?` : "Удалить сообщение?"}
    checkbox={confirmDel.checkbox}
    confirmLabel="Удалить"
    onconfirm={doDelete}
    oncancel={() => (confirmDel = null)}
  />
{/if}

{#if viewer}
  <div class="viewer" role="dialog" aria-label="Просмотр" tabindex="-1" oncontextmenu={viewerMenu}>
    <button class="viewer-close" onclick={() => (viewer = null)} aria-label="Закрыть">✕</button>
    {#if viewer.message && SAVABLE.has(viewer.message.kind)}
      {@const saving = saves[viewer.message.id]}
      <button
        class="viewer-save"
        disabled={saving?.busy}
        onclick={() => viewer && save(viewer.message)}
      >
        {saving?.busy
          ? "Сохранение…"
          : saving?.path
            ? "Показать в папке"
            : saving?.error
              ? saving.error
              : "Сохранить"}
      </button>
    {/if}
    <!-- While the original downloads only a progress ring shows: a blurry
         preview stretched to full size looks broken. -->
    {#if viewer.loading}
      <div class="viewer-loading">
        <Progress done={viewerBytes} total={viewer.size} size={84} />
        {#if viewer.size > 0}
          <span>{formatFileSize(viewerBytes)} из {formatFileSize(viewer.size)}</span>
        {/if}
      </div>
    {:else if viewer.video && !viewer.error}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video src={viewer.src} controls autoplay></video>
    {:else if viewer.src}
      <img src={viewer.src} alt="" draggable="false" />
    {/if}
    {#if viewer.error}
      <p class="viewer-state error">{viewer.error}</p>
    {/if}
    <button class="viewer-backdrop" onclick={() => (viewer = null)} aria-label="Закрыть"></button>
  </div>
{/if}

<style>
  .chat {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  header.hidden {
    display: none;
  }

  .select-bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 52px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .sb-x {
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
    font-weight: 400;
  }

  .sb-count {
    flex: 1;
    font-weight: 600;
  }

  .danger-text {
    color: var(--danger);
  }

  .message.selected,
  .sticker.selected {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  header {
    flex: none;
    display: flex;
    align-items: center;
    height: 52px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .peer {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 10px 4px 6px;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
  }

  .peer:hover {
    background: var(--bg-hover);
  }

  .title {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .scroll-area {
    position: relative;
    flex: 1;
    min-height: 0;
    /* Static, painted once: soft gradients in the --bg family. */
    background: var(--chat-bg);
  }

  .messages {
    height: 100%;
    overflow-y: auto;
  }

  /* On a wide window the conversation stays a readable column in the
     middle, as in Telegram Desktop, instead of spreading to the edges. */
  .content {
    width: 100%;
    max-width: 820px;
    margin: 0 auto;
    min-height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    padding: 10px 16px 12px;
  }

  .note,
  .day {
    margin: 8px 0;
    text-align: center;
    color: var(--text-dim);
    font-size: 12px;
  }

  .day {
    position: sticky;
    top: 4px;
    z-index: 1;
  }

  /* Date separators and service lines share one pill. */
  .day span,
  .service span {
    display: inline-block;
    max-width: 80%;
    padding: 3px 12px;
    border-radius: 13px;
    background: var(--pill-bg);
    color: #e3eaf1;
    font-size: 13px;
    line-height: 1.35;
    font-weight: 500;
  }

  .message {
    position: relative;
    align-self: flex-start;
    max-width: min(74%, 520px);
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 8px;
    padding: 6px 11px 6px;
    border-radius: 18px;
    background: var(--bubble-in);
    /* Off-screen bubbles skip layout and paint; the size is a placeholder
       until the browser has measured the real one. */
    content-visibility: auto;
    contain-intrinsic-size: auto 44px;
  }

  .message.tail {
    border-bottom-left-radius: 6px;
  }

  .message.out {
    align-self: flex-end;
    background: var(--bubble-out);
  }

  .message.out.tail {
    border-bottom-left-radius: 18px;
    border-bottom-right-radius: 6px;
  }

  .message.joined {
    margin-top: 2px;
  }

  .message.media {
    padding: 4px 4px 6px;
  }

  .message.media > .sender,
  .message.media > .body,
  .message.media > .file {
    margin-inline: 7px;
  }

  .message.media-only {
    padding: 3px;
  }

  .sender {
    color: var(--accent);
    font-size: 13px;
    font-weight: 600;
  }

  /* Text and time share a line box: the time floats at the end of the last
     line when it fits, otherwise it drops below. */
  .body {
    display: flow-root;
    font-size: calc(15.5px * var(--scale, 1));
    line-height: 1.4;
  }

  .single {
    max-width: 100%;
    border-radius: 12px;
    overflow: hidden;
  }

  .single :global(.image) {
    border-radius: 12px;
  }

  /* A bubble with a picture is exactly as wide as the picture: the caption
     wraps under it instead of stretching the bubble past the image. */
  .message.media {
    width: min-content;
  }

  .message.media .body {
    overflow-wrap: anywhere;
  }

  .album {
    width: 420px;
    max-width: 100%;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 2px;
    border-radius: 12px;
    overflow: hidden;
  }

  .album.two {
    width: 380px;
    grid-template-columns: repeat(2, 1fr);
  }

  .cell {
    aspect-ratio: 1;
    min-width: 0;
  }

  .sticker {
    align-self: flex-start;
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 8px;
    max-width: 168px;
  }

  .sticker.out {
    align-self: flex-end;
  }

  .sticker :global(.image) {
    width: 168px !important;
    max-height: 168px;
    background: transparent;
    cursor: default;
  }

  .sticker :global(img) {
    object-fit: contain;
  }

  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 180px;
    padding: 2px 0;
  }

  .file-icon {
    flex: none;
    width: 42px;
    height: 42px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
  }

  .file-icon:disabled {
    opacity: 1;
  }

  audio {
    width: 260px;
    max-width: 100%;
    height: 40px;
  }

  .file-body {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-size {
    color: var(--text-dim);
    font-size: 12px;
  }

  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }

  .web {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    width: 100%;
    margin: 6px 0 2px;
    padding: 4px 0 4px 10px;
    border: none;
    border-left: 3px solid var(--link);
    border-radius: 0;
    background: transparent;
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }

  .web-site {
    color: var(--link);
    font-size: 13.5px;
    font-weight: 600;
  }

  .web-title {
    font-weight: 600;
  }

  .web-desc {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    font-size: 14px;
    white-space: pre-wrap;
  }

  .message.out {
    --link: #a9d4ff;
  }

  .foot {
    clear: both;
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 10px;
    margin-top: 5px;
  }

  .message.media:not(.media-only) .foot,
  .message.media-only .foot {
    padding: 0 6px 3px;
  }

  .foot .meta {
    margin-left: auto;
  }

  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .reaction {
    border: none;
    color: inherit;
    font-family: inherit;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 9px;
    border-radius: 13px;
    background: rgb(255 255 255 / 9%);
    font-size: 15px;
    line-height: 1;
    user-select: none;
  }

  .reaction:disabled {
    cursor: default;
    opacity: 1;
  }

  .reaction b {
    font-size: 13px;
    font-weight: 600;
  }

  .reaction.mine {
    background: var(--accent);
    color: #fff;
  }

  .message.out .reaction {
    background: rgb(255 255 255 / 14%);
  }

  .message.out .reaction.mine {
    background: #fff;
    color: var(--bubble-out, #2b5278);
  }

  .ticks {
    margin: -2px -2px -2px 3px;
    vertical-align: middle;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .ticks.read {
    color: #6ec6ff;
  }

  .message.media-only .ticks.read {
    color: #fff;
  }

  .names {
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    line-height: 1.25;
  }

  .status {
    color: var(--text-dim);
    font-size: 13px;
    font-weight: 400;
  }

  .status.online {
    color: var(--accent);
  }

  .meta {
    align-self: flex-end;
    color: var(--text-dim);
    font-size: 11px;
    line-height: 1;
    white-space: nowrap;
    user-select: none;
  }

  .body > .meta {
    float: right;
    margin: 7px 0 -3px 10px;
  }

  /* A picture without text: the time is a small pill over its corner. */
  .message.media-only > .meta {
    position: absolute;
    right: 10px;
    bottom: 10px;
    padding: 3px 7px;
    border-radius: 10px;
    background: rgb(0 0 0 / 45%);
    color: #fff;
  }

  .message.out .meta {
    color: rgb(255 255 255 / 60%);
  }

  .message.out.media-only > .meta {
    color: #fff;
  }

  .message.out .file-size {
    color: rgb(255 255 255 / 65%);
  }

  .to-bottom {
    position: absolute;
    right: 16px;
    bottom: 12px;
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: var(--bg-input);
    color: var(--text);
    box-shadow: 0 2px 8px rgb(0 0 0 / 40%);
  }

  .badge {
    position: absolute;
    top: -6px;
    right: -4px;
    min-width: 18px;
    padding: 1px 5px;
    border-radius: 9px;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 11px;
    font-weight: 600;
  }

  .error {
    flex: none;
    margin: 0;
    padding: 4px 16px;
  }

  .quote {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
    padding: 3px 8px;
    border: none;
    border-left: 3px solid var(--accent);
    border-radius: 4px 8px 8px 4px;
    background: rgb(255 255 255 / 7%);
    color: inherit;
    font-size: 13px;
    font-weight: 400;
    line-height: 1.3;
    text-align: left;
  }

  .message.out .quote {
    border-left-color: rgb(255 255 255 / 75%);
  }

  .q-name {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--accent);
    font-weight: 600;
  }

  .message.out .q-name {
    color: #fff;
  }

  .q-text {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
  }

  .message.out .q-text {
    color: rgb(255 255 255 / 75%);
  }

  .message.media > .quote {
    margin-inline: 7px;
    contain: inline-size;
  }

  :global(.message.flash) {
    animation: flash 1.3s ease-out;
  }

  @keyframes flash {
    0%,
    40% {
      box-shadow: 0 0 0 3px var(--accent);
    }
    100% {
      box-shadow: 0 0 0 3px transparent;
    }
  }

  .compose-bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px max(12px, calc((100% - 820px) / 2 + 16px)) 0;
    border-top: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .cb-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 2px 10px;
    border-left: 3px solid var(--accent);
    font-size: 13px;
    line-height: 1.35;
  }

  .cb-title {
    color: var(--accent);
    font-weight: 600;
  }

  .cb-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
  }

  .cb-x {
    flex: none;
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
    font-weight: 400;
  }

  .cb-x:hover {
    background: var(--bg-hover);
  }

  footer.has-bar {
    border-top: none;
  }

  .unread-divider {
    margin: 10px -16px 6px;
    padding: 4px 0;
    background: var(--pill-bg);
    color: var(--text-dim);
    font-size: 13px;
    text-align: center;
  }

  .search-btn {
    flex: none;
    width: 38px;
    height: 38px;
    margin-left: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
  }

  .search-btn:hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .chat-search {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .chat-search input {
    flex: 1;
    min-width: 0;
    height: 34px;
    border-radius: 17px;
  }

  .cs-count {
    min-width: 24px;
    color: var(--text-dim);
    font-size: 13px;
    text-align: right;
  }

  .cs-x {
    width: 30px;
    height: 30px;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
    font-weight: 400;
  }

  .cs-results {
    flex: none;
    max-height: 40%;
    overflow-y: auto;
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .cs-hit {
    width: 100%;
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 10px;
    padding: 8px 14px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }

  .cs-hit:hover {
    background: var(--bg-hover);
  }

  .cs-who {
    color: var(--accent);
    font-weight: 600;
  }

  .cs-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cs-date {
    color: var(--text-dim);
    font-size: 12.5px;
  }

  .pinned {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px 6px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .pin-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 2px 0 2px 10px;
    border: none;
    border-left: 2px solid var(--accent);
    border-radius: 0;
    background: transparent;
    color: var(--text);
    font-weight: 400;
    text-align: left;
  }

  .pin-title {
    color: var(--accent);
    font-size: 13px;
    font-weight: 600;
  }

  .pin-text {
    max-width: 100%;
    overflow: hidden;
    font-size: 13.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pin-x {
    width: 30px;
    height: 30px;
    flex: none;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
    font-weight: 400;
  }

  .pin-x:hover {
    background: var(--bg-hover);
  }

  .dropzone {
    position: absolute;
    inset: 8px;
    z-index: 20;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--accent);
    border-radius: 14px;
    background: rgb(0 0 0 / 55%);
    pointer-events: none;
  }

  .dropzone span {
    padding: 10px 18px;
    border-radius: 20px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }

  .upload {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px max(12px, calc((100% - 820px) / 2 + 16px));
    background: var(--bg-raised);
    border-top: 1px solid var(--border);
    font-size: 13px;
  }

  .up-name {
    min-width: 0;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .up-bar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--bg-input);
    overflow: hidden;
  }

  .up-bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }

  .up-pct {
    width: 38px;
    color: var(--text-dim);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .attach {
    flex: none;
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-dim);
  }

  .attach:hover:not(:disabled) {
    color: var(--accent);
    background: var(--bg-hover);
  }

  .file-input {
    display: none;
  }

  footer {
    flex: none;
    display: flex;
    align-items: flex-end;
    gap: 8px;
    /* The input lines up with the message column above it. */
    padding: 8px max(12px, calc((100% - 820px) / 2 + 16px));
    border-top: 1px solid var(--border);
    background: var(--bg-raised);
  }

  textarea {
    flex: 1;
    min-height: 40px;
    max-height: 140px;
    padding: 10px 14px;
    resize: none;
    field-sizing: content;
    background: var(--bg-input);
    border: 1px solid transparent;
    border-radius: 20px;
    color: var(--text);
    font: inherit;
    outline: none;
  }

  textarea:focus {
    border-color: var(--accent);
  }

  .send {
    flex: none;
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 0 0 2px;
    border-radius: 50%;
  }

  .viewer {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 28px;
  }

  .viewer-backdrop {
    position: absolute;
    inset: 0;
    z-index: -1;
    border: none;
    border-radius: 0;
    background: rgb(0 0 0 / 88%);
    cursor: zoom-out;
  }

  .viewer img,
  .viewer video {
    max-width: 100%;
    min-height: 0;
    max-height: 100%;
    border-radius: 6px;
  }

  .viewer-state {
    flex: none;
    margin: 0;
    padding: 4px 12px;
    border-radius: 12px;
    background: rgb(0 0 0 / 60%);
    color: #fff;
    font-size: 13px;
  }

  .viewer-loading {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    color: rgb(255 255 255 / 75%);
    font-size: 13px;
    pointer-events: none;
  }

  .viewer-save {
    position: absolute;
    top: 14px;
    right: 64px;
    z-index: 2;
    padding: 7px 14px;
    border-radius: 18px;
    background: rgb(255 255 255 / 12%);
    color: #fff;
    font-weight: 500;
  }

  .viewer-save:hover {
    background: rgb(255 255 255 / 20%);
  }

  .spin {
    transform-origin: center;
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .small {
    font-size: inherit;
  }

  .viewer-close {
    position: absolute;
    top: 12px;
    right: 12px;
    width: 38px;
    height: 38px;
    padding: 0;
    border-radius: 50%;
    background: rgb(255 255 255 / 12%);
    color: #fff;
    font-weight: 400;
  }
</style>
