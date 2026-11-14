/** One short message at a time, hidden by itself after a moment. */
export const toastState = $state({ text: "" });

let timer: ReturnType<typeof setTimeout> | undefined;

export function showToast(text: string) {
  toastState.text = text;
  clearTimeout(timer);
  timer = setTimeout(() => (toastState.text = ""), 2500);
}
