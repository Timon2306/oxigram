// Per-computer preferences: remembered in localStorage, read reactively by
// the components that need them.

const KEY = "oxigram.settings";

export type TextSize = "small" | "normal" | "large";

type Stored = {
  textSize: TextSize;
  // What costs memory and traffic, each switchable in "Экономия памяти".
  /** GIFs and video stickers loop in place. */
  autoplayGifs: boolean;
  /** Profile photos; off shows the coloured letter only. */
  avatars: boolean;
  /** Pictures in chats load by themselves; off loads one on a click. */
  autoloadImages: boolean;
  /** Custom (premium) emoji reactions are drawn as pictures. */
  customEmoji: boolean;
  /** The chats at the top get their messages fetched ahead of time. */
  prefetchChats: boolean;
};

const DEFAULTS: Stored = {
  textSize: "normal",
  autoplayGifs: true,
  avatars: true,
  autoloadImages: true,
  customEmoji: false,
  prefetchChats: true,
};

function load(): Stored {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { ...DEFAULTS };
  }
}

export const settings = $state<Stored>(load());

const SCALE: Record<TextSize, number> = { small: 0.9, normal: 1, large: 1.12 };

/** Applies the text size to the whole interface. */
export function applyTextSize() {
  document.documentElement.style.setProperty("--scale", String(SCALE[settings.textSize]));
}

export function saveSettings() {
  applyTextSize();
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
  } catch {
    // Kept for this session only.
  }
}
