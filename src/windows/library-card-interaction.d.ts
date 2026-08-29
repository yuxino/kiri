export function getLibraryCardInteraction(input: {
  selectionActive: boolean;
  selected: boolean;
  menuOpen: boolean;
  editingTitle: boolean;
}): {
  opensOnClick: boolean;
  showsActions: boolean;
};
