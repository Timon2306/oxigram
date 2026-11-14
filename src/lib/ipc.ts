import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";

export type Me = {
  id: number;
  name: string;
  username: string | null;
  phone: string | null;
  /** A string: the ID does not fit a JavaScript number. */
  photoId: string | null;
};

export type Contact = {
  /** Also the ID of the private chat with them. */
  id: number;
  title: string;
  photoId: string | null;
  presence: Presence | null;
};

/** Mirrors `UiState` in src-tauri/src/tg.rs. */
export type UiState =
  | { stage: "needKeys" }
  | { stage: "needPhone" }
  | { stage: "needCode"; phone: string }
  | { stage: "needPassword"; hint: string | null }
  | { stage: "ready"; me: Me };

/** Mirrors `UiError` in src-tauri/src/error.rs. */
export type UiError = {
  code: string;
  message: string;
  waitSeconds: number | null;
};

/** Mirrors `DialogRow` in src-tauri/src/store/chats.rs. */
export type Dialog = {
  id: number;
  kind: "user" | "saved" | "group" | "channel";
  title: string;
  lastText: string;
  lastDate: number;
  lastOutgoing: boolean;
  /** ID of the last message; with `readOutbox` it tells if an own one was read. */
  lastId: number;
  unread: number;
  pinned: boolean;
  archived: boolean;
  /** A string: the ID does not fit a JavaScript number. */
  photoId: string | null;
  /** Own messages up to this ID were read by the other side. */
  readOutbox: number;
  /** Reactions to own messages not seen yet. */
  unreadReactions: number;
  /** Notifications are off for this chat. */
  muted: boolean;
  /** A private chat with someone in the contacts. */
  contact: boolean;
  /** A private chat with a bot. */
  bot: boolean;
  /** Private chats only; null when unknown. */
  presence: Presence | null;
};

export type Entity = {
  kind: string;
  offset: number;
  length: number;
  /** Target of a "textUrl", or the user ID of a "mentionName". */
  target?: string;
};

/** Typing markers for the formatting the input understands. */
const MARKS: Record<string, string> = {
  bold: "**",
  italic: "__",
  strike: "~~",
  spoiler: "||",
  code: "`",
  pre: "```",
};

/**
 * Puts the typing markers back around formatted spans, so editing a
 * message keeps its formatting (the backend parses them again on send).
 */
export function toMarkup(text: string, entities: Entity[]): string {
  const inserts: { at: number; mark: string; open: boolean }[] = [];
  for (const entity of entities) {
    const mark = MARKS[entity.kind];
    if (!mark) continue;
    inserts.push({ at: entity.offset, mark, open: true });
    inserts.push({ at: entity.offset + entity.length, mark, open: false });
  }
  // From the end, so earlier offsets stay valid. At one spot an opener is
  // inserted first, so the closer typed before it lands on its left.
  inserts.sort((a, b) => b.at - a.at || Number(b.open) - Number(a.open));
  let out = text;
  for (const insert of inserts) out = out.slice(0, insert.at) + insert.mark + out.slice(insert.at);
  return out;
}

export type WebPreview = { url: string; site: string; title: string; description: string };

/** A chat folder and the rules that fill it. */
export type Folder = {
  id: number;
  title: string;
  emoticon: string | null;
  contacts: boolean;
  nonContacts: boolean;
  groups: boolean;
  channels: boolean;
  bots: boolean;
  excludeMuted: boolean;
  excludeRead: boolean;
  excludeArchived: boolean;
  include: number[];
  exclude: number[];
};

/** Whether a chat belongs in a folder, by Telegram's rules. */
export function inFolder(dialog: Dialog, folder: Folder): boolean {
  if (folder.exclude.includes(dialog.id)) return false;
  if (folder.include.includes(dialog.id)) return true;
  if (folder.excludeArchived && dialog.archived) return false;
  if (folder.excludeMuted && dialog.muted) return false;
  if (folder.excludeRead && dialog.unread === 0) return false;
  switch (dialog.kind) {
    case "user":
      return dialog.bot ? folder.bots : dialog.contact ? folder.contacts : folder.nonContacts;
    case "group":
      return folder.groups;
    case "channel":
      return folder.channels;
    default:
      return false;
  }
}

/** A message found by the global search, with its chat. */
export type FoundMessage = { message: Message; peer: PeerSummary };

/** A chat found by @username. */
export type FoundChat = { id: number; kind: Dialog["kind"]; title: string; photoId: string | null };

export type Reaction = {
  /** "" for a custom (premium) emoji the client does not draw. */
  emoji: string;
  count: number;
  /** The user's own reaction. */
  mine: boolean;
};

