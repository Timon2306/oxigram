// Light or dark look, remembered per computer. Applied before the first
// paint (from main.ts) so the window never flashes the other theme.

export type Theme = "dark" | "light";

const KEY = "oxigram.theme";

export function savedTheme(): Theme {
  try {
    return localStorage.getItem(KEY) === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // Not remembered; the choice still holds until restart.
  }
}
