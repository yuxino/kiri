export function getLibraryCardInteraction(input: {
  selectionActive: boolean;
  selected: boolean;
  menuOpen: boolean;
  editingTitle: boolean;
  highlighted: boolean;
}): {
  opensOnClick: boolean;
  showsActions: boolean;
};

export function getLibraryCardPrimaryAction(kind: "image" | "video" | "gif"): {
  icon: "pencil.tip" | "eye";
  title: "Edit" | "View";
  opensEditor: boolean;
};

export function getMenuFocusIndex(
  key: "ArrowDown" | "ArrowUp" | "Home" | "End",
  current: number,
  itemCount: number,
): number;

export function getLibraryContentPoint(input: {
  clientX: number;
  clientY: number;
  rectLeft: number;
  rectTop: number;
  clientLeft: number;
  clientTop: number;
  scrollLeft: number;
  scrollTop: number;
}): { x: number; y: number };

export function getLibraryBandRect(input: {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}): { x: number; y: number; w: number; h: number };

export function getAvailableShortcutLabel(shortcutStatus: {
  label: string;
  status: "enabled" | "occupied";
} | null): string | null;