export type Presence = {
  kind: "online" | "offline" | "recently" | "week" | "month" | "hidden";
  /** Unix seconds: until when "online" holds, or when "offline" began. */
  at: number;
};

/** Online right now (an "online" status expires on its own). */
export function isOnline(presence: Presence | null | undefined, now: number): boolean {
  return presence?.kind === "online" && presence.at > now;
}

/** "в сети", "был(а) в 14:05", "был(а) недавно"… as Telegram words it. */
export function presenceText(presence: Presence | null | undefined, now: number): string {
  if (!presence) return "";
  if (isOnline(presence, now)) return "в сети";
  switch (presence.kind) {
    case "recently":
      return "был(а) недавно";
    case "week":
      return "был(а) на этой неделе";
    case "month":
      return "был(а) в этом месяце";
    case "hidden":
      return "был(а) давно";
  }
  // "offline", or an "online" that expired: `at` is the last moment seen.
  const ago = now - presence.at;
  if (ago < 60) return "был(а) только что";
  if (ago < 3600) {
    const minutes = Math.floor(ago / 60);
    return `был(а) ${minutes} ${plural(minutes, "минуту", "минуты", "минут")} назад`;
  }
  const seen = new Date(presence.at * 1000);
  const today = new Date(now * 1000);
  const time = seen.toLocaleTimeString("ru", { hour: "2-digit", minute: "2-digit" });
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (seen.toDateString() === today.toDateString()) return `был(а) в ${time}`;
  if (seen.toDateString() === yesterday.toDateString()) return `был(а) вчера в ${time}`;
  return `был(а) ${seen.toLocaleDateString("ru", { day: "2-digit", month: "2-digit", year: "2-digit" })}`;
}

function plural(n: number, one: string, few: string, many: string): string {
  const tens = n % 100;
  const ones = n % 10;
  if (tens >= 11 && tens <= 14) return many;
  if (ones === 1) return one;
  if (ones >= 2 && ones <= 4) return few;
  return many;
}

export function dialogTitle(dialog: Dialog): string {
  if (dialog.kind === "saved") return "Избранное";
  return dialog.title || "Без названия";
}

/** Mirrors `MessageRow` in src-tauri/src/store/chats.rs. */
export type Message = {
  id: number;
  peerId: number;
  senderId: number | null;
  senderName: string;
  date: number;
  text: string;
  kind: string;
  outgoing: boolean;
  edited: boolean;
  /** Size of the picture `messageImage` can fetch; 0 when there is none. */
  imageW: number;
  imageH: number;
  fileName: string;
  fileSize: number;
  /** The full file can be fetched with `messageFileUrl` and played or shown. */
  playable: boolean;
  /** Messages sharing this value form one album. */
  groupedId: string | null;
  reactions: Reaction[];
  /** Seconds of audio or video; 0 when not known. */
  duration: number;
  entities: Entity[];
  web: WebPreview | null;
  /** The message this one replies to. */
  replyToId: number | null;
};

export type PeerSummary = { kind: Dialog["kind"]; title: string; photoId: string | null };

/** Mirrors `PeerInfo` in src-tauri/src/chats.rs. */
export type PeerInfo = {
  id: number;
  kind: Dialog["kind"];
  title: string;
  photoId: string | null;
  username: string | null;
  phone: string | null;
  about: string;
  members: number | null;
  /** Members online (channels and supergroups). */
  online: number | null;
  /** "10.11" or "10.11.1990". */
  birthday: string | null;
  commonChats: number | null;
  inviteLink: string | null;
  muted: boolean;
  /** Shared content by kind; only non-zero counts. */
  media: { kind: "photo" | "video" | "file" | "audio" | "link" | "voice" | "gif"; count: number }[];
};

export type SharedKind = "media" | "file" | "link" | "voice" | "audio";

/** Mirrors `UiEvent` in src-tauri/src/updates.rs. */
export type UiEvent =
  | { type: "message"; message: Message; peer: PeerSummary }
  | { type: "edited"; message: Message }
  | { type: "deleted"; peerId: number | null; ids: number[] }
  | { type: "connection"; online: boolean }
  | { type: "readOutbox"; peerId: number; maxId: number }
  | { type: "presence"; peerId: number; presence: Presence }
  | { type: "reactions"; peerId: number; id: number; reactions: Reaction[] }
  | { type: "upload"; token: string; bytes: number }
  | { type: "typing"; peerId: number; userId: number; action: TypingAction }
  | { type: "pinned"; peerId: number };

export type TypingAction = "typing" | "cancel" | "voice" | "round" | "video" | "photo" | "file" | "sticker";

