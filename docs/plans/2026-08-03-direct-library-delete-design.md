# Direct Library Delete Design

## Goal

Remove the unnecessary menu step when deleting a capture from the library.

## Interaction

- Every active capture card shows a visible trash button beside Favorite.
- Clicking it moves the capture to Kiri's recoverable Trash immediately.
- Open and Show in Finder remain in the compact More menu.
- A trashed capture keeps visible Restore and Delete Permanently actions.
- Permanent deletion continues to require confirmation because it cannot be undone.

This keeps the frequent action direct while preserving a safe boundary around
irreversible deletion.
