export function getLibraryCardInteraction({
  selectionActive,
  selected,
  menuOpen,
  editingTitle,
  highlighted,
}) {
  return {
    opensOnClick: !selectionActive && !menuOpen && !editingTitle,
    showsActions: highlighted || selected || menuOpen,
  };
}

export function getLibraryCardPrimaryAction(kind) {
  return kind === "image"
    ? { icon: "pencil.tip", title: "Edit", opensEditor: true }
    : { icon: "eye", title: "View", opensEditor: false };
}

export function getMenuFocusIndex(key, current, itemCount) {
  if (itemCount <= 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return itemCount - 1;
  if (key === "ArrowDown") return (current + 1 + itemCount) % itemCount;
  if (key === "ArrowUp") return (current - 1 + itemCount) % itemCount;
  return current;
}