/** Who is typing in a chat right now, by chat ID. */
export type TypingState = Record<number, { userId: number; action: TypingAction }>;

/** "печатает…", "записывает голосовое…" and so on. */
export function typingText(action: TypingAction): string {
  switch (action) {
    case "voice":
      return "записывает голосовое…";
    case "round":
      return "записывает видеосообщение…";
    case "video":
      return "отправляет видео…";
    case "photo":
      return "отправляет фото…";
    case "file":
      return "отправляет файл…";
    case "sticker":
      return "выбирает стикер…";
    default:
      return "печатает…";
  }
}

export type DialogPage = { dialogs: Dialog[]; done: boolean };

export const api = {
  getState: () => invoke<UiState>("get_state"),
  saveKeys: (apiId: number, apiHash: string) =>
    invoke<UiState>("save_keys", { apiId, apiHash }),
  clearKeys: () => invoke<UiState>("clear_keys"),
  requestCode: (phone: string) => invoke<UiState>("request_code", { phone }),
  submitCode: (code: string) => invoke<UiState>("submit_code", { code }),
  submitPassword: (password: string) =>
    invoke<UiState>("submit_password", { password }),
  backToPhone: () => invoke<UiState>("back_to_phone"),
  signOut: () => invoke<UiState>("sign_out"),
  cachedDialogs: () => invoke<Dialog[]>("cached_dialogs"),
  loadDialogs: (reset: boolean) => invoke<DialogPage>("load_dialogs", { reset }),
  /** Both history calls return messages newest first. */
  cachedHistory: (peerId: number, beforeId: number | null, limit: number) =>
    invoke<Message[]>("cached_history", { peerId, beforeId, limit }),
  history: (peerId: number, beforeId: number | null, limit: number) =>
    invoke<Message[]>("history", { peerId, beforeId, limit }),
  sendMessage: (peerId: number, text: string, replyTo: number | null = null) =>
    invoke<Message>("send_message", { peerId, text, replyTo }),
  pinDialog: (peerId: number, pinned: boolean) => invoke<void>("pin_dialog", { peerId, pinned }),
  markUnread: (peerId: number, unread: boolean) => invoke<void>("mark_unread", { peerId, unread }),
  archiveDialog: (peerId: number, archived: boolean) =>
    invoke<void>("archive_dialog", { peerId, archived }),
  clearHistory: (peerId: number) => invoke<void>("clear_history", { peerId }),
  deleteChat: (peerId: number) => invoke<void>("delete_chat", { peerId }),
  /** `emoji: ""` removes the own reaction. */
  react: (peerId: number, messageId: number, emoji: string) =>
    invoke<void>("react", { peerId, messageId, emoji }),
  editMessage: (peerId: number, messageId: number, text: string) =>
    invoke<void>("edit_message", { peerId, messageId, text }),
  deleteMessages: (peerId: number, ids: number[], forEveryone: boolean) =>
    invoke<void>("delete_messages", { peerId, ids, forEveryone }),
  pinMessage: (peerId: number, messageId: number, pinned: boolean) =>
    invoke<void>("pin_message", { peerId, messageId, pinned }),
  forwardMessages: (fromPeerId: number, ids: number[], toPeerId: number) =>
    invoke<Message[]>("forward_messages", { fromPeerId, ids, toPeerId }),
  markRead: (peerId: number) => invoke<void>("mark_read", { peerId }),
  peerInfo: (peerId: number) => invoke<PeerInfo>("peer_info", { peerId }),
  /** Shared media of one kind, newest first; pages older than `beforeId`. */
  sharedMedia: (peerId: number, kind: SharedKind | "pinned", beforeId: number | null, limit = 60) =>
    invoke<Message[]>("shared_media", { peerId, kind, beforeId, limit }),
  setMuted: (peerId: number, muted: boolean) => invoke<void>("set_muted", { peerId, muted }),
  contacts: () => invoke<Contact[]>("contacts"),
  cacheSize: () => invoke<number>("cache_size"),
  clearCache: () => invoke<number>("clear_cache"),
  autostartEnabled: () => invoke<boolean>("plugin:autostart|is_enabled"),
  setAutostart: (on: boolean) => invoke<void>(on ? "plugin:autostart|enable" : "plugin:autostart|disable"),
  folders: (selfId: number) => invoke<Folder[]>("folders", { selfId }),
  searchChat: (peerId: number, query: string) => invoke<Message[]>("search_chat", { peerId, query }),
  searchGlobal: (query: string) => invoke<FoundMessage[]>("search_global", { query }),
  resolveUsername: (username: string) => invoke<FoundChat>("resolve_username", { username }),
  setTyping: (peerId: number) => invoke<void>("set_typing", { peerId }),
  setActiveChat: (peerId: number | null) => invoke<void>("set_active_chat", { peerId }),
  setUnread: (unread: number) => invoke<void>("set_unread", { unread }),
  /**
   * Sends a file. The bytes go as the raw request body (no base64 copy);
   * progress arrives as "upload" events carrying `token`.
   */
  sendFile: async (
    peerId: number,
    file: File,
    options: { caption: string; asPhoto: boolean; replyTo: number | null; token: string },
  ) =>
    invoke<Message>("send_file", new Uint8Array(await file.arrayBuffer()), {
      headers: {
        "x-peer": String(peerId),
        "x-name": encodeURIComponent(file.name || "file"),
        "x-caption": encodeURIComponent(options.caption),
        "x-as-photo": options.asPhoto ? "1" : "0",
        "x-reply-to": options.replyTo === null ? "" : String(options.replyTo),
        "x-token": options.token,
      },
    }),
  peerPresence: (peerId: number) => invoke<Presence | null>("peer_presence", { peerId }),
  /** Saves the attachment to Downloads; resolves to the saved file's path. */
  saveFile: (peerId: number, messageId: number) =>
    invoke<string>("save_file", { peerId, messageId }),
  /** Starts the live event stream; events arrive in batches. */
  subscribe: (handler: (events: UiEvent[]) => void) => {
    const onEvent = new Channel<UiEvent[]>();
    onEvent.onmessage = handler;
    return invoke<void>("subscribe", { onEvent });
  },
};

