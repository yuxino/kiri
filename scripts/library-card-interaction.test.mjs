import assert from "node:assert/strict";
import test from "node:test";
import {
  createLibraryHarness,
  deferred,
  nodes,
  settleRequests,
  testAsset,
} from "./helpers/library-render-harness.mjs";

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

function cardProps(overrides = {}) {
  return {
    asset: testAsset,
    thumbnailRevision: 0,
    menuOpen: false,
    menu: null,
    selected: false,
    selectionActive: false,
    onMenu() {},
    onOpen() {},
    registerRef() {},
    onAvailability() {},
    async onRestoreMissing() {},
    ...overrides,
  };
}

function preview(tree) {
  return nodes(tree).find((node) => node?.type === "img" || node?.type === "video");
}

for (const kind of ["image", "video", "gif"]) {
  test(`${kind} preview failure clears when updated content arrives without resetting card editing`, async () => {
    const harness = createLibraryHarness();
    const props = cardProps({ asset: { ...testAsset, kind } });
    const card = harness.mount("AssetCard", props);
    preview(card.render()).props.onError();
    await settleRequests();
    assert.ok(nodes(card.render()).includes("Preview unavailable"));

    harness.window.dispatchEvent({ type: `kiri-rename:${testAsset.id}` });
    assert.ok(nodes(card.render()).some((node) => node?.type === "input"));
    const updated = card.render({ ...props, thumbnailRevision: 1, availability: "ready" });
    assert.ok(preview(updated), "updated content must remount the preview");
    assert.ok(!nodes(updated).includes("Preview unavailable"));
    assert.ok(nodes(updated).some((node) => node?.type === "input"), "rename state must survive");
    card.unmount();
  });
}

for (const result of ["missing", "rejected"]) {
  test(`an old ${result} availability response cannot overwrite updated content`, async () => {
    const request = deferred();
    const reports = [];
    const harness = createLibraryHarness({ getAssetAvailability: () => request.promise });
    const props = cardProps({ onAvailability: (status) => reports.push(status) });
    const card = harness.mount("AssetCard", props);
    preview(card.render()).props.onError();
    card.render({ ...props, thumbnailRevision: 1, availability: "ready" });
    if (result === "rejected") request.reject(new Error("old request failed"));
    else request.resolve({ status: result });
    await settleRequests();
    assert.deepEqual(reports, []);
    assert.ok(preview(card.render()));
    card.unmount();
  });
}

test("only the newest availability request can publish a result", async () => {
  const oldRequest = deferred();
  const newRequest = deferred();
  const requests = [oldRequest, newRequest];
  const reports = [];
  const harness = createLibraryHarness({ getAssetAvailability: () => requests.shift().promise });
  const card = harness.mount("AssetCard", cardProps({ onAvailability: (status) => reports.push(status) }));
  const onError = preview(card.render()).props.onError;
  onError();
  onError();
  newRequest.resolve({ status: "missing" });
  await settleRequests();
  oldRequest.resolve({ status: "ready" });
  await settleRequests();
  assert.deepEqual(reports, ["missing"]);
  assert.ok(!nodes(card.render()).includes("Preview unavailable"));
  card.unmount();
});

test("removed cards cannot publish a pending availability result", async () => {
  const request = deferred();
  const reports = [];
  const harness = createLibraryHarness({ getAssetAvailability: () => request.promise });
  const card = harness.mount("AssetCard", cardProps({ onAvailability: (status) => reports.push(status) }));
  preview(card.render()).props.onError();
  card.unmount();
  request.resolve({ status: "missing" });
  await settleRequests();
  assert.deepEqual(reports, []);
});

test("content events reject old card reports even before the next render", async () => {
  const harness = createLibraryHarness();
  const library = harness.mount("LibraryWindow", {});
  library.render();
  await settleRequests();
  const card = nodes(library.render()).find((node) => node?.type?.name === "AssetCard");
  assert.ok(card);
  harness.emit("assetContentChanged", testAsset.id);
  card.props.onAvailability("missing");
  const updated = nodes(library.render()).find((node) => node?.type?.name === "AssetCard");
  assert.equal(updated.props.thumbnailRevision, 1);
  assert.equal(updated.props.availability, "ready");
  library.unmount();
});

test("a detached preview error cannot start a check for the new revision", async () => {
  let requests = 0;
  const harness = createLibraryHarness({
    getAssetAvailability: async () => { requests++; return { status: "missing" }; },
  });
  const props = cardProps();
  const card = harness.mount("AssetCard", props);
  const oldError = preview(card.render()).props.onError;
  card.render({ ...props, thumbnailRevision: 1, availability: "ready" });
  oldError();
  await settleRequests();
  assert.equal(requests, 0);
  assert.ok(preview(card.render()));
  card.unmount();
});

for (const outcome of ["ready", "rejected"]) {
  test(`preview retry settles its busy state when the check is ${outcome}`, async () => {
    const retry = deferred();
    let calls = 0;
    const harness = createLibraryHarness({
      getAssetAvailability: () => ++calls === 1 ? Promise.resolve({ status: "ready" }) : retry.promise,
    });
    const card = harness.mount("AssetCard", cardProps());
    const firstKey = preview(card.render()).props.key;
    preview(card.render()).props.onError();
    await settleRequests();
    const retryButton = (tree) => nodes(tree).find((node) =>
      node?.type === "button" && nodes(node).includes("Retry"));
    retryButton(card.render()).props.onClick({ stopPropagation() {} });
    assert.equal(retryButton(card.render()).props.disabled, true);
    if (outcome === "ready") retry.resolve({ status: "ready" });
    else retry.reject(new Error("availability unavailable"));
    await settleRequests();
    const tree = card.render();
    if (outcome === "ready") {
      const retriedPreview = preview(tree);
      assert.ok(retriedPreview);
      assert.notEqual(retriedPreview.props.key, firstKey);
    } else {
      assert.ok(nodes(tree).includes("Preview unavailable"));
      assert.equal(retryButton(tree).props.disabled, false);
    }
    card.unmount();
  });
}
