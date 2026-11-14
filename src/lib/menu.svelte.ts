export type MenuItem = {
  label: string;
  action: () => void;
  danger?: boolean;
  /** Key into the icon set of ContextMenu. */
  icon?: string;
};

/** A row of quick reactions shown at the top of the menu. */
export type MenuStrip = {
  emojis: string[];
  /** The emoji already chosen, highlighted. */
  active: string | null;
  pick: (emoji: string) => void;
};

/** The one context menu of the app; empty `items` means it is closed. */
export const contextMenu = $state<{
  items: MenuItem[];
  strip: MenuStrip | null;
  x: number;
  y: number;
}>({
  items: [],
  strip: null,
  x: 0,
  y: 0,
});

/** Opens the menu at the pointer. Call from an `oncontextmenu` handler. */
export function openMenu(event: MouseEvent, items: MenuItem[], strip: MenuStrip | null = null) {
  event.preventDefault();
  if (items.length === 0 && !strip) return;
  contextMenu.strip = strip;
  contextMenu.items = items;
  contextMenu.x = event.clientX;
  contextMenu.y = event.clientY;
}

export function closeMenu() {
  if (contextMenu.items.length > 0 || contextMenu.strip) {
    contextMenu.items = [];
    contextMenu.strip = null;
  }
}