const MESSAGES: Record<string, string> = {
  KEYS_INVALID: "Проверьте ключи: api_id — число, api_hash — 32 символа (0–9, a–f).",
  API_ID_INVALID: "Telegram не принял эти api_id / api_hash. Проверьте их и введите заново.",
  API_ID_PUBLISHED_FLOOD: "Этот api_id ограничен Telegram. Нужен другой ключ.",
  PHONE_NUMBER_INVALID: "Неверный номер телефона. Введите его в международном формате.",
  PHONE_NUMBER_BANNED: "Этот номер заблокирован в Telegram.",
  PHONE_NUMBER_FLOOD: "Слишком много попыток входа с этого номера. Попробуйте позже.",
  PHONE_CODE_INVALID: "Неверный код.",
  PHONE_CODE_EXPIRED: "Код устарел. Запросите новый.",
  PHONE_CODE_EMPTY: "Введите код.",
  PASSWORD_INVALID: "Неверный пароль.",
  SIGN_UP_REQUIRED:
    "Аккаунта с этим номером нет. Зарегистрироваться можно только в официальном приложении Telegram.",
  SESSION_PASSWORD_NEEDED: "Нужен облачный пароль.",
  NETWORK: "Нет связи с Telegram. Проверьте интернет и попробуйте ещё раз.",
  NOT_AUTHORIZED: "Вход в аккаунт не выполнен.",
  AUTH_KEY_UNREGISTERED: "Сессия завершена на сервере. Войдите заново.",
  PEER_UNKNOWN: "Этот чат пока недоступен. Обновите список диалогов.",
  MEDIA_EMPTY: "Файл недоступен.",
  USERNAME_NOT_OCCUPIED: "Такого имени пользователя нет.",
  USERNAME_INVALID: "Неверное имя пользователя.",
  FILE_TOO_BIG: "Файл больше 512 МБ — такой пока не отправить.",
  FILE_EMPTY: "Файл пустой.",
  CAPTION_TOO_LONG: "Подпись длиннее 1024 символов.",
  UPLOAD_FAILED: "Не удалось загрузить файл.",
  MEDIA_EPHEMERAL: "Исчезающие фото и видео сохранить нельзя.",
  NO_DOWNLOADS_DIR: "Не найдена папка «Загрузки».",
  MESSAGE_EMPTY: "Сообщение пустое.",
  MESSAGE_TOO_LONG: "Сообщение длиннее 4096 символов.",
  CHAT_WRITE_FORBIDDEN: "В этот чат нельзя писать.",
  USER_IS_BLOCKED: "Пользователь вас заблокировал.",
  WRONG_STAGE: "Этот шаг входа уже неактуален. Начните вход заново.",
};

function isUiError(value: unknown): value is UiError {
  return typeof value === "object" && value !== null && "code" in value && "message" in value;
}

/** Turns anything thrown by `invoke` into a message for the user. */
export function describeError(err: unknown): string {
  if (!isUiError(err)) return String(err);
  if (err.waitSeconds != null) {
    return `Telegram просит подождать ${formatWait(err.waitSeconds)} перед следующей попыткой.`;
  }
  return MESSAGES[err.code] ?? `${err.message} (${err.code})`;
}

