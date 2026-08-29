import assert from "node:assert/strict";
import test from "node:test";

import { getLibraryCardInteraction } from "../src/windows/library-card-interaction.js";

test("an ordinary card click opens without showing card actions", () => {
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: false,
      selected: false,
      menuOpen: false,
      editingTitle: false,
    }),
    { opensOnClick: true, showsActions: false },
  );
});

test("rubber-band selection shows actions and prevents accidental opening", () => {
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: true,
      selected: true,
      menuOpen: false,
      editingTitle: false,
    }),
    { opensOnClick: false, showsActions: true },
  );
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: true,
      selected: false,
      menuOpen: false,
      editingTitle: false,
    }),
    { opensOnClick: false, showsActions: false },
  );
});

test("a context menu can reveal its card actions without entering selection", () => {
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: false,
      selected: false,
      menuOpen: true,
      editingTitle: false,
    }),
    { opensOnClick: false, showsActions: true },
  );
});
