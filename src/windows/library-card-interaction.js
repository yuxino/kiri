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

export function getLibraryContentPoint({
  clientX,
  clientY,
  rectLeft,
  rectTop,
  clientLeft,
  clientTop,
  scrollLeft,
  scrollTop,
}) {
  return {
    x: clientX - rectLeft - clientLeft + scrollLeft,
    y: clientY - rectTop - clientTop + scrollTop,
  };
}

export function getLibraryBandRect({ x0, y0, x1, y1 }) {
  return {
    x: Math.min(x0, x1),
    y: Math.min(y0, y1),
    w: Math.abs(x1 - x0),
    h: Math.abs(y1 - y0),
  };
}

export function getAvailableShortcutLabel(shortcutStatus) {
  return shortcutStatus?.status === "enabled" && shortcutStatus.label
    ? shortcutStatus.label
    : null;
}
