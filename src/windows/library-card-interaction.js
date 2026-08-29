export function getLibraryCardInteraction({
  selectionActive,
  selected,
  menuOpen,
  editingTitle,
}) {
  return {
    opensOnClick: !selectionActive && !menuOpen && !editingTitle,
    showsActions: selected || menuOpen,
  };
}
