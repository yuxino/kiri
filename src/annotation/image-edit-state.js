/** Pending textarea geometry is layout, not a user edit until committed. */
export function hasPendingImageTextChange(marks, draft) {
  if (!draft?.editing) return false;
  const previous = marks.find(mark => mark.id === draft.previousId);
  if (!draft.mark) return Boolean(previous);
  if (!previous) return true;
  return ["text", "color", "background", "fontSize"].some(key => previous[key] !== draft.mark[key]);
}

/** Undoing back to the saved marks/crop removes the close warning. */
export function hasUnsavedImageChanges(saved, marks, crop, draft) {
  return JSON.stringify(saved.marks) !== JSON.stringify(marks)
    || JSON.stringify(saved.crop) !== JSON.stringify(crop)
    || hasPendingImageTextChange(marks, draft);
}