export function errorCode(err: unknown): string | null {
  return isUiError(err) ? err.code : null;
}

function formatWait(seconds: number): string {
  if (seconds < 60) return `${seconds} с`;
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 60) return `${minutes} мин`;
  return `${Math.ceil(minutes / 60)} ч`;
}


const KIND_LABELS: Record<string, string> = {
  photo: "Фото",
  sticker: "Стикер",
  document: "Файл",
  video: "Видео",
  gif: "GIF",
  round: "Видеосообщение",
  voice: "Голосовое сообщение",
  audio: "Аудио",
  contact: "Контакт",
  poll: "Опрос",
  location: "Геопозиция",
  dice: "Кубик",
  service: "Служебное сообщение",
  media: "Вложение",
};

/** Stand-in text for content that is not rendered yet; `null` for plain text. */
export function kindLabel(kind: string): string | null {
  return kind === "text" ? null : (KIND_LABELS[kind] ?? KIND_LABELS.media);
}

/** Dialog previews arrive as "[kind]" when the message has no text. */
export function previewText(text: string): string {
  const match = /^\[(\w+)\]$/.exec(text);
  return match ? (kindLabel(match[1]) ?? text) : text;
}

const timeFormat = new Intl.DateTimeFormat("ru", { hour: "2-digit", minute: "2-digit" });
const dayFormat = new Intl.DateTimeFormat("ru", { day: "numeric", month: "long", year: "numeric" });
const shortDayFormat = new Intl.DateTimeFormat("ru", { day: "2-digit", month: "2-digit", year: "2-digit" });

export function formatTime(unixSeconds: number): string {
  return timeFormat.format(unixSeconds * 1000);
}

export function formatDay(unixSeconds: number): string {
  return dayFormat.format(unixSeconds * 1000);
}

export function dayKey(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toDateString();
}

/** Time for today, a short date otherwise: the dialog list convention. */
export function formatListDate(unixSeconds: number): string {
  if (!unixSeconds) return "";
  return dayKey(unixSeconds) === new Date().toDateString()
    ? formatTime(unixSeconds)
    : shortDayFormat.format(unixSeconds * 1000);
}

/** Channels and supergroups number their messages per chat; elsewhere IDs are account-wide. */
export function hasOwnMessageIds(peerId: number): boolean {
  return peerId <= -1_000_000_000_000;
}

// Picture paths are resolved once per session; the files live on disk and the
// map only holds short URL strings.
const imageUrls = new Map<string, Promise<string>>();

/** Forgets fetched picture URLs, after the cache on disk was cleared. */
export function forgetMediaUrls() {
  imageUrls.clear();
}

function cachedUrl(key: string, load: () => Promise<string>): Promise<string> {
  let url = imageUrls.get(key);
  if (!url) {
    url = load().then(convertFileSrc);
    // A failed download may succeed later (e.g. after reconnecting).
    url.catch(() => imageUrls.delete(key));
    imageUrls.set(key, url);
  }
  return url;
}

export function avatarUrl(peerId: number, photoId: string): Promise<string> {
  return cachedUrl(`a:${peerId}:${photoId}`, () =>
    invoke<string>("avatar", { peerId, photoId }),
  );
}

export function messageImageUrl(peerId: number, messageId: number): Promise<string> {
  return cachedUrl(`m:${peerId}:${messageId}`, () =>
    invoke<string>("message_image", { peerId, messageId }),
  );
}

/** Who wants to hear about a running full-file download, by cache key. */
const fileProgress = new Map<string, Set<(bytes: number) => void>>();

/**
 * The full video, audio or original photo; downloaded on first request.
 * `onprogress` gets the bytes received so far while it downloads.
 */
export async function messageFileUrl(
  peerId: number,
  messageId: number,
  onprogress?: (bytes: number) => void,
): Promise<string> {
  const key = `f:${peerId}:${messageId}`;
  let listeners = fileProgress.get(key);
  if (!listeners) {
    listeners = new Set();
    fileProgress.set(key, listeners);
  }
  if (onprogress) listeners.add(onprogress);
  try {
    return await cachedUrl(key, () => {
      const onProgress = new Channel<number>();
      onProgress.onmessage = (bytes) => fileProgress.get(key)?.forEach((listener) => listener(bytes));
      return invoke<string>("message_file", { peerId, messageId, onProgress });
    });
  } finally {
    if (onprogress) listeners.delete(onprogress);
  }
}

/** "0:42", "12:05", "1:02:33". */
export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} Б`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} КБ`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} ГБ`;
}
