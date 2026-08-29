import assert from "node:assert/strict";
import test from "node:test";

import {
  getAvailableShortcutLabel,
  getLibraryBandRect,
  getLibraryCardInteraction,
  getLibraryCardPrimaryAction,
  getLibraryContentPoint,
  getMenuFocusIndex,
} from "../src/windows/library-card-interaction.js";

test("an ordinary card click opens without showing card actions", () => {
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: false,
      selected: false,
      menuOpen: false,
      editingTitle: false,
      highlighted: false,
    }),
    { opensOnClick: true, showsActions: false },
  );
});

test("pointer hover or keyboard focus reveals quick actions without changing direct-open behavior", () => {
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: false,
      selected: false,
      menuOpen: false,
      editingTitle: false,
      highlighted: true,
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
      highlighted: false,
    }),
    { opensOnClick: false, showsActions: true },
  );
  assert.deepEqual(
    getLibraryCardInteraction({
      selectionActive: true,
      selected: false,
      menuOpen: false,
      editingTitle: false,
      highlighted: false,
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
      highlighted: false,
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

test("card menus support native arrow and edge keyboard navigation", () => {
  assert.equal(getMenuFocusIndex("ArrowDown", 0, 4), 1);
  assert.equal(getMenuFocusIndex("ArrowDown", 3, 4), 0);
  assert.equal(getMenuFocusIndex("ArrowUp", 0, 4), 3);
  assert.equal(getMenuFocusIndex("Home", 2, 4), 0);
  assert.equal(getMenuFocusIndex("End", 1, 4), 3);
  assert.equal(getMenuFocusIndex("ArrowDown", -1, 4), 0);
  assert.equal(getMenuFocusIndex("ArrowDown", 0, 0), -1);
});

test("rubber-band pointer coordinates do not count container padding twice", () => {
  assert.deepEqual(
    getLibraryContentPoint({
      clientX: 922,
      clientY: 460,
      rectLeft: 100,
      rectTop: 40,
      clientLeft: 1,
      clientTop: 1,
      scrollLeft: 0,
      scrollTop: 320,
    }),
    { x: 821, y: 739 },
  );
});

test("rubber-band geometry is normalized in either drag direction", () => {
  assert.deepEqual(getLibraryBandRect({ x0: 821, y0: 739, x1: 220, y1: 410 }), {
    x: 220,
    y: 410,
    w: 601,
    h: 329,
  });
});

test("empty library advertises only an available global shortcut", () => {
  assert.equal(
    getAvailableShortcutLabel({ label: "⇧⌘A", status: "enabled" }),
    "⇧⌘A",
  );
  assert.equal(
    getAvailableShortcutLabel({ label: "⇧⌘A", status: "occupied" }),
    null,
  );
  assert.equal(getAvailableShortcutLabel(null), null);
});
