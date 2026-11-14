// Per-computer preferences: remembered in localStorage, read reactively by
// the components that need them.

const KEY = "oxigram.settings";

export type TextSize = "small" | "normal" | "large";

type Stored = { textSize: TextSize; autoplayGifs: boolean };

const DEFAULTS: Stored = { textSize: "normal", autoplayGifs: true };

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
