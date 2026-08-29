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
