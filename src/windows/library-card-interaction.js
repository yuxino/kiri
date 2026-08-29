export function getLibraryCardInteraction({
  selectionActive,
  selected,
  menuOpen,
  editingTitle,
  hovered,
}) {
  return {
    opensOnClick: !selectionActive && !menuOpen && !editingTitle,
    showsActions: hovered || selected || menuOpen,
  };
}

export function getLibraryCardPrimaryAction(kind) {
  return kind === "image"
    ? { icon: "pencil.tip", title: "Edit", opensEditor: true }
    : { icon: "eye", title: "View", opensEditor: false };
}
