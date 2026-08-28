import assert from "node:assert/strict";
import test from "node:test";

import {
  createViewerLoadingState,
  createViewerReadyState,
  viewerMediaKind,
  viewerStateAfterFailure,
} from "../src/windows/viewer-state.js";

const video = { kind: "video" };
const image = { kind: "image" };

test("unresolved viewer content never renders as image or video", () => {
  assert.equal(viewerMediaKind(createViewerLoadingState()), null);
  assert.equal(viewerMediaKind(viewerStateAfterFailure(null, "ready")), null);
});

test("viewer renders media only after asset metadata resolves", () => {
  assert.equal(viewerMediaKind(createViewerReadyState(video)), "video");
  assert.equal(viewerMediaKind(createViewerReadyState(image)), "image");
});

test("viewer distinguishes missing, unreadable, and playback failures", () => {
  assert.equal(viewerStateAfterFailure(video, "missing").kind, "missing");
  assert.equal(viewerStateAfterFailure(video, "unreadable").kind, "unreadable");
  assert.equal(
    viewerStateAfterFailure(video, "libraryUnavailable").libraryUnavailable,
    true,
  );
  assert.equal(viewerStateAfterFailure(null, null).availabilityUnknown, true);
  assert.equal(viewerStateAfterFailure(video, null).kind, "playbackFailed");
  assert.equal(viewerStateAfterFailure(video, "ready").kind, "playbackFailed");
});
