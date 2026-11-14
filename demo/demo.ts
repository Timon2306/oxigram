// Demo mode: answers every backend command with made-up data, so the real
// interface can be shown without an account. Nothing here talks to Telegram.
import { mockIPC } from "@tauri-apps/api/mocks";

const now = Math.floor(Date.now() / 1000);
const min = 60;
const hour = 3600;
const day = 86400;

/** A soft picture as an SVG data URL: two-colour gradient with shapes. */
function picture(a: string, b: string, w = 640, h = 420, extra = ""): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">
    <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs>
    <rect width="${w}" height="${h}" fill="url(#g)"/>
    <circle cx="${w * 0.78}" cy="${h * 0.28}" r="${h * 0.12}" fill="#fff" fill-opacity=".55"/>
    <path d="M0 ${h * 0.78} L${w * 0.28} ${h * 0.5} L${w * 0.5} ${h * 0.7} L${w * 0.7} ${h * 0.45} L${w} ${h * 0.75} V${h} H0Z" fill="#000" fill-opacity=".22"/>
    <path d="M0 ${h * 0.9} L${w * 0.35} ${h * 0.68} L${w * 0.62} ${h * 0.86} L${w} ${h * 0.7} V${h} H0Z" fill="#000" fill-opacity=".25"/>
    ${extra}
  </svg>`;
  return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
}

/** A face-less avatar: gradient with a simple silhouette. */
function avatar(a: string, b: string): string {
  return picture(
    a,
    b,
    160,
    160,
    `<circle cx="80" cy="64" r="28" fill="#fff" fill-opacity=".85"/><path d="M28 160c4-34 26-52 52-52s48 18 52 52z" fill="#fff" fill-opacity=".85"/>`,
  );
}

const ME = { id: 1000, name: "Демо", username: "oxigram_demo", phone: null, photoId: "p1000" };
const SAVED = 1000;
const ANNA = 1001;

type Row = Record<string, unknown>;

const dialogs: Row[] = [
  dialog(ANNA, "user", "Анна Морозова", "Скинь потом билд, хочу потыкать 🙌", now - 4 * min, false, 3, { pinned: true, photo: "p1001", unread: 0, readOutbox: 9999, presence: { kind: "online", at: now + 300 } }),
  dialog(1002, "group", "Команда Oxigram", "Максим: память в трее 5,7 МБ, проверил", now - 18 * min, false, 0, { unread: 4, photo: "p1002" }),
  dialog(1003, "user", "Максим Котов", "Ок, завтра созвонимся", now - 52 * min, true, 0, { photo: "p1003", readOutbox: 99, presence: { kind: "offline", at: now - 20 * min } }),
  dialog(-1001004, "channel", "Rust по-русски", "Вышел Rust 1.95: что нового в async", now - 2 * hour, false, 0, { unread: 12, photo: "p1004", muted: true }),
  dialog(1005, "user", "Лиза", "Фото с выходных 📷", now - 5 * hour, false, 0, { unreadReactions: 1, photo: "p1005", presence: { kind: "recently", at: 0 } }),
  dialog(1006, "group", "Дизайн-ревью", "Иконку обновил, глянь", now - 9 * hour, true, 0, { readOutbox: 99, photo: "p1006" }),
  dialog(1007, "user", "Игорь", "Спасибо!", now - day, false, 0, { presence: { kind: "week", at: 0 } }),
  dialog(SAVED, "saved", "", "Список идей для релиза", now - 2 * day, true, 0, {}),
  dialog(1008, "channel", "Tauri Updates", "Tauri 2.12: tray improvements", now - 3 * day, false, 0, { photo: "p1008" }),
  dialog(1009, "user", "Ольга Белова", "Договорились 👍", now - 4 * day, false, 0, { photo: "p1009" }),
];

function dialog(
  id: number,
  kind: string,
  title: string,
  lastText: string,
  lastDate: number,
  lastOutgoing: boolean,
  _x: number,
  o: { muted?: boolean; pinned?: boolean; photo?: string; unread?: number; unreadReactions?: number; readOutbox?: number; presence?: Row },
): Row {
  return {
    id,
    kind,
    title,
    photoId: o.photo ?? null,
    lastText,
    lastDate,
    lastOutgoing,
    lastId: 100,
    unread: o.unread ?? 0,
    pinned: o.pinned ?? false,
    archived: false,
    readOutbox: o.readOutbox ?? 0,
    unreadReactions: o.unreadReactions ?? 0,
    muted: o.muted ?? false,
    contact: kind === "user",
    bot: false,
    presence: o.presence ?? null,
  };
}

let nextId = 1;
function message(peerId: number, out: boolean, text: string, date: number, extra: Row = {}): Row {
  return {
    id: nextId++,
    peerId,
    senderId: out ? ME.id : peerId,
    senderName: out ? ME.name : "Анна Морозова",
    date,
    text,
    kind: "text",
    outgoing: out,
    edited: false,
    imageW: 0,
    imageH: 0,
    fileName: "",
    fileSize: 0,
    playable: false,
    duration: 0,
    groupedId: null,
    reactions: [],
    replyToId: null,
    entities: [],
    web: null,
    ...extra,
  };
}

const t0 = now - 50 * min;
const annaChat: Row[] = [
  message(ANNA, false, "Привет! Слышала, ты свой клиент Telegram пишешь?", t0 - day),
  message(ANNA, true, "Да, Oxigram. Rust + Tauri, без TDLib", t0 - day + 2 * min),
  message(ANNA, true, "Главная цель — чтобы почти не ел память", t0 - day + 2 * min + 20),
  message(ANNA, false, "И сколько выходит?", t0 - day + 5 * min, { reactions: [{ emoji: "🤔", count: 1, mine: false }] }),
  message(ANNA, true, "Около 70 МБ с открытым окном, а в трее 5–6 МБ", t0 + 1 * min, { reactions: [{ emoji: "🔥", count: 1, mine: false }] }),
  message(ANNA, false, "Ого, это меньше, чем одна вкладка браузера 😄", t0 + 3 * min),
  message(ANNA, true, "", t0 + 6 * min, {
    kind: "photo",
    imageW: 640,
    imageH: 400,
    playable: true,
  }),
  message(ANNA, true, "Вот так сейчас выглядит, тёмная тема и узор свой", t0 + 6 * min + 10),
  message(ANNA, false, "Красиво! А реакции и ответы есть?", t0 + 20 * min, { reactions: [{ emoji: "❤️", count: 1, mine: true }] }),
];
const replyTarget = annaChat[8].id as number;
annaChat.push(
  message(ANNA, true, "Есть: реакции, ответы, пересылка, правка, галочки прочтения", t0 + 22 * min, { replyToId: replyTarget }),
  message(ANNA, true, "Код тут: github.com/Timon2306/oxigram — пиши @anna_design. Секрет: работает без TDLib, а сборка — cargo tauri build", t0 + 30 * min, {
    entities: [{"kind":"bold","offset":0,"length":7},{"kind":"url","offset":9,"length":28},{"kind":"mention","offset":45,"length":12},{"kind":"spoiler","offset":67,"length":18},{"kind":"code","offset":98,"length":17}],
    web: { url: "https://github.com/Timon2306/oxigram", site: "GitHub", title: "Oxigram", description: "Лёгкий клиент Telegram на Rust и Tauri: около 70 МБ ОЗУ, 7 МБ exe." },
  }),
  message(ANNA, false, "", t0 + 40 * min, { kind: "video", imageW: 640, imageH: 360, playable: true, duration: 42, fileSize: 5200000 }),
  message(ANNA, false, "", t0 + 41 * min, { kind: "voice", playable: true, duration: 17, fileSize: 41000 }),
  message(ANNA, false, "Скинь потом билд, хочу потыкать 🙌", t0 + 46 * min),
);
const photoMessageId = annaChat[6].id as number;

const groupChat: Row[] = [
  { ...message(1002, false, "Собрал релиз, exe 6,4 МБ", now - 40 * min), senderId: 1003, senderName: "Максим Котов" },
  { ...message(1002, false, "Память в трее 5,7 МБ, проверил", now - 18 * min), senderId: 1003, senderName: "Максим Котов" },
];

const images: Record<string, string> = {
  p1000: avatar("#5a4ff0", "#2fd2c6"),
  p1001: avatar("#f78fb3", "#c56cf0"),
  p1002: avatar("#2a8cf0", "#2fd2c6"),
  p1003: avatar("#f7b731", "#fa8231"),
  p1004: avatar("#e15f41", "#c44569"),
  p1005: avatar("#26de81", "#20bf6b"),
  p1006: avatar("#a55eea", "#4b7bec"),
  p1008: avatar("#45aaf2", "#2d98da"),
  p1009: avatar("#fd9644", "#fc5c65"),
};
const photo = picture("#4b7bec", "#26de81");

const info: Row = {
  id: ANNA,
  kind: "user",
  title: "Анна Морозова",
  photoId: "p1001",
  username: "anna_design",
  phone: "79001234567",
  about: "Дизайнер интерфейсов. Люблю тёмные темы и быстрые приложения.",
  members: null,
  online: null,
  birthday: "14.03",
  commonChats: 2,
  inviteLink: null,
  muted: false,
  media: [
    { kind: "photo", count: 24 },
    { kind: "file", count: 6 },
    { kind: "link", count: 11 },
  ],
};

mockIPC((cmd, args) => {
  const a = (args ?? {}) as Record<string, unknown>;
  switch (cmd) {
    case "get_state":
      return { stage: "ready", me: ME };
    case "plugin:app|version":
      return "0.1.0";
    case "cached_dialogs":
      return dialogs;
    case "load_dialogs":
      return { dialogs, done: true };
    case "cached_history":
    case "history":
      if (a.beforeId) return [];
      if (a.peerId === ANNA) return [...annaChat].reverse();
      if (a.peerId === 1002) return [...groupChat].reverse();
      return [];
    case "peer_presence":
      return (dialogs.find((d) => d.id === a.peerId)?.presence as Row) ?? null;
    case "peer_info":
      return a.peerId === ANNA ? info : { ...info, id: a.peerId, title: "Демо", media: [] };
    case "shared_media":
      if (a.kind === "pinned") return a.peerId === ANNA ? [annaChat[4]] : [];
      return a.kind === "media" && !a.beforeId
        ? Array.from({ length: 9 }, (_, i) => ({
            ...message(ANNA, i % 2 === 0, "", now - i * day, { kind: "photo", imageW: 400, imageH: 400 }),
            id: 5000 + i,
          }))
        : [];
    case "avatar":
      return images[String(a.photoId)] ?? "";
    case "message_image":
      return a.messageId === photoMessageId || Number(a.messageId) >= 5000
        ? picture(
            ["#4b7bec", "#f78fb3", "#26de81", "#fa8231", "#a55eea"][Number(a.messageId) % 5],
            ["#26de81", "#c56cf0", "#45aaf2", "#f7b731", "#2fd2c6"][Number(a.messageId) % 5],
          )
        : photo;
    case "message_file":
      return photo;
    case "contacts":
      return dialogs
        .filter((d) => d.kind === "user")
        .map((d) => ({ id: d.id, title: d.title, photoId: d.photoId, presence: d.presence }));
    case "send_file":
      return message(ANNA, true, "", Math.floor(Date.now() / 1000), { kind: "photo", imageW: 640, imageH: 400, playable: true });
    case "subscribe": {
      // Keep the event channel: demoEmit() then plays events as if Telegram sent them.
      const channel = a.onEvent as { onmessage: (events: unknown[]) => void };
      (window as unknown as { demoEmit: (events: unknown[]) => void }).demoEmit = (events) => channel.onmessage(events);
      return null;
    }
    case "cache_size":
    case "clear_cache":
      return cmd === "cache_size" ? 48_500_000 : 0;
    case "plugin:autostart|is_enabled":
      return false;
    case "folders":
      return [
        { id: 1, title: "Личные", emoticon: null, contacts: true, nonContacts: true, groups: false, channels: false, bots: false, excludeMuted: false, excludeRead: false, excludeArchived: true, include: [], exclude: [] },
        { id: 2, title: "Работа", emoticon: null, contacts: false, nonContacts: false, groups: true, channels: false, bots: false, excludeMuted: false, excludeRead: false, excludeArchived: true, include: [1003], exclude: [] },
        { id: 3, title: "Каналы", emoticon: null, contacts: false, nonContacts: false, groups: false, channels: true, bots: false, excludeMuted: false, excludeRead: false, excludeArchived: true, include: [], exclude: [] },
      ];
    case "search_chat":
      return annaChat.filter((m) => String(m.text).toLowerCase().includes(String(a.query).toLowerCase())).reverse();
    case "search_global":
      return annaChat
        .filter((m) => String(m.text).toLowerCase().includes(String(a.query).toLowerCase()))
        .map((m) => ({ message: m, peer: { kind: "user", title: "Анна Морозова", photoId: "p1001" } }));
    case "resolve_username":
      return { id: ANNA, kind: "user", title: "Анна Морозова", photoId: "p1001" };
    case "set_typing":
    case "set_active_chat":
    case "set_unread":
    case "mark_read":
    case "set_muted":
      return null;
    default:
      return null;
  }
});

// Images come back as data URLs; serve them as they are.
(window as unknown as { __TAURI_INTERNALS__: { convertFileSrc: (p: string) => string } }).__TAURI_INTERNALS__.convertFileSrc = (
  path: string,
) => path;

const params = new URLSearchParams(location.search);
const scene = params.get("scene") ?? "chat";
try {
  localStorage.setItem("oxigram.theme", scene === "light" ? "light" : "dark");
  localStorage.removeItem("oxigram.sidebarWidth");
  localStorage.removeItem("oxigram.infoDocked");
} catch {
  // Storage may be off; the defaults are fine.
}

await import("../src/main");

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Scenes for screenshots: ?scene=chat | context | menu | light | contacts | list
async function openAnna() {
  for (let i = 0; i < 100; i++) {
    const row = [...document.querySelectorAll<HTMLButtonElement>(".dialog")].find((b) =>
      b.textContent?.includes("Анна Морозова"),
    );
    if (row) {
      row.click();
      return;
    }
    await wait(50);
  }
}

if (scene !== "list") await openAnna();
for (let i = 0; i < 100 && document.querySelectorAll(".message").length < 5; i++) await wait(50);
await wait(400);
if (scene === "context") {
  const bubbles = [...document.querySelectorAll<HTMLElement>(".message")];
  const target = bubbles[bubbles.length - 3];
  const box = target?.getBoundingClientRect();
  if (target && box) {
    target.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: box.left + 60, clientY: box.top + 10 }),
    );
  }
} else if (scene === "typing") {
  const host = window as unknown as { demoEmit?: (events: unknown[]) => void };
  for (let i = 0; i < 100 && !host.demoEmit; i++) await wait(50);
  host.demoEmit?.([
    { type: "typing", peerId: ANNA, userId: ANNA, action: "typing" },
    { type: "typing", peerId: 1002, userId: 1003, action: "voice" },
  ]);
} else if (scene === "menu") {
  document.querySelector<HTMLButtonElement>("aside header .icon")?.click();
} else if (scene === "contacts") {
  document.querySelector<HTMLButtonElement>("aside header .icon")?.click();
  await wait(100);
  [...document.querySelectorAll<HTMLButtonElement>(".drawer button")].find((b) => b.textContent?.includes("Контакты"))?.click();
}
