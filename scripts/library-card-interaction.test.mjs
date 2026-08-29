import assert from "node:assert/strict";
import test from "node:test";

import {
  getLibraryCardInteraction,
  getLibraryCardPrimaryAction,
} from "../src/windows/library-card-interaction.js";

test("an ordinary card click opens without showing card actions", () => {
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: false,
      selected: false,
      menuOpen: false,
      editingTitle: false,
      hovered: false,
    }),
    { opensOnClick: true, showsActions: false },
  );
});

test("hover reveals quick actions without changing direct-open behavior", () => {
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: false,
      selected: false,
      menuOpen: false,
      editingTitle: false,
      hovered: true,
    }),
    { opensOnClick: true, showsActions: true },
  );
});

test("rubber-band selection shows actions and prevents accidental opening", () => {
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: true,
      selected: true,
      menuOpen: false,
      editingTitle: false,
      hovered: false,
    }),
    { opensOnClick: false, showsActions: true },
  );
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: true,
      selected: false,
      menuOpen: false,
      editingTitle: false,
      hovered: false,
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
      hovered: false,
    }),
    { opensOnClick: false, showsActions: true },
  );
});

test("image quick action edits while media quick actions view", () => {
  assert.deepEqual(getLibraryCardPrimaryAction("image"), {
    icon: "pencil.tip",
    title: "Edit",
    opensEditor: true,
  });
  assert.deepEqual(getLibraryCardPrimaryAction("video"), {
    icon: "eye",
    title: "View",
    opensEditor: false,
  });
  assert.deepEqual(getLibraryCardPrimaryAction("gif"), {
    icon: "eye",
    title: "View",
    opensEditor: false,
  });
});
